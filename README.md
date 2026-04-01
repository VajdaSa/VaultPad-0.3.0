# VAULTPAD Protocol v0.3

Solana program (Anchor) implementing a token launchpad with aggressive on-chain anti-manipulation mechanics.

Every buy is escrowed. Snipers get 72-hour locks. Bundlers get 72-hour locks. No wallet can hold more than 4%. Creators can't buy their own token.

## Anti-Manipulation Stack

### 1. Per-Buyer Randomized Locks
Every `buy` creates a `BuyerLock` PDA. Tokens sit in the vault until the lock expires.
- **Normal buyers**: 5-30 minute random lock
- **Penalty buyers**: 0-72 hour random lock (see below)

### 2. Same-Block Detection
If 2+ different wallets buy in the same Solana slot → **all buyers in that slot get a 72-hour penalty lock** (randomly opened within that window). This kills bundlers who submit multiple buys in one block.

### 3. Same-Funder Detection
Every wallet's funder (first SOL sender) is recorded in a `WalletTracker` PDA. If two wallets share the same funder, anyone can call `flag_same_funder` to flag them both. **Flagged wallets get 72h penalty locks on ALL future buys.** This is a permissionless on-chain snitch mechanism — bots and community members can detect sybil wallets and flag them.

### 4. Creator Cannot Buy
The `buy` instruction hard-reverts if `buyer == launch.creator`. Creator only earns via the 1% fee on other people's trades.

### 5. Max 4% Wallet Cap
No wallet can accumulate more than 4% of total supply (across all locks + claimed tokens). Enforced via `WalletTracker.total_purchased`. This prevents any single entity from cornering supply.

## Fee Structure

Every trade: **1% platform + 1% creator = 2% total**

For cause tokens, the creator's 1% auto-deposits into a charity wallet PDA gated by X account verification.

## Instructions (9 total)

| Instruction | Who | What |
|---|---|---|
| `initialize_protocol` | Authority | One-time setup |
| `update_protocol` | Authority | Modify config |
| `create_launch` | Anyone | Mint token + bonding curve + creator lock (1-24h) |
| `buy` | Anyone (not creator) | SOL → escrowed tokens with anti-manipulation checks |
| `sell` | Anyone | Claimed tokens → SOL |
| `claim_buyer_lock` | Lock owner | Claim after lock expires |
| `claim_creator_lock` | Creator | Claim 10% after creator lock expires |
| `withdraw_charity` | Charity auth | Withdraw from charity PDA |
| `flag_same_funder` | Anyone | Flag two wallets sharing a funder → penalty locks |

## Account Layout

```
ProtocolConfig     [b"protocol"]                          — global config
LaunchState        [b"launch", mint]                      — per-token curve + state
BuyerLock          [b"lock", mint, buyer, id]             — per-buy escrow
WalletTracker      [b"wallet_tracker", mint, wallet]      — holdings + funder tracking
SOL Vault          [b"sol_vault", mint]                   — SOL reserves
Charity Wallet     [b"charity", mint]                     — cause token fees
Token Vault        ATA of LaunchState                     — token supply
```

## Build & Deploy

```bash
anchor build
solana-keygen pubkey target/deploy/vaultpad-keypair.json
# Update declare_id! in lib.rs + Anchor.toml
anchor deploy --provider.cluster devnet
```

## License

MIT
