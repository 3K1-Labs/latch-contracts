# Recurring Escrow Contract Spec

A "personal contract" template that implements pre-funded, fixed-schedule recurring payments for Latch smart accounts. The owner deposits a lump sum; a designated payee pulls fixed amounts on a fixed ledger cadence.

## Motivation & Architecture

The recurring escrow is a contract version of the "Backend Automation" use case — recurring subscription payments — but as a pre-funded, capped satellite contract rather than an ongoing policy on the main account.

It follows the **"personal contract" model**:
- A lightweight satellite contract owned by the user's smart account C-address.
- Holds SEP-41 tokens and exposes `pull` / `cancel` entrypoints.
- Pre-funded: the owner deposits a lump sum up front; the payee pulls from that balance.
- Discoverable: clients can identify the contract bytecode and display a recurring-payment-specific interface.

## Single-Token, Single-Payee Design

The contract fixes the token and payee at construction time.
- Matches the timelock vault / vesting schedule shape (one asset per instance).
- Eliminates multi-account accounting complexity within a single instance.
- Multiple payees require separate deployments.

## Deposit (Top-Up)

No explicit `deposit()` method — the owner (or anyone) sends more tokens directly to the contract's address via a plain `token::transfer`. Same reasoning as the timelock vault.

## Contract Interface

```rust
pub fn __constructor(
    e: &Env,
    owner: Address,
    payee: Address,
    amount_per_period: i128,
    period_ledgers: u32,
    token: Address,
);

pub fn pull(e: Env, to: Address);

pub fn cancel(e: Env, to: Address);

pub fn get_owner(e: &Env) -> Address;
pub fn get_payee(e: &Env) -> Address;
pub fn get_token(e: &Env) -> Address;
pub fn get_amount_per_period(e: &Env) -> i128;
pub fn get_period_ledgers(e: &Env) -> u32;
pub fn get_last_pull_ledger(e: &Env) -> u32;
pub fn is_cancelled(e: &Env) -> bool;
pub fn get_balance(e: &Env) -> i128;
```

### `__constructor`

| Parameter          | Type    | Description                                          |
| ------------------ | ------- | ---------------------------------------------------- |
| `owner`            | Address | Smart-account address that funds and controls the escrow. |
| `payee`            | Address | Address authorized to pull periodic payments.        |
| `amount_per_period`| i128    | Number of stroops released per period (must be > 0). |
| `period_ledgers`   | u32     | Number of ledgers between allowed pulls (must be > 0). |
| `token`            | Address | SEP-41 token contract address.                       |

### `pull`

Releases one period's worth of tokens. Requires `payee.require_auth()`.

Conditions checked (in order):
1. **Cancelled** — reverts with `EscrowError::Cancelled` if the escrow has been cancelled.
2. **Period gate** — reverts with `EscrowError::TooEarly` if fewer than `period_ledgers` have elapsed since the last pull (no wait on first pull).
3. **Balance gate** — reverts with `EscrowError::InsufficientBalance` if the contract balance is less than `amount_per_period`.

On success: transfers `amount_per_period` to `to`, updates `last_pull_ledger`, emits `Pullled`.

### `cancel`

Transfers the entire remaining token balance to `to` and permanently disables future `pull()` calls. Requires `owner.require_auth()`. Idempotent — calling cancel on an already-cancelled escrow is a no-op.

Emits `Cancelled` with the refunded amount.

## Error Codes

| Error                | Code | Meaning                                           |
| -------------------- | ---- | ------------------------------------------------- |
| `InvalidAmount`      | 1    | `amount_per_period` was not positive.             |
| `InvalidPeriod`      | 2    | `period_ledgers` was zero.                        |
| `TooEarly`           | 3    | `pull()` called before a full period had elapsed. |
| `InsufficientBalance`| 4    | Contract balance < `amount_per_period`.           |
| `Cancelled`          | 5    | Escrow has been cancelled.                        |

## Events

| Event      | Fields                                      | Emitted by           |
| ---------- | ------------------------------------------- | -------------------- |
| `Pullled`  | payee (topic), to, amount, ledger           | `pull()` on success  |
| `Cancelled`| owner (topic), to, refunded                 | `cancel()`           |

## Out of Scope (v1)

- Variable / escalating payment amounts — fixed amount per period only.
- Multiple payees per instance — separate deployments.
- Automatic / unprompted payment execution — Soroban has no native cron primitive.

## References

- Issue [#43](https://github.com/3K1-Labs/latch-contracts/issues/43)
- Issue [#39](https://github.com/3K1-Labs/latch-contracts/issues/39) — deployment entrypoint.
- Issue [#40](https://github.com/3K1-Labs/latch-contracts/issues/40) — timelock vault ("no deposit() needed" reasoning).
