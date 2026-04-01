use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::token::{Token, TokenAccount};
use anchor_spl::associated_token::AssociatedToken;
use crate::state::*;
use crate::constants::*;
use crate::errors::VaultpadError;

#[derive(Accounts)]
pub struct Buy<'info> {
    #[account(mut, seeds = [PROTOCOL_SEED], bump = protocol_config.bump)]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [LAUNCH_SEED, launch_state.mint.as_ref()],
        bump = launch_state.bump,
        constraint = !launch_state.graduated @ VaultpadError::AlreadyGraduated,
    )]
    pub launch_state: Account<'info, LaunchState>,

    /// CHECK: validated via launch_state.mint
    pub mint: AccountInfo<'info>,

    #[account(mut, associated_token::mint = mint, associated_token::authority = launch_state)]
    pub token_vault: Account<'info, TokenAccount>,

    /// CHECK: SOL vault PDA
    #[account(mut, seeds = [SOL_VAULT_SEED, launch_state.mint.as_ref()], bump = launch_state.sol_vault_bump)]
    pub sol_vault: SystemAccount<'info>,

    /// Per-buyer lock escrow (new one each buy)
    #[account(
        init, payer = buyer,
        space = 8 + BuyerLock::INIT_SPACE,
        seeds = [LOCK_SEED, launch_state.mint.as_ref(), buyer.key().as_ref(), launch_state.total_locks_created.to_le_bytes().as_ref()],
        bump,
    )]
    pub buyer_lock: Account<'info, BuyerLock>,

    /// Per-wallet tracker: tracks total holdings + funder for this launch.
    /// Initialized on first buy, updated on subsequent buys.
    #[account(
        init_if_needed, payer = buyer,
        space = 8 + WalletTracker::INIT_SPACE,
        seeds = [WALLET_TRACKER_SEED, launch_state.mint.as_ref(), buyer.key().as_ref()],
        bump,
    )]
    pub wallet_tracker: Account<'info, WalletTracker>,

    /// The account that originally funded the buyer's wallet.
    /// Passed by the client. The program records this on first buy
    /// and uses it for same-funder detection on subsequent buys.
    /// CHECK: we just read the pubkey, no lamport manipulation
    pub funder_account: AccountInfo<'info>,

    /// CHECK: platform wallet
    #[account(mut, constraint = platform_wallet.key() == protocol_config.platform_wallet)]
    pub platform_wallet: SystemAccount<'info>,

    /// CHECK: creator wallet (receives fees, but CANNOT buy)
    #[account(mut, constraint = creator_wallet.key() == launch_state.creator)]
    pub creator_wallet: SystemAccount<'info>,

    /// CHECK: charity PDA
    #[account(mut, seeds = [CHARITY_SEED, launch_state.mint.as_ref()], bump)]
    pub charity_wallet: SystemAccount<'info>,

    #[account(mut)]
    pub buyer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Buy>, amount_in: u64, min_tokens_out: u64) -> Result<()> {
    require!(amount_in > 0, VaultpadError::InvalidAmount);

    let launch = &ctx.accounts.launch_state;

    // ═══════════════════════════════════════════════════════════
    // ANTI-MANIPULATION CHECK 1: Creator cannot buy their own token
    // ═══════════════════════════════════════════════════════════
    require!(
        ctx.accounts.buyer.key() != launch.creator,
        VaultpadError::CreatorCannotBuy
    );

    let config = &ctx.accounts.protocol_config;

    // ─── Calculate fees ─────────────────────────────────────
    let platform_fee = (amount_in as u128)
        .checked_mul(config.platform_fee_bps as u128).unwrap()
        .checked_div(10_000).unwrap() as u64;
    let creator_fee = (amount_in as u128)
        .checked_mul(config.creator_fee_bps as u128).unwrap()
        .checked_div(10_000).unwrap() as u64;
    let net_sol = amount_in
        .checked_sub(platform_fee).ok_or(VaultpadError::MathOverflow)?
        .checked_sub(creator_fee).ok_or(VaultpadError::MathOverflow)?;

    // ─── Bonding curve ──────────────────────────────────────
    let launch = &ctx.accounts.launch_state;
    let tokens_out = launch.calculate_buy(net_sol)?;
    require!(tokens_out >= min_tokens_out, VaultpadError::SlippageExceeded);
    require!(tokens_out > 0, VaultpadError::CurveDepleted);

    // ═══════════════════════════════════════════════════════════
    // ANTI-MANIPULATION CHECK 2: Max wallet cap (4%)
    // ═══════════════════════════════════════════════════════════
    let tracker = &ctx.accounts.wallet_tracker;
    let max_tokens = launch.max_wallet_tokens();
    let new_total = tracker.total_purchased
        .checked_add(tokens_out)
        .ok_or(VaultpadError::MathOverflow)?;
    require!(new_total <= max_tokens, VaultpadError::MaxWalletExceeded);

    // ═══════════════════════════════════════════════════════════
    // ANTI-MANIPULATION CHECK 3: Same-block detection
    // If 2+ wallets buy in the same slot → PENALTY LOCK (72h random)
    // ═══════════════════════════════════════════════════════════
    let clock = Clock::get()?;
    let current_slot = clock.slot;
    let launch_mut = &mut ctx.accounts.launch_state;

    let mut is_same_block = false;
    if launch_mut.last_buy_slot == current_slot {
        // Another wallet already bought in this slot
        launch_mut.buys_in_current_slot = launch_mut.buys_in_current_slot.saturating_add(1);
        if launch_mut.buys_in_current_slot >= SAME_BLOCK_THRESHOLD {
            is_same_block = true;
        }
    } else {
        // New slot, reset counter
        launch_mut.last_buy_slot = current_slot;
        launch_mut.buys_in_current_slot = 1;
    }

    // ═══════════════════════════════════════════════════════════
    // ANTI-MANIPULATION CHECK 4: Same-funder detection
    // If buyer's funder matches another wallet's funder → PENALTY
    // ═══════════════════════════════════════════════════════════
    let tracker_mut = &mut ctx.accounts.wallet_tracker;
    let funder_key = ctx.accounts.funder_account.key();

    let is_same_funder;
    if tracker_mut.buy_count == 0 {
        // First buy: record the funder
        tracker_mut.mint = launch_mut.mint;
        tracker_mut.wallet = ctx.accounts.buyer.key();
        tracker_mut.funder = funder_key;
        tracker_mut.flagged = false;
        tracker_mut.bump = ctx.bumps.wallet_tracker;
        is_same_funder = false;
    } else {
        // Subsequent buy: check if funder changed or is flagged
        is_same_funder = tracker_mut.flagged;
    }

    // ═══════════════════════════════════════════════════════════
    // DETERMINE LOCK DURATION
    // Normal: 5-30 min random
    // Penalty (same-block or same-funder): 0-72h random
    // ═══════════════════════════════════════════════════════════
    let is_penalty = is_same_block || is_same_funder;

    let buyer_bytes = ctx.accounts.buyer.key().to_bytes();
    let lock_seed = clock.slot
        .wrapping_mul(clock.unix_timestamp as u64)
        .wrapping_add(u64::from_le_bytes(buyer_bytes[0..8].try_into().unwrap()))
        .wrapping_add(launch_mut.total_locks_created);

    let (lock_min, lock_max) = if is_penalty {
        (PENALTY_LOCK_MIN, PENALTY_LOCK_MAX)
    } else {
        (BUYER_LOCK_MIN, BUYER_LOCK_MAX)
    };

    let range = (lock_max - lock_min) as u64;
    let lock_duration = if range > 0 {
        lock_min + (lock_seed % range) as i64
    } else {
        lock_min
    };
    let unlock_time = clock.unix_timestamp + lock_duration;

    if is_penalty {
        msg!("⚠ PENALTY LOCK: same-block={}, same-funder={}, lock={}h",
            is_same_block, is_same_funder, lock_duration / 3600);
        launch_mut.penalty_locks_issued = launch_mut.penalty_locks_issued.saturating_add(1);
        tracker_mut.flagged = true;
    }

    // ─── SOL transfers ──────────────────────────────────────
    system_program::transfer(
        CpiContext::new(ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.buyer.to_account_info(),
                to: ctx.accounts.sol_vault.to_account_info(),
            }),
        net_sol,
    )?;

    if platform_fee > 0 {
        system_program::transfer(
            CpiContext::new(ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.buyer.to_account_info(),
                    to: ctx.accounts.platform_wallet.to_account_info(),
                }),
            platform_fee,
        )?;
    }

    if creator_fee > 0 {
        let dest = if launch_mut.category == CATEGORY_CAUSE {
            ctx.accounts.charity_wallet.to_account_info()
        } else {
            ctx.accounts.creator_wallet.to_account_info()
        };
        system_program::transfer(
            CpiContext::new(ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.buyer.to_account_info(),
                    to: dest,
                }),
            creator_fee,
        )?;
    }

    // ─── Create buyer lock (tokens stay in vault) ───────────
    let lock = &mut ctx.accounts.buyer_lock;
    lock.mint = launch_mut.mint;
    lock.buyer = ctx.accounts.buyer.key();
    lock.lock_id = launch_mut.total_locks_created;
    lock.amount = tokens_out;
    lock.unlock_time = unlock_time;
    lock.claimed = false;
    lock.created_at = clock.unix_timestamp;
    lock.is_penalty = is_penalty;
    lock.bump = ctx.bumps.buyer_lock;

    // ─── Update wallet tracker ──────────────────────────────
    tracker_mut.total_purchased = new_total;
    tracker_mut.locked_balance = tracker_mut.locked_balance
        .checked_add(tokens_out).ok_or(VaultpadError::MathOverflow)?;
    tracker_mut.buy_count = tracker_mut.buy_count.saturating_add(1);

    // ─── Update curve ───────────────────────────────────────
    launch_mut.virtual_sol_reserves = launch_mut.virtual_sol_reserves
        .checked_add(net_sol).ok_or(VaultpadError::MathOverflow)?;
    launch_mut.virtual_token_reserves = launch_mut.virtual_token_reserves
        .checked_sub(tokens_out).ok_or(VaultpadError::MathOverflow)?;
    launch_mut.real_sol_reserves = launch_mut.real_sol_reserves
        .checked_add(net_sol).ok_or(VaultpadError::MathOverflow)?;
    launch_mut.real_token_reserves = launch_mut.real_token_reserves
        .checked_sub(tokens_out).ok_or(VaultpadError::MathOverflow)?;

    let total_fee = platform_fee.checked_add(creator_fee).unwrap();
    launch_mut.volume = launch_mut.volume.checked_add(amount_in).unwrap();
    launch_mut.fees_generated = launch_mut.fees_generated.checked_add(total_fee).unwrap();
    launch_mut.trade_count = launch_mut.trade_count.checked_add(1).unwrap();
    launch_mut.total_locks_created = launch_mut.total_locks_created.checked_add(1).unwrap();

    let c = &mut ctx.accounts.protocol_config;
    c.total_volume = c.total_volume.checked_add(amount_in).unwrap();
    c.total_fees_collected = c.total_fees_collected.checked_add(total_fee).unwrap();

    msg!("Buy: {} SOL -> {} tokens | Lock #{} {}m | penalty={}",
        amount_in, tokens_out, lock.lock_id, lock_duration / 60, is_penalty);

    Ok(())
}
