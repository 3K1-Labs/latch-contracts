# Vesting Schedule Contract Spec

A "personal contract" template that implements linear and cliff-based token vesting for Latch smart accounts. Deployed per schedule, owned by the beneficiary's smart account.

## Motivation & Architecture

The vesting schedule contract is a natural generalization of a timelock vault: instead of unlocking a lump sum at a single fixed ledger, it releases funds gradually over time.

It follows the **"personal contract" model**:
- A lightweight satellite contract owned by the user's smart account C-address.
- Holds SEP-41 tokens and exposes a dedicated `claim` entrypoint.
- Non-revocable: once deployed and funded, the vesting schedule cannot be cancelled, modified, or drained by any third party.
- Discoverable: clients can identify the contract bytecode and display a vesting-specific schedule interface distinct from a plain vault.

## Single-Token Design

The contract fixes the vested asset at construction time via `token: Address`.
- Matches the timelock vault shape.
- Eliminates multi-token accounting complexity within a single instance.
- Prevents unrelated vesting schedules for different assets from sharing contract state.

## Contract Interface

```rust
pub fn __constructor(
    e: Env,
    owner: Address,
    token: Address,
    total_amount: i128,
    start_ledger: u32,
    cliff_ledger: Option<u32>,
    end_ledger: u32,
);

pub fn claim(e: Env, to: Address) -> i128;

pub fn get_schedule(e: Env) -> ScheduleData;

pub fn vested_amount(e: Env) -> i128;

pub fn claimable_amount(e: Env) -> i128;

pub fn claimed_amount(e: Env) -> i128;

pub fn owner(e: Env) -> Address;

pub fn token(e: Env) -> Address;
```

### `__constructor`

Initializes the vesting schedule.

#### Parameters
- `owner: Address` — The beneficiary smart account authorized to claim tokens.
- `token: Address` — The SEP-41 token contract address being vested.
- `total_amount: i128` — Total tokens to vest across the entire duration (must be `> 0`).
- `start_ledger: u32` — Ledger sequence where linear vesting starts.
- `cliff_ledger: Option<u32>` — Optional cliff ledger. When specified, must satisfy `start_ledger <= cliff_ledger <= end_ledger`.
- `end_ledger: u32` — Ledger sequence where 100% of tokens are vested (must be `> start_ledger`).

#### Validations
- Fails with `AlreadyInitialized` (`1`) if already initialized.
- Fails with `InvalidAmount` (`2`) if `total_amount <= 0`.
- Fails with `InvalidLedgerRange` (`3`) if `start_ledger >= end_ledger`.
- Fails with `InvalidCliffLedger` (`4`) if `cliff_ledger` is before `start_ledger` or after `end_ledger`.

### `claim`

Calculates the amount vested as of `e.ledger().sequence()`, subtracts `claimed_amount`, transfers the difference from the contract to `to`, records the new claimed amount, and emits a `Claimed` event.

- Requires `owner.require_auth()`.
- Returns the amount actually released (`0` if nothing is currently claimable, e.g. before start or before cliff).

## Mathematical Specification & Rounding

### Vesting Calculation

Given:
- $T$: `total_amount`
- $L_{start}$: `start_ledger`
- $L_{cliff}$: `cliff_ledger` (optional)
- $L_{end}$: `end_ledger`
- $L_{curr}$: `e.ledger().sequence()`

$$\text{Vested}(L_{curr}) = \begin{cases}
0 & \text{if } L_{curr} < L_{start} \\
0 & \text{if } L_{cliff} \text{ is set and } L_{curr} < L_{cliff} \\
T & \text{if } L_{curr} \ge L_{end} \\
\lfloor \frac{T \times (L_{curr} - L_{start})}{L_{end} - L_{start}} \rfloor & \text{otherwise}
\end{cases}$$

### Precision & Dust Guarantees

1. **Dust-Free Completion**: When $L_{curr} \ge L_{end}$, $\text{Vested} = T$ unconditionally. The final claim releases exactly $T - \text{claimed\_amount}$, ensuring no residual dust is left in the contract.
2. **Overflow Safety**: Multi-word 128-bit integer math (`mul_div_u128`) computes $\lfloor (T \times \Delta L_{elapsed}) / \Delta L_{total} \rfloor$ without intermediate 128-bit overflow even for amounts up to `i128::MAX`.
3. **Monotonicity**: Cumulative claims never decrease, and repeated calls in the same ledger sequence release `0` without double-claiming.

## State and Storage

Stored under instance storage (`DataKey::Schedule`):

```rust
#[contracttype]
pub struct ScheduleData {
    pub owner: Address,
    pub token: Address,
    pub total_amount: i128,
    pub start_ledger: u32,
    pub cliff_ledger: Option<u32>,
    pub end_ledger: u32,
    pub claimed_amount: i128,
}
```

Every entrypoint automatically extends instance TTL:
- `INSTANCE_EXTEND_AMOUNT` = 30 days (518,400 ledgers)
- `INSTANCE_TTL_THRESHOLD` = 29 days (501,120 ledgers)

## Error Index

| Error Code | Variant | Cause |
|---|---|---|
| 1 | `AlreadyInitialized` | `__constructor` invoked more than once on the same contract instance. |
| 2 | `InvalidAmount` | `total_amount` passed to constructor was $\le 0$. |
| 3 | `InvalidLedgerRange` | `start_ledger >= end_ledger`. |
| 4 | `InvalidCliffLedger` | `cliff_ledger` specified outside the $[start\_ledger, end\_ledger]$ range. |
| 5 | `NotInitialized` | `claim` or query called on an uninitialized contract instance. |

## Events

```rust
#[contractevent]
pub struct ScheduleCreated {
    #[topic]
    pub owner: Address,
    #[topic]
    pub token: Address,
    pub total_amount: i128,
    pub start_ledger: u32,
    pub cliff_ledger: Option<u32>,
    pub end_ledger: u32,
}

#[contractevent]
pub struct Claimed {
    #[topic]
    pub owner: Address,
    #[topic]
    pub to: Address,
    pub amount: i128,
    pub ledger: u32,
}
```
