use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::token::{self, Mint, Token, TokenAccount, MintTo};
use anchor_spl::associated_token::AssociatedToken;
use crate::state::*;
use crate::constants::*;
use crate::errors::VaultpadError;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CreateLaunchParams {
    pub name: String,
    pub ticker: String,
    pub uri: String,
    pub category: u8,
    pub fee_redirect_type: u8,
    pub fee_redirect_handle: String,
    pub charity_authority: Option<Pubkey>,
}

#[derive(Accounts)]
#[instruction(params: CreateLaunchParams)]
pub struct CreateLaunch<'info> {
    #[account(mut, seeds = [PROTOCOL_SEED], bump = protocol_config.bump)]
    pub protocol_config: Account<'info, ProtocolConfig>,
    #[account(init, payer = creator, space = 8 + LaunchState::INIT_SPACE, seeds = [LAUNCH_SEED, mint.key().as_ref()], bump)]
    pub launch_state: Account<'info, LaunchState>,
    #[account(init, payer = creator, mint::decimals = TOKEN_DECIMALS, mint::authority = launch_state)]
    pub mint: Account<'info, Mint>,
    #[account(init, payer = creator, associated_token::mint = mint, associated_token::authority = launch_state)]
    pub token_vault: Account<'info, TokenAccount>,
    /// CHECK: SOL vault PDA
    #[account(mut, seeds = [SOL_VAULT_SEED, mint.key().as_ref()], bump)]
    pub sol_vault: SystemAccount<'info>,
    /// CHECK: Charity PDA
    #[account(mut, seeds = [CHARITY_SEED, mint.key().as_ref()], bump)]
    pub charity_wallet: SystemAccount<'info>,
    /// CHECK: platform wallet
    #[account(mut, constraint = platform_wallet.key() == protocol_config.platform_wallet)]
    pub platform_wallet: SystemAccount<'info>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<CreateLaunch>, params: CreateLaunchParams) -> Result<()> {
    require!(params.name.len() <= MAX_NAME_LEN, VaultpadError::NameTooLong);
    require!(params.ticker.len() <= MAX_TICKER_LEN, VaultpadError::TickerTooLong);
    require!(params.uri.len() <= MAX_URI_LEN, VaultpadError::UriTooLong);
    require!(params.fee_redirect_handle.len() <= MAX_HANDLE_LEN, VaultpadError::HandleTooLong);
    require!(params.category <= 3, VaultpadError::InvalidCategory);

    // Pay launch cost
    system_program::transfer(
        CpiContext::new(ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.creator.to_account_info(),
                to: ctx.accounts.platform_wallet.to_account_info(),
            }),
        ctx.accounts.protocol_config.launch_cost,
    )?;

    // Mint full supply
    let supply = (TOKEN_SUPPLY as u64)
        .checked_mul(10u64.pow(TOKEN_DECIMALS as u32))
        .ok_or(VaultpadError::MathOverflow)?;

    let mint_key = ctx.accounts.mint.key();
    let bump = ctx.bumps.launch_state;
    let seeds: &[&[&[u8]]] = &[&[LAUNCH_SEED, mint_key.as_ref(), &[bump]]];

    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.token_vault.to_account_info(),
                authority: ctx.accounts.launch_state.to_account_info(),
            },
            seeds,
        ),
        supply,
    )?;

    // Randomized creator lock
    let clock = Clock::get()?;
    let cb = ctx.accounts.creator.key().to_bytes();
    let seed = clock.slot.wrapping_mul(clock.unix_timestamp as u64)
        .wrapping_add(u64::from_le_bytes(cb[0..8].try_into().unwrap()));
    let range = (CREATOR_LOCK_MAX - CREATOR_LOCK_MIN) as u64;
    let creator_lock_dur = CREATOR_LOCK_MIN + (seed % range) as i64;

    let locked = supply.checked_mul(CREATOR_LOCK_PERCENT).unwrap().checked_div(100).unwrap();
    let curve_tokens = supply.checked_sub(locked).unwrap();

    let is_cause = params.category == CATEGORY_CAUSE;
    let charity_auth = if is_cause { params.charity_authority.unwrap_or(ctx.accounts.creator.key()) } else { Pubkey::default() };
    let charity_key = if is_cause { ctx.accounts.charity_wallet.key() } else { Pubkey::default() };

    let l = &mut ctx.accounts.launch_state;
    l.mint = ctx.accounts.mint.key();
    l.creator = ctx.accounts.creator.key();
    l.name = params.name;
    l.ticker = params.ticker;
    l.uri = params.uri;
    l.category = params.category;
    l.fee_redirect_type = params.fee_redirect_type;
    l.fee_redirect_handle = params.fee_redirect_handle;
    l.virtual_sol_reserves = INITIAL_VIRTUAL_SOL;
    l.virtual_token_reserves = supply;
    l.real_sol_reserves = 0;
    l.real_token_reserves = curve_tokens;
    l.graduated = false;
    l.total_supply = supply;
    l.creator_lock_unlock_time = clock.unix_timestamp + creator_lock_dur;
    l.creator_lock_claimed = false;
    l.creator_locked_tokens = locked;
    l.charity_wallet = charity_key;
    l.charity_authority = charity_auth;
    l.volume = 0;
    l.fees_generated = 0;
    l.trade_count = 0;
    l.total_locks_created = 0;
    l.penalty_locks_issued = 0;
    l.created_at = clock.unix_timestamp;
    l.last_buy_slot = 0;
    l.buys_in_current_slot = 0;
    l.bump = bump;
    l.sol_vault_bump = ctx.bumps.sol_vault;
    l._reserved = [0u8; 32];

    let c = &mut ctx.accounts.protocol_config;
    c.total_launches = c.total_launches.checked_add(1).unwrap();

    msg!("Launch: {} (${}), creator lock ~{}h, max wallet 4%",
        l.name, l.ticker, creator_lock_dur / 3600);
    Ok(())
}
