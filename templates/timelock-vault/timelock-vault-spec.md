# Timelock Vault Spec

A per-user-deployable Soroban contract that locks funds until a specified ledger sequence. First concrete example of the "personal contracts" pattern — small, pre-vetted, auditable contracts deployed via a user's smart account.

## Motivation

Users want a simple way to set aside funds with a time-based lock — a "savings vault" that prevents impulsive withdrawals before a target date, while still going through the smart account's full authorization stack (signers, policies) for the actual withdrawal. Unlike policies and verifiers, which are shared singletons, each user deploys their own vault instance via the smart account's `CreateContract`-gated entrypoint (#39).

## Contract Interface

```rust
fn __constructor(e: &Env, owner: Address, unlock_ledger: u32)
fn deposit(e: Env, from: Address, token: Address, amount: i128)
fn withdraw(e: Env, token: Address, amount: i128, to: Address)
fn get_owner(e: &Env) -> Address
fn get_unlock_ledger(e: &Env) -> u32
fn get_balance(e: &Env, token: Address) -> i128
```

### `__constructor`

Sets the vault's `owner` (the user's smart-account C-address) and `unlock_ledger` (the ledger sequence at which withdrawal becomes possible). Both values are immutable after deployment. Rejects `unlock_ledger` that is not strictly in the future.

### `deposit`

Convenience method: calls `from.require_auth()`, transfers tokens from `from` to the vault's own address via `token::TokenClient::transfer`, and emits a `Deposited` event.

Not strictly required — anyone can send funds to the vault via a plain `token::transfer` without calling this method. It exists for indexer/audit-trail purposes.

### `withdraw`

Requires **both**:
1. `owner.require_auth()` — only the owning smart account may withdraw.
2. `e.ledger().sequence() >= unlock_ledger` — the ledger must be at or past the unlock point.

Neither check alone is sufficient: owner-only would defeat the timelock, ledger-only would let anyone drain it.

Transfers `amount` of the given token from the vault to `to`. Emits a `Withdrawn` event.

### `get_owner` / `get_unlock_ledger` / `get_balance`

Read-only queries returning the vault's owner address, unlock point, and current balance of any given token.

## Types

```rust
/// Persistent storage keys.
#[contracttype]
pub enum DataKey {
    Owner,
    UnlockLedger,
}

/// Error codes.
#[contracterror]
#[repr(u32)]
pub enum VaultError {
    StillLocked = 1,
    InvalidUnlockLedger = 2,
}
```

## Events

```rust
#[contractevent]
pub struct Deposited {
    #[topic] pub token: Address,
    pub from: Address,
    pub amount: i128,
}

#[contractevent]
pub struct Withdrawn {
    #[topic] pub token: Address,
    pub to: Address,
    pub amount: i128,
}
```

## Storage

| Key | Type | Mutability | Description |
|-----|------|------------|-------------|
| `DataKey::Owner` | `Address` | Immutable | The smart-account address that owns this vault |
| `DataKey::UnlockLedger` | `u32` | Immutable | Ledger sequence at which withdrawal unlocks |

Both entries use persistent storage with TTL management (~120-day extend, bump on access).

Token balances are held in the vault's own contract address on the respective SAC/SEP-41 token contracts — the vault does not maintain its own balance ledger.

## Security Model

1. **Dual gate on withdrawal.** Both `owner.require_auth()` and `sequence >= unlock_ledger` must pass. This means the vault adds a time constraint *on top of* whatever signer/policy stack the smart account already enforces.
2. **Owner is a smart account.** Because `owner` is the user's C-address (not an Ed25519 key), `require_auth()` triggers the smart account's full `__check_auth` — so the vault never bypasses multisig, session policies, spending limits, etc.
3. **Immutable lock.** `unlock_ledger` cannot be changed after deployment, preventing social-engineering attacks that trick a user into pushing the lock earlier.
4. **Open deposits.** Anyone can send funds to the vault (directly or via `deposit()`). Only the owner can withdraw, and only after the unlock point.

## What This Is Not

- **Not a vesting schedule.** No linear/cliff release over time.
- **Not a dead-man's switch.** No beneficiary claims after owner inactivity.
- **Not a recurring escrow.** No scheduled releases up to a cap.
- **Not a shared singleton.** Each user deploys their own instance.

## Test Plan

| Test | Description |
|------|-------------|
| `constructor_stores_owner_and_unlock` | Verify constructor state is correct |
| `constructor_rejects_past_unlock` | `unlock_ledger < current` → `InvalidUnlockLedger` |
| `constructor_rejects_equal_unlock` | `unlock_ledger == current` → `InvalidUnlockLedger` |
| `deposit_transfers_and_shows_balance` | Deposit updates vault balance |
| `withdraw_before_unlock_rejected` | Valid owner, too early → `StillLocked` |
| `withdraw_after_unlock_succeeds` | Valid owner, past unlock → success |
| `withdraw_at_exact_unlock_ledger_succeeds` | Boundary: `sequence == unlock_ledger` succeeds |
| `withdraw_non_owner_rejected_before_unlock` | Wrong caller, before unlock → auth failure |
| `withdraw_non_owner_rejected_after_unlock` | Wrong caller, after unlock → auth failure |
| `works_with_multiple_tokens` | Deposit/withdraw two different SAC tokens |
| `deposit_emits_event` | `Deposited` event emitted |
| `withdraw_emits_event` | `Withdrawn` event emitted |
| `partial_withdraw_leaves_remainder` | Partial withdrawal, then second withdrawal |
| `e2e_deploy_via_smart_account` | `#[ignore]` — awaiting #39 |
