# Inheritance Vault ("Dead-Man's-Switch") Spec v1

## 1. Purpose

`inheritance-vault` is a "personal contract" template: a small, pre-vetted satellite contract a Latch smart account deploys for itself, one instance per `owner`/`beneficiary` pair.

It implements an informal succession mechanism:

- funds stay under `owner`'s normal control as long as `owner` periodically proves they're still active (`check_in`)
- funds become claimable by a single designated `beneficiary` if `owner` goes silent for a configured number of ledgers

This is a different trigger model from a pure timelock vault. A timelock releases based on elapsed time alone and only ever answers to the one party who deployed it; this contract releases based on *inactivity*, and hands a second party — the beneficiary — real, if conditional, power over the funds once triggered. That trust-model difference, not the mechanics, is the main source of design risk here, and is called out explicitly below rather than left implicit.

## 2. Non-Goals

`inheritance-vault` does not handle:

- multiple beneficiaries or split inheritance — single beneficiary only for v1
- owner-side notification/reminder systems ("you haven't checked in in 30 days") — client UX, not a contract concern
- dispute or arbitration if `owner` is unreachable for a reason other than the intended one (lost device, lost keys, extended travel — vs. actually deceased or incapacitated) — this is a blunt instrument by design, not a substitute for real estate planning
- storage-TTL keep-alive during a long silent period — an operational/client concern (see §8)

Those belong to other systems.

## 3. Public Types

```rust
struct VaultData {
    owner: Address,
    beneficiary: Address,
    inactivity_period_ledgers: u32,
    last_active_ledger: u32,
}
```

```rust
enum InheritanceVaultError {
    InvalidBeneficiary = 1,
    InvalidInactivityPeriod = 2,
    AlreadyClaimable = 3,
    NotYetClaimable = 4,
    NoFundsToClaim = 5,
}
```

## 4. Public Interface

```rust
fn __constructor(e: Env, owner: Address, beneficiary: Address, inactivity_period_ledgers: u32)
fn check_in(e: &Env)
fn update_beneficiary(e: &Env, new_beneficiary: Address)
fn extend_inactivity_period(e: &Env, new_inactivity_period_ledgers: u32)
fn claim(e: &Env, token: Address, to: Address) -> i128
fn is_claimable(e: &Env) -> bool
fn get_vault_data(e: &Env) -> VaultData
```

## 5. Deployment Model

- One `inheritance-vault` instance per `owner`/`beneficiary` pair — not a shared singleton like `ed25519-verifier` or `threshold-policy`.
- Deployed as a satellite contract owned by a Latch smart account, via that account's own account-authorized contract-deployment entrypoint, using the account's existing signer/policy stack to gate the deployment itself. Discoverable the same way as any other satellite the account has deployed.
- `__constructor` performs no `require_auth` of its own — authorization for the deployment happens one level up, at the account's deployment entrypoint, the same way `fee-forwarder`'s constructor doesn't re-check the admin's authorization that already gated who could deploy it.

## 6. State Machine

The vault has exactly two states, determined purely from stored data and the current ledger sequence — there is no separate persisted "claimed" flag:

- **Active**: `e.ledger().sequence() < last_active_ledger + inactivity_period_ledgers`. `owner` controls the vault.
- **Claimable**: `e.ledger().sequence() >= last_active_ledger + inactivity_period_ledgers`. `beneficiary` can claim. This state is permanent for the life of the vault instance — see §6.1.

`is_claimable()` computes this directly from `get_vault_data()`; there is no other source of truth.

### 6.1 No Owner Override Once Claimable

Once claimable, `check_in`, `update_beneficiary`, and `extend_inactivity_period` are **all** rejected with `AlreadyClaimable`, even with valid `owner` authorization.

This is deliberate. Without it, `owner` — or anyone who can get `owner` to sign, including under duress, or simply an owner surfacing the moment they notice a pending claim — could grief a legitimate claim in flight by resetting the clock at the last second. Once triggered, the vault stays triggered; there is no path back to owner control for that instance. A `beneficiary` who wants to give `owner` more time has no mechanism to do so either (see §9) — the threshold, once crossed, is final in both directions.

### 6.2 Pre-Threshold Owner Reconfiguration

Before the threshold is reached, `owner` may at any time (each individually gated by `owner.require_auth()`):

- call `check_in` to reset `last_active_ledger` to now
- call `update_beneficiary` to change `beneficiary` (rejects `new_beneficiary == owner`)
- call `extend_inactivity_period` to change `inactivity_period_ledgers` (rejects `0`; the name says "extend" but the method accepts any positive value — shortening is also allowed, since this only affects the owner's own vault and their own security posture)

## 7. Validation Rules

### 7.1 Constructor

- `beneficiary != owner` — otherwise `InvalidBeneficiary`
- `inactivity_period_ledgers != 0` — otherwise `InvalidInactivityPeriod`
- `last_active_ledger` is seeded to the deployment ledger — deploying the vault counts as the first check-in

### 7.2 `check_in`

- requires `owner.require_auth()`
- rejects `AlreadyClaimable` if already past threshold (§6.1)
- resets `last_active_ledger` to `e.ledger().sequence()`

### 7.3 `update_beneficiary`

- requires `owner.require_auth()`
- rejects `AlreadyClaimable` if already past threshold
- rejects `InvalidBeneficiary` if `new_beneficiary == owner`

### 7.4 `extend_inactivity_period`

- requires `owner.require_auth()`
- rejects `AlreadyClaimable` if already past threshold
- rejects `InvalidInactivityPeriod` if `new_inactivity_period_ledgers == 0`

### 7.5 `claim`

- requires `beneficiary.require_auth()`
- rejects `NotYetClaimable` if not yet past threshold
- rejects `NoFundsToClaim` if this contract's balance of `token` is `0`
- transfers the contract's **entire** balance of `token` to `to` in one call and returns the amount transferred — there is no partial claim and no beneficiary-side "wait" mechanism (see §9)

## 8. Storage TTL Is an Operational Concern

This contract is meant to sit dormant for potentially very long stretches — that is the entire point of a dead-man's switch. Every state-changing call (`__constructor`, `check_in`, `update_beneficiary`, `extend_inactivity_period`, `claim`) extends the instance storage TTL generously (`VAULT_EXTEND_AMOUNT`, matching the common network ceiling on a single extension request, ~180 days).

That is not sufficient on its own for an inactivity window approaching or exceeding that ceiling with no other activity: if neither `owner` nor `beneficiary` submits a transaction for long enough, the ledger entries can still expire the way any Soroban contract's can, independent of `inactivity_period_ledgers`. Keeping the entry alive across a genuinely long silence is a permissionless "extend footprint TTL" operation any address can submit for any contract, with no authorization from `owner` or `beneficiary` required. This is a client/keep-alive concern — the same category as owner notifications (§2) — not something the contract's own logic can or should manage.

## 9. Explicit Design Decisions

Two decisions called out for future reference, since they shape the trust model:

- **No owner override after the threshold.** Answered in §6.1: no, deliberately, to prevent griefing a legitimate claim.
- **No beneficiary-side "give the owner more time."** Not needed for v1: once the threshold passes, `beneficiary` can claim, full stop. Adding a beneficiary-initiated delay would reintroduce exactly the kind of override §6.1 exists to prevent, just from the other party.

## 10. Failure Cases

The contract must reject:

- constructing with `beneficiary == owner`
- constructing with `inactivity_period_ledgers == 0`
- `check_in` / `update_beneficiary` / `extend_inactivity_period` without valid `owner` authorization
- `check_in` / `update_beneficiary` / `extend_inactivity_period` once the vault is claimable
- `update_beneficiary` with `new_beneficiary == owner`
- `extend_inactivity_period` with a new period of `0`
- `claim` without valid `beneficiary` authorization
- `claim` before the inactivity threshold is reached
- `claim` when the contract holds no balance of the requested token

## 11. Security Invariants

- `owner` and `beneficiary` are the only two addresses that can ever authorize a state change; no third party has any privileged access
- once claimable, the vault cannot be returned to the active state by any caller, including `owner`
- `claim` is beneficiary-gated and threshold-gated together — neither condition alone is sufficient, mirroring the "two independent gates" (auth + ledger check) shape used elsewhere for time-conditioned release
- this contract is a blunt instrument: it cannot and does not attempt to distinguish "owner is deceased/incapacitated" from any other reason `owner` stopped checking in

## 12. Known Gap: End-to-End Deployment Test

The account-authorized contract-deployment entrypoint this template is meant to be installed through is not yet implemented in this workspace. Until that lands, `inheritance-vault` is verified with unit tests deploying it directly (`Env::register`), matching how every other satellite/policy contract in this workspace is tested today. An end-to-end test deploying via that entrypoint from a real smart account should be added once it exists — it is out of scope for this contract's own implementation, which does not depend on the entrypoint's internals beyond being constructor-compatible with a generic `deploy_contract(wasm_hash, salt, constructor_args)`-style call.
