use anchor_lang::prelude::*;

pub mod state;
pub mod instructions;
pub mod errors;
pub mod constants;

use instructions::*;

declare_id!("Vault1111111111111111111111111111111111111");

#[program]
pub mod vaultpad {
    use super::*;

    /// Initialize the protocol (one-time).
    pub fn initialize_protocol(ctx: Context<InitializeProtocol>, params: InitializeProtocolParams) -> Result<()> {
        instructions::initialize_protocol::handler(ctx, params)
    }

    /// Update protocol config (authority only).
    pub fn update_protocol(ctx: Context<UpdateProtocol>, params: UpdateProtocolParams) -> Result<()> {
        instructions::update_protocol::handler(ctx, params)
    }

    /// Create a new token launch with bonding curve.
    pub fn create_launch(ctx: Context<CreateLaunch>, params: CreateLaunchParams) -> Result<()> {
        instructions::create_launch::handler(ctx, params)
    }

    /// Buy tokens. Every buy is escrowed in a BuyerLock PDA.
    ///
    /// Anti-manipulation enforced on-chain:
    /// - Creator CANNOT buy their own token
    /// - 4% max wallet cap (no wallet can hold >4% of supply)
    /// - Same-block buys (2+ wallets in one slot) → 72h penalty lock
    /// - Same-funder wallets → 72h penalty lock
    /// - Normal buyers → 5-30 min randomized lock
    pub fn buy(ctx: Context<Buy>, amount_in: u64, min_tokens_out: u64) -> Result<()> {
        instructions::buy::handler(ctx, amount_in, min_tokens_out)
    }

    /// Sell claimed (unlocked) tokens back to the curve.
    pub fn sell(ctx: Context<Sell>, tokens_in: u64, min_sol_out: u64) -> Result<()> {
        instructions::sell::handler(ctx, tokens_in, min_sol_out)
    }

    /// Claim tokens from a buyer lock after it expires.
    pub fn claim_buyer_lock(ctx: Context<ClaimBuyerLock>) -> Result<()> {
        instructions::claim_buyer_lock::handler(ctx)
    }

    /// Creator claims their locked 10% after creator lock expires.
    pub fn claim_creator_lock(ctx: Context<ClaimCreatorLock>) -> Result<()> {
        instructions::claim_creator_lock::handler(ctx)
    }

    /// Withdraw from charity wallet (cause tokens only).
    pub fn withdraw_charity(ctx: Context<WithdrawCharity>, amount: u64) -> Result<()> {
        instructions::withdraw_charity::handler(ctx, amount)
    }

    /// Permissionless: flag two wallets as same-funder.
    /// If wallet_a.funder == wallet_b.funder, both get flagged
    /// for 72h penalty locks on all future buys.
    /// Anyone can call this — it's the on-chain snitch mechanism.
    pub fn flag_same_funder(ctx: Context<FlagSameFunder>) -> Result<()> {
        instructions::flag_same_funder::handler(ctx)
    }
}
