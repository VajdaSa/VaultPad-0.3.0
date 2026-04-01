use anchor_lang::prelude::*;
use crate::state::*;
use crate::constants::*;
use crate::errors::VaultpadError;

/// Permissionless instruction: anyone can flag two wallets as same-funder.
/// If wallet_a.funder == wallet_b.funder, both get flagged.
/// Flagged wallets receive penalty locks (72h) on all future buys.
///
/// This is the on-chain "snitch" mechanism — community members or bots
/// can detect sybil wallets and flag them.
#[derive(Accounts)]
pub struct FlagSameFunder<'info> {
    #[account(
        mut,
        seeds = [LAUNCH_SEED, launch_state.mint.as_ref()],
        bump = launch_state.bump,
    )]
    pub launch_state: Account<'info, LaunchState>,

    #[account(
        mut,
        seeds = [WALLET_TRACKER_SEED, launch_state.mint.as_ref(), tracker_a.wallet.as_ref()],
        bump = tracker_a.bump,
        constraint = tracker_a.mint == launch_state.mint,
    )]
    pub tracker_a: Account<'info, WalletTracker>,

    #[account(
        mut,
        seeds = [WALLET_TRACKER_SEED, launch_state.mint.as_ref(), tracker_b.wallet.as_ref()],
        bump = tracker_b.bump,
        constraint = tracker_b.mint == launch_state.mint,
    )]
    pub tracker_b: Account<'info, WalletTracker>,

    /// Anyone can call this
    pub caller: Signer<'info>,
}

pub fn handler(ctx: Context<FlagSameFunder>) -> Result<()> {
    let a = &ctx.accounts.tracker_a;
    let b = &ctx.accounts.tracker_b;

    // Must be different wallets
    require!(a.wallet != b.wallet, VaultpadError::InvalidAmount);

    // Must share the same funder
    require!(
        a.funder == b.funder && a.funder != Pubkey::default(),
        VaultpadError::Unauthorized
    );

    // Flag both
    let a_mut = &mut ctx.accounts.tracker_a;
    a_mut.flagged = true;

    let b_mut = &mut ctx.accounts.tracker_b;
    b_mut.flagged = true;

    msg!(
        "FLAGGED same-funder: {} and {} (funder: {})",
        a_mut.wallet, b_mut.wallet, a_mut.funder
    );

    Ok(())
}
