# Timelock Policy for Delayed Execution — Audit & Implementation Plan

> **Status:** Draft for review. No code written. Awaiting approval before proceeding.
> **Branch:** `feature/timelock-policy` (created off `fork/main`, clean and up to date)
> **Scope:** v1 — one fixed delay per context rule installation, no variable per-action delay, no guardian-triggered recovery.

---

## 1. Audit Findings

### 1.1 The `Policy` Trait and Authorization Flow

**Source:** `stellar_accounts::policies::Policy` (v0.7.2)

The `Policy` trait defines three lifecycle hooks:

| Method | When called | Purpose |
|---|---|---|
| `install` | When a policy is added to a context rule via `add_policy()` | Initialize storage, validate params |
| `enforce` | During `do_check_auth()` — the authorization hot path | Validate + mutate state; **must panic** if conditions unmet |
| `uninstall` | When a policy is removed from a context rule | Clean up storage |

**Critical constraint:** `enforce()` is called *synchronously inside a single transaction's authorization phase*. It receives the `Context` (target contract, function name, args), the authenticated signers, and the context rule. It is called by `do_check_auth()` as part of the `CustomAccountInterface::__check_auth` flow — there is no mechanism within this flow to defer execution to a later transaction.

**Key insight for the timelock design:** `enforce()` can *store* a proposal (state mutation is permitted and expected — see `spending_limit` recording spend entries), but it **cannot** trigger the actual execution of the proposed action. Execution must happen in a separate transaction, invoked independently of the authorization flow.

### 1.2 `do_check_auth` Call Path (Full Trace)

```
Smart account __check_auth(signature_payload, signatures, auth_contexts)
  └─ smart_account::do_check_auth(e, signature_payload, signatures, auth_contexts)
       ├─ 1. Validate context_rule_ids length matches auth_contexts
       ├─ 2. For each (context, rule_id):
       │     └─ get_validated_context_by_id(e, context, all_signers, id)
       │          ├─ Fetch ContextRule from storage
       │          ├─ Reject if rule expired (valid_until < current ledger)
       │          ├─ Reject if context type doesn't match rule type
       │          └─ Filter matched signers from all_signers
       ├─ 3. Collect all allowed signers from all rules
       ├─ 4. Bind context_rule_ids into signed digest (anti-downgrade)
       ├─ 5. Authenticate each signer (Delegated → require_auth_for_args; External → verifier.verify)
       └─ 6. For each validated (rule, context, matched_signers):
             └─ For each policy in rule.policies:
                   PolicyClient::new(e, &policy).enforce(context, matched_signers, rule, smart_account)
```

**What this means for the timelock:** Steps 1–5 handle cryptographic auth. Step 6 is where policies intervene. A timelock's `enforce()` runs at step 6 and can store proposal data, but the transaction is still in the authorization phase — the actual contract call (the `Context::Contract` invocation) hasn't happened yet. The call happens *after* `do_check_auth` returns `Ok(())`.

### 1.3 The `execute()` Entrypoint

**Source:** `ExecutionEntryPoint` trait in `stellar_accounts::smart_account`

```rust
fn execute(e: &Env, target: Address, target_fn: Symbol, target_args: Vec<Val>) {
    e.current_contract_address().require_auth();
    e.invoke_contract::<Val>(&target, &target_fn, target_args);
}
```

`LatchSmartAccount` implements this trait with no overrides — it's a one-line forwarder.

**Key findings:**
- `execute()` is a public entrypoint on the smart account contract
- It requires `require_auth()` from the smart account itself (self-authorization)
- It calls `e.invoke_contract()` to forward the call to the target
- It does **not** invoke any policy's `enforce()` — it's a raw forwarder
- There is no hook, callback, or extension point where a policy could intercept `execute()` calls

**Answer to the open question:** The second execute step (actually performing the delayed action) **does NOT need `latch-smart-account` cooperation.** The timelock policy contract can implement its own `execute()` entrypoint as an independent function. The smart account's `execute()` entrypoint is only relevant for the *proposal* phase (where the smart account calls the timelock contract to propose an action). The *execution* phase is a direct call to the timelock contract by any authorized party.

### 1.4 Existing "Canceller" / "Another Signer" Concepts

There is **no existing "designated canceller" primitive** in the codebase. The relevant concepts are:

- **`ContextRule` signers:** A set of `Signer` values (Delegated or External) bound to a context rule. Any signer in the rule can authenticate for actions under that rule.
- **`authenticated_signers`:** Passed to `enforce()` — the subset of rule signers that actually authenticated for this particular call.
- **`remove_context_rule`:** The existing mechanism for revoking a rule's authorization entirely (drops all pending session-level auth).
- **No per-action cancellation:** There is no concept of cancelling a specific pending action today.

**For the timelock design:** The "cancellable-by" list should be derived from the context rule's signers at proposal time. Any signer on the rule (or a subset explicitly designated at installation) can cancel. This is grounded in the existing `Signer` primitive, not a new concept.

### 1.5 Existing Policy Crate Conventions

**Reviewed crates:** `session-policy`, `threshold-policy`, `weighted-threshold-policy`, `spending-limit-policy`

| Convention | Detail |
|---|---|
| **File layout** | `src/lib.rs` (main), `src/test.rs` (tests), `Cargo.toml` |
| **Error handling** | `#[contracterror]` enum, `panic_with_error!(e, Error::Variant)`, error numbering starts fresh at 1 per crate |
| **Storage** | `#[contracttype]` enum for storage keys, `e.storage().persistent().set/get/has/remove`, TTL extension on reads |
| **Events** | `#[contractevent]` structs with `#[topic]` on identity fields, published inline (convention target: `emit_*` helpers) |
| **Auth** | `smart_account.require_auth()` in `enforce()` and `install()` |
| **Tests** | `#![cfg(test)] extern crate std;`, `Env::default()`, `e.register(MockContract, ())`, `e.mock_all_auths()`, `e.as_contract()`, `#[should_panic(expected = "Error(Contract, #N)")]` |
| **Crate shape** | Two shapes: "thin wrapper" (delegates to OZ upstream) or "own logic" (session-policy style with section banners and full rustdoc) |
| **Cargo.toml** | `[lib] crate-type = ["lib", "cdylib"]`, `doctest = false`, workspace deps via `{ workspace = true }` |

**The timelock policy will be "own logic"** — it has no OZ upstream equivalent. It should follow `session-policy`'s style with section banners, full rustdoc, and its own storage/error/event modules.

### 1.6 CI Matrix and Workspace Config

**Workspace root (`Cargo.toml`):** The new crate must be added to `members`:
```toml
members = [
  # ... existing members ...
  "policies/timelock-policy",
]
```

**CI matrix (`.github/workflows/rust.yml`):** The `build-and-test` job uses a matrix of `workspace` paths. Add:
```yaml
matrix:
  workspace:
    # ... existing entries ...
    - policies/timelock-policy
```

**No branch ruleset files** exist in `.github/` — branch protection is configured via GitHub UI settings, not checked-in files. No changes needed there.

**`Cargo.toml` for the crate** (following conventions):
```toml
[package]
name = "timelock-policy"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
crate-type = ["lib", "cdylib"]
doctest = false

[dependencies]
soroban-sdk = { workspace = true }
stellar-accounts = { workspace = true }

[dev-dependencies]
soroban-sdk = { workspace = true, features = ["testutils"] }
stellar-accounts = { workspace = true }
```

### 1.7 Zodiac Delay Module — Prior Art Analysis

**Source:** `gnosisguild/zodiac-modifier-delay`

| Zodiac concept | Directly transferable? | Soroban adaptation |
|---|---|---|
| **Queue transactions** via `execTransactionFromModule()` | ✅ Yes — maps to `propose()` | Store pending proposal in timelock contract storage |
| **Cooldown period** before execution | ✅ Yes — maps to delay in ledgers | Use `e.ledger().sequence() + delay_ledgers` as unlock time |
| **`executeNextTx()`** — public function anyone can call | ✅ Yes — maps to `execute_pending()` | Anyone (or designated executors) can trigger after delay |
| **Skip transactions by advancing nonce** | ⚠️ Partially | Not needed in v1. Can be added later as "cancel + re-propose" |
| **Cooldown + expiration** | ⚠️ Expiration not in v1 scope | `valid_until` on the context rule already handles expiry at the rule level |
| **Ordered execution** (nonce-based queue) | ❌ Not needed | Latch's timelock is per-proposal, not a global queue. Each proposal is independent. |
| **Module enable/disable** | ❌ Not needed | Handled by `install()`/`uninstall()` on the context rule |

**Key transferable insight:** Zodiac's model is "queue → wait → execute, with cancel window." This maps cleanly to Latch's "propose → delay → execute, with cancel-before-unlock." The main difference is that Zodiac operates as a Safe module (intercepting Safe transactions), while Latch's timelock operates as a policy (intercepting authorization). The execution model is fundamentally different, but the state machine is the same.

**What does NOT transfer:** Zodiac's nonce-ordered queue, module lifecycle management, and the owner-can-skip pattern. These are EVM/Safe-specific and don't apply to Soroban's per-context-rule model.

---

## 2. Implementation Plan

### 2.1 Storage Design

```rust
/// Storage keys for timelock policy data.
#[contracttype]
pub enum TimelockStorageKey {
    /// Per-installation configuration: delay_ledgers, cancellable_by list.
    /// Keyed by (smart_account, context_rule_id).
    Config(Address, u32),
    /// Pending proposal: keyed by (smart_account, proposal_id).
    Proposal(Address, u32),
    /// Next proposal ID counter: keyed by (smart_account, context_rule_id).
    NextProposalId(Address, u32),
}

/// Configuration stored at install time.
#[contracttype]
pub struct TimelockConfig {
    /// Number of ledgers that must pass between propose and execute.
    pub delay_ledgers: u32,
    /// Addresses that can cancel pending proposals (subset of rule signers,
    /// or all signers if empty — meaning "any rule signer can cancel").
    pub cancellable_by: Vec<Address>,
}

/// A pending proposal awaiting its delay window.
#[contracttype]
pub struct PendingProposal {
    /// The target contract to call on execution.
    pub target: Address,
    /// The function name to invoke.
    pub fn_name: Symbol,
    /// The arguments to pass.
    pub args: Vec<Val>,
    /// The ledger sequence at which this proposal becomes executable.
    pub unlock_ledger: u32,
    /// The address that proposed this action.
    pub proposer: Address,
}
```

**Storage TTL:** Following `session-policy`'s convention, use persistent storage with TTL extension on reads (`SESSION_TTL_THRESHOLD` / `SESSION_EXTEND_AMOUNT` pattern — adapted for the timelock's own constants).

### 2.2 Error Codes

```rust
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum TimelockError {
    /// The policy is not installed for this smart account and context rule.
    NotInstalled = 1,
    /// The policy is already installed for this smart account and context rule.
    AlreadyInstalled = 2,
    /// The delay must be greater than zero.
    InvalidDelay = 3,
    /// Only CallContract context rules are supported.
    OnlyCallContractAllowed = 4,
    /// The action cannot be executed yet — the delay has not elapsed.
    DelayNotElapsed = 5,
    /// The proposal does not exist or has already been executed/cancelled.
    ProposalNotFound = 6,
    /// The caller is not authorized to cancel this proposal.
    UnauthorizedCancel = 7,
    /// The proposal has already been executed.
    AlreadyExecuted = 8,
    /// No authenticated signers provided for proposal.
    NoAuthenticatedSigners = 9,
}
```

Numbering starts fresh at 1, per the repo convention for independently deployed contracts.

### 2.3 Events

```rust
#[contractevent]
#[derive(Clone, Debug)]
pub struct TimelockProposed {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub proposal_id: u32,
    pub target: Address,
    pub fn_name: Symbol,
    pub unlock_ledger: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TimelockExecuted {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub proposal_id: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TimelockCancelled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub proposal_id: u32,
}
```

### 2.4 Function Signatures

#### `install`

```rust
pub fn install(
    e: &Env,
    params: &TimelockAccountParams,  // { delay_ledgers: u32, cancellable_by: Vec<Address> }
    context_rule: &ContextRule,
    smart_account: &Address,
)
```

- Requires `smart_account.require_auth()`.
- Validates `delay_ledgers > 0`.
- Validates `context_rule.context_type` is `CallContract` (not `Default` or `CreateContract`).
- Stores `TimelockConfig` in persistent storage.
- Emits `TimelockInstalled` event.

#### `uninstall`

```rust
pub fn uninstall(
    e: &Env,
    context_rule: &ContextRule,
    smart_account: &Address,
)
```

- Requires `smart_account.require_auth()`.
- Removes `TimelockConfig` and any pending proposals for this `(smart_account, context_rule_id)`.
- Emits `TimelockUninstalled` event.

#### `enforce`

```rust
fn enforce(
    e: &Env,
    context: &Context,
    authenticated_signers: &Vec<Signer>,
    context_rule: &ContextRule,
    smart_account: &Address,
)
```

- Called by `do_check_auth()` during authorization.
- Requires `smart_account.require_auth()`.
- Validates at least one signer authenticated.
- Extracts `target`, `fn_name`, and `args` from the `Context::Contract` variant.
- Stores a `PendingProposal` with `unlock_ledger = e.ledger().sequence() + config.delay_ledgers`.
- Returns the `proposal_id` (via event or return value — **see design decision below**).
- Emits `TimelockProposed` event.

#### `execute_pending` (entrypoint on the timelock contract)

```rust
pub fn execute_pending(
    e: Env,
    smart_account: Address,
    proposal_id: u32,
)
```

- Reads the `PendingProposal` from storage.
- Validates `e.ledger().sequence() >= proposal_id.unlock_ledger`.
- Removes the proposal from storage (one-shot: can't re-execute).
- Calls `e.invoke_contract(&proposal.target, &proposal.fn_name, proposal.args)`.
- Emits `TimelockExecuted` event.

#### `cancel` (entrypoint on the timelock contract)

```rust
pub fn cancel(
    e: Env,
    smart_account: Address,
    proposal_id: u32,
)
```

- Reads the `PendingProposal` from storage.
- Validates the caller is in the `cancellable_by` list (or is a signer on the context rule if `cancellable_by` is empty).
- Removes the proposal from storage.
- Emits `TimelockCancelled` event.

#### `get_pending_proposals` (read-only entrypoint)

```rust
pub fn get_pending_proposals(
    e: &Env,
    smart_account: &Address,
    context_rule_id: u32,
) -> Vec<PendingProposal>
```

Returns all pending (not yet executed or cancelled) proposals for a given smart account and context rule.

### 2.5 Design Decision: How Does `enforce()` Communicate the Proposal ID?

The `Policy::enforce()` trait signature returns `()` — it cannot return a value to the caller. But the caller (the smart account's authorization flow) needs to know the proposal ID to later call `execute_pending()`.

**Proposed approach:** The proposal ID is communicated entirely through the `TimelockProposed` event. The event includes `proposal_id`, `smart_account`, and `context_rule_id`. Off-chain clients (or on-chain watchers) can index events to find the proposal ID. On-chain, `execute_pending()` can also accept a linear scan of proposals for a given `(smart_account, context_rule_id)` if needed — but the event-based approach is cleaner and matches how Soroban event indexing works.

**Alternative considered:** Having `propose()` as a separate entrypoint (not going through `enforce()`). This would allow returning the proposal ID directly. However, this breaks the policy model — `enforce()` is the canonical hook for intercepting authorized calls. Keeping the proposal inside `enforce()` ensures the timelock delay applies to *every* call through the context rule, not just calls explicitly routed to a `propose()` function. A separate `propose()` entrypoint would require callers to remember to use it, creating an escape hatch.

**Recommendation:** Use `enforce()` for proposals (mandatory delay for all calls), communicate proposal IDs via events. This is the safest v1 design.

### 2.6 v1 Scope Confirmation

| Feature | In v1? | Notes |
|---|---|---|
| One fixed delay per context rule | ✅ Yes | Set at `install()` time, applies to all proposals under that rule |
| Variable per-action delay | ❌ No | Would require changes to the `enforce()` signature or a separate `propose()` entrypoint |
| Guardian-triggered recovery | ❌ No | Different trigger model, per Discussion #31 |
| Cancellation by designated signers | ✅ Yes | Via `cancellable_by` list at install time |
| Proposal expiry | ❌ No | Not in v1. The context rule's `valid_until` provides a natural boundary. |
| Multiple concurrent proposals | ✅ Yes | Each proposal gets a unique ID; multiple can coexist |
| Nonce-ordered execution | ❌ No | Each proposal is independent, not ordered in a queue |

### 2.7 Test Plan

Matching the acceptance criteria from the issue:

| # | Test | Expected result |
|---|---|---|
| 1 | **Propose → execute after delay succeeds** | Call `execute_pending()` at `unlock_ledger` or later → target function is called, proposal removed |
| 2 | **Execute before delay fails** | Call `execute_pending()` before `unlock_ledger` → panics with `DelayNotElapsed` |
| 3 | **Cancel before unlock blocks execution** | `cancel()` then `execute_pending()` → panics with `ProposalNotFound` |
| 4 | **Cancel after unlock is a no-op/error** | `execute_pending()` then `cancel()` → panics with `ProposalNotFound` |
| 5 | **Propose rejects non-CallContract rule** | Install on `Default` rule → panics with `OnlyCallContractAllowed` |
| 6 | **Propose rejects zero delay** | Install with `delay_ledgers = 0` → panics with `InvalidDelay` |
| 7 | **Unauthorized cancel rejected** | Caller not in `cancellable_by` → panics with `UnauthorizedCancel` |
| 8 | **Enforce requires auth** | Call without smart account auth → panics (auth failure) |
| 9 | **Install requires auth** | Call without smart account auth → panics |
| 10 | **Uninstall cleans up proposals** | Install → propose → uninstall → `get_pending_proposals` returns empty |
| 11 | **Integration with do_check_auth** | Set up smart account with timelock policy, call `do_check_auth` → proposal stored, verify via events |
| 12 | **Multiple proposals coexist** | Propose two actions → both have unique IDs, both can be executed independently |
| 13 | **Get pending proposals returns correct list** | Propose 3 actions, cancel 1 → `get_pending_proposals` returns 2 |

**Test structure** (following `session-policy/src/test.rs` conventions):
- `extern crate std;`
- `MockContract` for the timelock contract itself
- `MockTargetContract` for testing actual execution
- `e.mock_all_auths()` for standard tests
- `e.as_contract()` for direct function calls
- `#[should_panic(expected = "Error(Contract, #N)")]` for error cases

---

## 3. Open Questions for Maintainer Review

1. **Proposal ID communication:** Is the event-based approach acceptable, or should we consider a separate `propose()` entrypoint that returns the ID? The event approach is safer (mandatory delay for all calls) but requires off-chain indexing to map proposal IDs.

2. **Cancellation authorization:** Should cancellation be restricted to the `cancellable_by` list only, or should any signer on the context rule also be able to cancel? The `cancellable_by` list provides more granular control.

3. **Storage cleanup on uninstall:** Should `uninstall()` fail if there are pending proposals, or should it clean them up silently? The `try_uninstall` pattern in `remove_context_rule` suggests silent cleanup is acceptable.

4. **`execute_pending` authorization:** Who can call `execute_pending()`? The Zodiac model makes it permissionless ("anyone can execute the next transaction"). For v1, should it be permissionless (any address), or restricted to signers on the context rule?

5. **Context type restriction:** Should the timelock only support `CallContract` rules, or also `Default` rules? `Default` rules match any context, which means the timelock would need to handle `CreateContract` contexts too. Recommendation: `CallContract` only for v1.

---

## 4. Files to Create/Modify

| File | Action | Purpose |
|---|---|---|
| `policies/timelock-policy/Cargo.toml` | **Create** | Crate manifest |
| `policies/timelock-policy/src/lib.rs` | **Create** | Main contract: types, errors, events, `Policy` impl, entrypoints |
| `policies/timelock-policy/src/test.rs` | **Create** | Test suite |
| `Cargo.toml` (workspace root) | **Edit** | Add `"policies/timelock-policy"` to `members` |
| `.github/workflows/rust.yml` | **Edit** | Add `policies/timelock-policy` to CI matrix |

---

## 5. Next Steps (After Approval)

1. Implement `policies/timelock-policy/Cargo.toml`
2. Implement `policies/timelock-policy/src/lib.rs` (types → errors → events → contract → Policy impl → entrypoints → helpers)
3. Implement `policies/timelock-policy/src/test.rs`
4. Wire into workspace root `Cargo.toml` and CI matrix
5. Run `cargo fmt --all`, `cargo clippy`, `cargo test` from `policies/timelock-policy/`
6. Run full workspace `cargo test` to verify no regressions
7. Commit and open PR
