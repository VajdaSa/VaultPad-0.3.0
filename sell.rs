use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use crate::state::*;
use crate::constants::*;
use crate::errors::VaultpadError;

#[derive(Accounts)]
pub struct Sell<'info> {
    #[account(mut, seeds = [PROTOCOL_SEED], bump = protocol_config.bump)]
    pub protocol_config: Account<'info, ProtocolConfig>,
    #[account(mut, seeds = [LAUNCH_SEED, launch_state.mint.as_ref()], bump = launch_state.bump, constraint = !launch_state.graduated @ VaultpadError::AlreadyGraduated)]
    pub launch_state: Account<'info, LaunchState>,
    /// CHECK: validated
    pub mint: AccountInfo<'info>,
    #[account(mut, associated_token::mint = mint, associated_token::authority = launch_state)]
    pub token_vault: Account<'info, TokenAccount>,
    /// CHECK: SOL vault
    #[account(mut, seeds = [SOL_VAULT_SEED, launch_state.mint.as_ref()], bump = launch_state.sol_vault_bump)]
    pub sol_vault: SystemAccount<'info>,
    #[account(mut, associated_token::mint = mint, associated_token::authority = seller)]
    pub seller_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [WALLET_TRACKER_SEED, launch_state.mint.as_ref(), seller.key().as_ref()],
        bump = wallet_tracker.bump,
    )]
    pub wallet_tracker: Account<'info, WalletTracker>,
    /// CHECK: platform
    #[account(mut, constraint = platform_wallet.key() == protocol_config.platform_wallet)]
    pub platform_wallet: SystemAccount<'info>,
    /// CHECK: creator
    #[account(mut, constraint = creator_wallet.key() == launch_state.creator)]
    pub creator_wallet: SystemAccount<'info>,
    /// CHECK: charity
    #[account(mut, seeds = [CHARITY_SEED, launch_state.mint.as_ref()], bump)]
    pub charity_wallet: SystemAccount<'info>,
    #[account(mut)]
    pub seller: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Sell>, tokens_in: u64, min_sol_out: u64) -> Result<()> {
    require!(tokens_in > 0, VaultpadError::InvalidAmount);
    require!(ctx.accounts.seller_token_account.amount >= tokens_in, VaultpadError::InsufficientTokens);

    let config = &ctx.accounts.protocol_config;
    let launch = &mut ctx.accounts.launch_state;

    let gross_sol = launch.calculate_sell(tokens_in)?;
    let platform_fee = (gross_sol as u128).checked_mul(config.platform_fee_bps as u128).unwrap().checked_div(10_000).unwrap() as u64;
    let creator_fee = (gross_sol as u128).checked_mul(config.creator_fee_bps as u128).unwrap().checked_div(10_000).unwrap() as u64;
    let net_sol = gross_sol.checked_sub(platform_fee).ok_or(VaultpadError::MathOverflow)?
        .checked_sub(creator_fee).ok_or(VaultpadError::MathOverflow)?;
    require!(net_sol >= min_sol_out, VaultpadError::SlippageExceeded);

    // Tokens: seller → vault
    token::transfer(
        CpiContext::new(ctx.accounts.token_program.to_account_info(),
            Transfer { from: ctx.accounts.seller_token_account.to_account_info(), to: ctx.accounts.token_vault.to_account_info(), authority: ctx.accounts.seller.to_account_info() }),
        tokens_in,
    )?;

    // SOL distributions from vault
    if net_sol > 0 {
        **ctx.accounts.sol_vault.to_account_info().try_borrow_mut_lamports()? -= net_sol;
        **ctx.accounts.seller.to_account_info().try_borrow_mut_lamports()? += net_sol;
    }
    if platform_fee > 0 {
        **ctx.accounts.sol_vault.to_account_info().try_borrow_mut_lamports()? -= platform_fee;
        **ctx.accounts.platform_wallet.to_account_info().try_borrow_mut_lamports()? += platform_fee;
    }
    if creator_fee > 0 {
        let dest = if launch.category == CATEGORY_CAUSE { ctx.accounts.charity_wallet.to_account_info() } else { ctx.accounts.creator_wallet.to_account_info() };
        **ctx.accounts.sol_vault.to_account_info().try_borrow_mut_lamports()? -= creator_fee;
        **dest.try_borrow_mut_lamports()? += creator_fee;
    }

    // Update curve
    launch.virtual_sol_reserves = launch.virtual_sol_reserves.checked_sub(gross_sol).ok_or(VaultpadError::MathOverflow)?;
    launch.virtual_token_reserves = launch.virtual_token_reserves.checked_add(tokens_in).ok_or(VaultpadError::MathOverflow)?;
    launch.real_sol_reserves = launch.real_sol_reserves.checked_sub(gross_sol).ok_or(VaultpadError::MathOverflow)?;
    launch.real_token_reserves = launch.real_token_reserves.checked_add(tokens_in).ok_or(VaultpadError::MathOverflow)?;

    // Update tracker
    let tracker = &mut ctx.accounts.wallet_tracker;
    tracker.claimed_balance = tracker.claimed_balance.saturating_sub(tokens_in);

    let total_fee = platform_fee.checked_add(creator_fee).unwrap();
    launch.volume = launch.volume.checked_add(gross_sol).unwrap();
    launch.fees_generated = launch.fees_generated.checked_add(total_fee).unwrap();
    launch.trade_count = launch.trade_count.checked_add(1).unwrap();
    let c = &mut ctx.accounts.protocol_config;
    c.total_volume = c.total_volume.checked_add(gross_sol).unwrap();
    c.total_fees_collected = c.total_fees_collected.checked_add(total_fee).unwrap();

    msg!("Sell: {} tokens -> {} SOL (net)", tokens_in, net_sol);
    Ok(())
}
