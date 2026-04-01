use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use anchor_spl::associated_token::AssociatedToken;
use crate::state::*;
use crate::constants::*;
use crate::errors::VaultpadError;

#[derive(Accounts)]
pub struct ClaimBuyerLock<'info> {
    #[account(seeds = [LAUNCH_SEED, launch_state.mint.as_ref()], bump = launch_state.bump)]
    pub launch_state: Account<'info, LaunchState>,

    #[account(
        mut,
        seeds = [LOCK_SEED, buyer_lock.mint.as_ref(), buyer_lock.buyer.as_ref(), buyer_lock.lock_id.to_le_bytes().as_ref()],
        bump = buyer_lock.bump,
        constraint = buyer_lock.buyer == buyer.key() @ VaultpadError::Unauthorized,
        constraint = buyer_lock.mint == launch_state.mint @ VaultpadError::Unauthorized,
        constraint = !buyer_lock.claimed @ VaultpadError::AlreadyClaimed,
    )]
    pub buyer_lock: Account<'info, BuyerLock>,

    #[account(
        mut,
        seeds = [WALLET_TRACKER_SEED, launch_state.mint.as_ref(), buyer.key().as_ref()],
        bump = wallet_tracker.bump,
    )]
    pub wallet_tracker: Account<'info, WalletTracker>,

    /// CHECK: validated
    pub mint: AccountInfo<'info>,

    #[account(mut, associated_token::mint = mint, associated_token::authority = launch_state)]
    pub token_vault: Account<'info, TokenAccount>,

    #[account(init_if_needed, payer = buyer, associated_token::mint = mint, associated_token::authority = buyer)]
    pub buyer_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub buyer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<ClaimBuyerLock>) -> Result<()> {
    let lock = &ctx.accounts.buyer_lock;
    let clock = Clock::get()?;
    require!(clock.unix_timestamp >= lock.unlock_time, VaultpadError::LockNotExpired);
    require!(lock.amount > 0, VaultpadError::LockEmpty);

    let amount = lock.amount;

    let mint_key = ctx.accounts.launch_state.mint;
    let bump = ctx.accounts.launch_state.bump;
    let seeds: &[&[&[u8]]] = &[&[LAUNCH_SEED, mint_key.as_ref(), &[bump]]];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.token_vault.to_account_info(),
                to: ctx.accounts.buyer_token_account.to_account_info(),
                authority: ctx.accounts.launch_state.to_account_info(),
            },
            seeds,
        ),
        amount,
    )?;

    // Update lock
    let lock_mut = &mut ctx.accounts.buyer_lock;
    lock_mut.claimed = true;

    // Update wallet tracker
    let tracker = &mut ctx.accounts.wallet_tracker;
    tracker.locked_balance = tracker.locked_balance.saturating_sub(amount);
    tracker.claimed_balance = tracker.claimed_balance.checked_add(amount)
        .ok_or(VaultpadError::MathOverflow)?;

    msg!("Claimed lock #{}: {} tokens (penalty={})", lock_mut.lock_id, amount, lock_mut.is_penalty);
    Ok(())
}
