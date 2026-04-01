# VAULTPAD Protocol

**Anti-manipulation token launchpad on Solana.**

Vaultpad is a Solana program built with Anchor that lets anyone launch a token with a built-in bonding curve. What makes it different: every single buy is escrowed with a randomized lock timer, snipers and bundlers get punished with 72-hour locks, no wallet can hold more than 4% of supply, and creators cannot buy their own token. The protocol enforces all of this on-chain — no backend, no admin override, no exceptions.

---

## Table of Contents

- [Why Vaultpad Exists](#why-vaultpad-exists)
- [How a Token Launch Works](#how-a-token-launch-works)
- [Anti-Manipulation Stack](#anti-manipulation-stack)
- [Fee Structure](#fee-structure)
- [Charity Wallet System](#charity-wallet-system)
- [All Instructions](#all-instructions)
- [All Accounts](#all-accounts)
- [Bonding Curve Math](#bonding-curve-math)
- [Build and Deploy](#build-and-deploy)
- [Project Structure](#project-structure)
- [Security Considerations](#security-considerations)
- [License](#license)

---

## Why Vaultpad Exists

Token launches on Solana have a recurring problem. A creator launches a token, snipers buy in the first block using bots, bundlers use dozens of wallets funded from the same source to grab supply, and within minutes the token is dead because insiders dumped on real buyers.

Vaultpad makes all of that unprofitable at the protocol level. Not through terms of service or off-chain monitoring, but through on-chain program logic that cannot be bypassed.

---

## How a Token Launch Works

When someone calls `create_launch`, the following happens in a single atomic transaction:

1. **2.5 SOL launch fee** is transferred from the creator to the platform treasury. This deters spam and low-effort launches.

2. **1 billion tokens are minted** (6 decimals = 1,000,000,000,000,000 base units). The token mint authority is set to the LaunchState PDA, meaning no one — not even the creator — can mint more tokens after launch.

3. **90% of the supply** is deposited into the bonding curve vault, available for buyers to purchase.

4. **A bonding curve is initialized** using constant-product (x*y=k) math with a 0.5 SOL virtual reserve. This sets the initial token price and ensures smooth price discovery as people buy in.

5. **For cause/charity tokens**: a charity wallet PDA is automatically derived. All creator fees (1% of every trade) flow into this wallet instead of the creator's personal wallet. Only the designated charity authority (linked to an X account off-chain) can withdraw.

After this transaction confirms, the token is immediately tradeable. Anyone can call `buy` to purchase tokens or `sell` to sell them back.

---

## Anti-Manipulation Stack

Vaultpad has five layers of on-chain protection. All enforced in the program. None can be turned off.

### 1. Per-Buyer Randomized Token Locks

This is the core mechanic. When you call `buy`, you do NOT receive tokens in your wallet. Instead, the program creates a `BuyerLock` PDA — an escrow account that records how many tokens you bought and when you can claim them.

Each lock gets a **random duration between 5 and 30 minutes**. The randomness is derived from:

```
seed = current_slot * unix_timestamp + buyer_pubkey_bytes + lock_counter
duration = 5 minutes + (seed mod 25 minutes)
```

To actually receive your tokens, you must call `claim_buyer_lock` after the lock expires. Until then, the tokens sit in the program vault. You cannot sell, transfer, or do anything with locked tokens.

A single buyer can have **multiple active locks** — one for each buy transaction. Each gets its own random timer.

**Why this works:** Snipers rely on buying and immediately selling (or transferring to a DEX). If every buy is locked for 5-30 minutes, front-running becomes a guessing game with no guaranteed exit. The sniper's capital is trapped, and by the time they can sell, the price may have moved against them.

### 2. Same-Block Detection (Anti-Bundling)

The `LaunchState` account tracks two fields: `last_buy_slot` (the most recent Solana slot where a buy occurred) and `buys_in_current_slot` (how many unique wallets bought in that slot).

If **2 or more wallets** buy in the same slot, all buyers in that slot receive a **penalty lock: 0 to 72 hours** (randomly distributed). Instead of waiting 5-30 minutes, they could be waiting up to 3 days.

```
Same slot detected (2+ wallets):
  Normal lock:   5-30 minutes
  Penalty lock:  0-72 hours (random)
```

**Why this works:** Bundlers submit multiple buy transactions from different wallets in the same block. This is the textbook attack on every pump.fun-style launchpad. Vaultpad detects it and punishes it. The bundler's tokens are locked for up to 3 days while everyone else's tokens unlock in minutes.

### 3. Same-Funder Detection (Anti-Sybil)

Every wallet that buys gets a `WalletTracker` PDA. On the first buy, the program records the wallet's `funder` — the account that originally sent SOL to that wallet. This is passed as an account in the buy instruction.

If two different wallets share the same funder, **anyone** can call the `flag_same_funder` instruction. This is a permissionless on-chain snitch mechanism. You pass in two WalletTracker accounts. The program checks:

- Are they different wallets? Yes.
- Do they have the same funder? Yes.
- Flag them both.

Once flagged, a wallet receives **72-hour penalty locks on every future buy** for that token. The flag is permanent and cannot be removed.

**Why this works:** Sybil attackers fund dozens of wallets from a single source. Vaultpad tracks this and lets anyone flag it. Community members, bots, or competing traders all have an incentive to flag sybils — it removes competition. The flagged wallets are permanently penalized.

### 4. Creator Cannot Buy

The very first check in the `buy` instruction:

```rust
require!(buyer.key() != launch.creator, CreatorCannotBuy);
```

The creator's public key is recorded in the `LaunchState` at launch time. If the buyer's signer matches, the transaction reverts. The creator can only earn through the 1% fee on other people's trades.

**Why this works:** A common rug pattern is the creator buying their own token with alt wallets to inflate the price, then selling. By blocking the creator wallet entirely, one avenue is closed. Combined with same-funder detection, using alt wallets funded from the creator's wallet also gets caught.

### 5. Max 4% Wallet Cap

No single wallet can accumulate more than 4% of the total token supply. This is enforced via the `WalletTracker.total_purchased` field, which accumulates across all buys (including unclaimed locks).

```rust
let max_tokens = total_supply * 400 / 10_000; // 4%
require!(tracker.total_purchased + tokens_out <= max_tokens, MaxWalletExceeded);
```

If a buy would push a wallet over 4%, the entire transaction reverts.

**Why this works:** Whales and insiders cannot corner the supply. Even if someone uses 10 wallets, each is capped at 4%, and same-funder detection penalizes multi-wallet strategies. The maximum any coordinated group can hold is limited by how many independently-funded wallets they operate — and each flagged wallet gets a 72h lock.

---

## Fee Structure

Every trade (buy and sell) has a **2% total fee**, split evenly:

| Recipient | Fee | Collected On |
|-----------|-----|-------------|
| Platform treasury | 1% (100 bps) | Every buy and sell |
| Creator | 1% (100 bps) | Every buy and sell |

**On buys:** Fees are deducted from the SOL input before the bonding curve calculation. If you send 1 SOL, 0.01 goes to the platform, 0.01 goes to the creator, and 0.98 enters the curve.

**On sells:** Fees are deducted from the SOL output after the curve calculation. If the curve returns 1 SOL, 0.01 goes to the platform, 0.01 goes to the creator, and you receive 0.98.

**For cause tokens:** The creator's 1% is redirected to the charity wallet PDA instead of the creator's personal wallet.

The fee percentages are set in the `ProtocolConfig` and can be updated by the protocol authority. They are capped at a hard maximum of 5% (500 bps) per fee — this cap is enforced in the program and cannot be changed.

---

## Charity Wallet System

When a token is created with `category = 2` (cause), the protocol activates the charity wallet system:

**At launch time:**
- A charity wallet PDA is derived from `[b"charity", mint_pubkey]`
- A `charity_authority` pubkey is set — this is the only signer that can withdraw from the charity wallet
- The charity authority is typically linked to an X (Twitter) account through off-chain verification

**During trading:**
- The creator's 1% fee from every buy and sell is sent to the charity wallet PDA instead of the creator's wallet
- SOL accumulates in the charity PDA over time as people trade

**Withdrawals:**
- Only the `charity_authority` can call `withdraw_charity`
- They specify an amount and a destination wallet
- The program verifies the signer matches the authority set at launch
- SOL transfers from the charity PDA to the destination
- A rent-exempt minimum is always preserved in the PDA

**Off-chain (how X account verification works):**
- The charity authority keypair is managed by a backend service
- To withdraw, the X account owner authenticates via X OAuth
- After verification, the backend signs the `withdraw_charity` transaction
- This ensures only the legitimate cause owner can access the funds

---

## All Instructions

### `initialize_protocol`

**Who can call:** Anyone (once). The caller becomes the protocol authority.

**What it does:** Creates the global `ProtocolConfig` PDA that stores the platform wallet address, fee rates, launch cost, and aggregate statistics. This must be called once before any launches can be created.

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `platform_wallet` | `Pubkey` | Treasury that receives the 1% platform fee |
| `platform_fee_bps` | `u16` | Platform fee in basis points (100 = 1%) |
| `creator_fee_bps` | `u16` | Creator fee in basis points (100 = 1%) |
| `launch_cost` | `u64` | SOL cost to launch a token, in lamports |

---

### `update_protocol`

**Who can call:** Protocol authority only.

**What it does:** Updates any field in the `ProtocolConfig`. All fields are optional — pass `None` for fields you don't want to change. Fee values are validated against the 500 bps (5%) hard cap.

---

### `create_launch`

**Who can call:** Anyone with enough SOL to cover the launch cost (2.5 SOL default) plus account rent.

**What it does:** Creates a new token and initializes its bonding curve. Mints the full supply, sets up the curve reserves, configures the creator lock with a randomized timer, and optionally initializes a charity wallet for cause tokens.

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `name` | `String` | Token name, max 32 characters |
| `ticker` | `String` | Token ticker symbol, max 10 characters |
| `uri` | `String` | Metadata URI, max 200 characters |
| `category` | `u8` | 0 = meme, 1 = tech, 2 = cause, 3 = defi |
| `fee_redirect_type` | `u8` | 0 = X, 1 = GitHub, 2 = Instagram |
| `fee_redirect_handle` | `String` | Social handle for fee attribution |
| `charity_authority` | `Option<Pubkey>` | For cause tokens: who controls the charity wallet |

---

### `buy`

**Who can call:** Anyone EXCEPT the creator of the token.

**What it does:** This is where all the anti-manipulation logic lives. Accepts SOL, deducts fees, calculates tokens via the bonding curve, and creates a `BuyerLock` escrow with a randomized unlock timer. Tokens do NOT go to the buyer's wallet.

**Anti-manipulation checks (in order):**
1. Buyer is not the creator
2. 4% wallet cap won't be exceeded
3. Same-block detection (penalty if 2+ wallets in same slot)
4. Same-funder detection (penalty if wallet is flagged)

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `amount_in` | `u64` | SOL to spend, in lamports |
| `min_tokens_out` | `u64` | Minimum tokens expected (slippage protection) |

---

### `sell`

**Who can call:** Anyone holding claimed (unlocked) tokens.

**What it does:** Swaps tokens back to the bonding curve for SOL. Seller must have tokens in their actual wallet (not in a lock). Same 2% fee applies on the SOL output.

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `tokens_in` | `u64` | Tokens to sell, in base units |
| `min_sol_out` | `u64` | Minimum SOL expected, in lamports |

---

### `claim_buyer_lock`

**Who can call:** The buyer who owns the lock.

**What it does:** Checks if the current time is past the lock's `unlock_time`. If yes, transfers tokens from the vault to the buyer's wallet. Updates the WalletTracker balances.

**Reverts if:** Lock hasn't expired, already claimed, or wrong signer.

---

### `claim_creator_lock`

**Who can call:** The creator of the token.

**What it does:** Same as claim_buyer_lock but for the creator's 10% allocation with its longer 1-24h timer.

---

### `withdraw_charity`

**Who can call:** The designated charity authority only.

**What it does:** Transfers SOL from the charity wallet PDA to any destination. Only works for cause tokens.

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `amount` | `u64` | Lamports to withdraw |

---

### `flag_same_funder`

**Who can call:** Anyone. Permissionless.

**What it does:** Compares two WalletTracker accounts. If they belong to different wallets but share the same funder, both get permanently flagged for 72h penalty locks on all future buys. This is the on-chain snitch mechanism.

---

## All Accounts

### `ProtocolConfig` — PDA `[b"protocol"]`

| Field | Type | Description |
|-------|------|-------------|
| `authority` | `Pubkey` | Protocol admin |
| `platform_wallet` | `Pubkey` | Fee treasury |
| `platform_fee_bps` | `u16` | Platform fee (default 100) |
| `creator_fee_bps` | `u16` | Creator fee (default 100) |
| `launch_cost` | `u64` | Launch cost in lamports |
| `total_launches` | `u64` | Counter |
| `total_volume` | `u64` | Cumulative volume |
| `total_fees_collected` | `u64` | Cumulative fees |

### `LaunchState` — PDA `[b"launch", mint]`

| Field | Type | Description |
|-------|------|-------------|
| `mint` | `Pubkey` | Token mint |
| `creator` | `Pubkey` | Creator wallet (blocked from buying) |
| `name, ticker, uri` | `String` | Token metadata |
| `category` | `u8` | 0-3 |
| `virtual_sol_reserves` | `u64` | Curve virtual SOL |
| `virtual_token_reserves` | `u64` | Curve virtual tokens |
| `real_sol_reserves` | `u64` | Actual SOL in vault |
| `real_token_reserves` | `u64` | Actual tokens in vault |
| `total_supply` | `u64` | For max wallet calculation |
| `creator_lock_unlock_time` | `i64` | Creator unlock timestamp |
| `creator_locked_tokens` | `u64` | Tokens locked for creator |
| `charity_wallet` | `Pubkey` | Charity PDA address |
| `charity_authority` | `Pubkey` | Who can withdraw charity |
| `last_buy_slot` | `u64` | For same-block detection |
| `buys_in_current_slot` | `u8` | Counter per slot |
| `penalty_locks_issued` | `u64` | Penalty counter |

### `BuyerLock` — PDA `[b"lock", mint, buyer, lock_id]`

| Field | Type | Description |
|-------|------|-------------|
| `mint` | `Pubkey` | Token mint |
| `buyer` | `Pubkey` | Lock owner |
| `lock_id` | `u64` | Sequential ID |
| `amount` | `u64` | Tokens escrowed |
| `unlock_time` | `i64` | When claimable |
| `claimed` | `bool` | Whether claimed |
| `is_penalty` | `bool` | Whether this is a 72h penalty lock |

### `WalletTracker` — PDA `[b"wallet_tracker", mint, wallet]`

| Field | Type | Description |
|-------|------|-------------|
| `mint` | `Pubkey` | Token mint |
| `wallet` | `Pubkey` | Tracked wallet |
| `total_purchased` | `u64` | Cumulative buys (for 4% cap) |
| `locked_balance` | `u64` | In active locks |
| `claimed_balance` | `u64` | In wallet |
| `funder` | `Pubkey` | First SOL sender |
| `flagged` | `bool` | Permanently flagged |
| `buy_count` | `u32` | Number of buys |

---

## Bonding Curve Math

Constant-product AMM (x*y=k), same formula as Uniswap v2.

**Initial state:** 0.5 SOL virtual reserve, full supply as virtual token reserve.

**Buy:** `tokens_out = virtual_tokens - (k / (virtual_sol + sol_in))`

**Sell:** `sol_out = virtual_sol - (k / (virtual_tokens + tokens_in))`

Price rises as SOL enters, drops as tokens are sold back. All outputs capped at real reserves.

---

## Build and Deploy

```bash
# Install Anchor 0.30.1
cargo install --git https://github.com/coral-xyz/anchor avm --force
avm install 0.30.1 && avm use 0.30.1

# Build
anchor build

# Get program ID and update lib.rs + Anchor.toml
solana-keygen pubkey target/deploy/vaultpad-keypair.json

# Deploy
anchor deploy --provider.cluster devnet
```

---

## Project Structure

```
programs/vaultpad/src/
├── lib.rs                              # 9 instructions
├── constants.rs                        # Seeds, fees, lock bounds, thresholds
├── errors.rs                           # 22 error codes
├── state.rs                            # 4 account structs + curve math
└── instructions/
    ├── initialize_protocol.rs          # One-time setup
    ├── update_protocol.rs              # Config updates
    ├── create_launch.rs                # Mint + curve + creator lock + charity
    ├── buy.rs                          # The big one: all anti-manipulation
    ├── sell.rs                         # Tokens back to SOL
    ├── claim_buyer_lock.rs             # Claim after lock expires
    ├── claim_creator_lock.rs           # Creator claims 10%
    ├── withdraw_charity.rs             # Charity withdrawals
    └── flag_same_funder.rs             # Permissionless sybil flagging
```

---

## Security Considerations

**Randomness.** Lock durations use on-chain slot/timestamp. Not cryptographically random, but unpredictable for external observers. Validators could predict but not retroactively change.

**Integer math.** All checked. Overflows revert.

**Fee enforcement.** Atomic within buy/sell. Cannot be bypassed.

**Charity keys.** Managed off-chain via X OAuth. Consider multisig for high-value causes.

**Funder field.** Passed by client on first buy. Sophisticated attackers could fake it, but `flag_same_funder` is permissionless — anyone can compare on-chain SOL history and flag.

**Wallet cap bypass.** 25+ independently-funded wallets could theoretically hold 100%. Economic cost is significant, and same-block detection still applies.

---

## License

MIT — see [LICENSE](./LICENSE) file.

