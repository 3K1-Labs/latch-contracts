# Account & Factory Upgrade Path

## The Problem

Soroban contracts are immutable by default once deployed. Two different pieces of this system
will eventually need to change after real accounts exist on-chain:

1. **The smart account's own core logic** — a new context-rule model, a new `stellar-accounts`
   version with breaking changes, a genuinely new "account standard."
2. **The factory's configuration** — which `smart-account` wasm hash gets deployed for new
   accounts going forward.

What happens in each case, and who is responsible for it, needs to be decided before real users
are on these contracts — not worked out after the fact under pressure.

---

## What's Already True Today

None of this required new code to discover — it's already how the architecture behaves, it just
hadn't been written down in one place.

### A three-tier mutability model already exists

| Layer | Mutable after deploy? | How |
|---|---|---|
| Core account logic (`__check_auth`, context rule engine) | **No** | Only by deploying a new account and migrating signers/funds |
| Policies (threshold, session, spending-limit) | **Yes, per-account, opt-in** | Account owner calls `remove_policy` + `add_policy` to point their context rule at a new policy contract address |
| Verifiers (Ed25519 / WebAuthn / Secp256k1) | **Yes, per-signer, opt-in** | Account owner calls `remove_signer` + `add_signer` to re-register against a new verifier address |

`latch-smart-account/README.md`'s security section already states this as a deliberate choice:
"No external admin. No owner key, no upgrade proxy. Only the account's own signers can mutate it."

### The factory already speced this, it just wasn't explained

`factory-spec.md` § 5 already says:

> - config is set once at construction
> - no admin update methods in v1
> - new code versions require deploying a new factory

Today's decision (below) confirms that and gives it the reasoning it was missing, rather than
changing it.

---

## Decision 1: The factory stays fully immutable

**No admin-settable `wasm_hash`, no timelock, no multisig on the factory.** A new smart-account
version means deploying an entirely new factory contract, and repointing clients (web extension,
mobile app, dApp) at its address.

### Alternatives considered and rejected

- **Single admin key that can call `set_wasm_hash`.** Rejected: a single key is a single point of
  failure with no recovery story. If it's lost, the factory can never be updated again through
  that mechanism (a new factory would be needed anyway, at that point defeating the purpose).
- **Multisig admin.** Rejected for now: the team doesn't currently trust its own key-management
  discipline enough to rely on multiple people correctly custodying signing keys long-term. A
  multisig is only as strong as the operational practices behind it, and those don't exist yet.
- **Timelock in front of either of the above.** Rejected: a timelock adds a public delay and
  reaction window before a proposed change executes, which mitigates a *silent, instant*
  compromise — it does not fix a *lost* key, and it does not make an untrusted custody model
  (single key or multisig) trustworthy. It's a mitigation layered on top of an authorization
  mechanism, not a replacement for needing one you trust.

### Why immutable-per-version is the better fit right now

- **No privileged key to lose, leak, or mismanage** — there's no settable admin function at all,
  so there's nothing to custody for this purpose.
- **No new audit surface.** A new factory deployment is the same, already-reviewed factory shape,
  redeployed — not a new privileged code path (`set_wasm_hash`) that needs its own review for
  subtle auth bugs.
- **The update becomes a real engineering event** — a PR, CI, a deliberate build and deploy —
  instead of a live transaction against a standing privileged function.
- **The cost is coordination, not security.** Client apps need to track which factory address is
  current, but that's public, non-secret config updated through normal app releases — not
  something that can be stolen.

---

## Decision 2: The smart account gets a self-authorized `upgrade()`

**Implemented (2026-08-20).** `LatchSmartAccount` now has an `upgrade()` entry point, gated the
same way every other mutation on the account already is:

```rust
use stellar_contract_utils::upgradeable::{self as upgradeable, Upgradeable};

#[contractimpl]
impl Upgradeable for LatchSmartAccount {
    fn upgrade(e: &Env, new_wasm_hash: BytesN<32>, _operator: Address) {
        e.current_contract_address().require_auth();
        upgradeable::upgrade(e, &new_wasm_hash);
    }
}
```

Built on OZ's own `Upgradeable` trait (`stellar-contract-utils`, pinned `=0.7.2` — same audited
release as `stellar-accounts`; `upgradeable` was in-scope for the `v0.7.0` audit). Verified this
exact self-authorized shape (`require_auth()` on the account's own address, `operator` parameter
present in the trait signature but deliberately unused) already exists as OZ's own reference
example, `examples/multisig-smart-account/account/src/contract.rs` — this isn't a novel pattern,
it's the documented way to use the trait.

### Why this matters beyond "nice to have"

Today, `upgrade()` doesn't exist on `LatchSmartAccount` at all. That's a different thing from "the
user chose not to have this power" — no amount of signer authority, no matter how high the
multisig threshold, can act on a function that was never written. That's a capability gap, not a
permission gate. A wallet that's pitched as *programmable* should mean the ceiling on what an
account can become is set by what its owner is willing to authorize, not by what shipped at
deploy time.

### What this does and doesn't change

- It does **not** introduce a new trusted party. The same signers who already control
  `add_signer`, `remove_policy`, and `execute` would be the ones who can call `upgrade()`.
- It does **not** affect the factory decision above — new accounts still come from a new factory
  when the core logic changes. `upgrade()` is what lets *existing* accounts opt into that new
  logic in place, without a full migration to a new address.
- It mirrors the existing pattern already used for Latch-added methods on top of what
  `SmartAccount for LatchSmartAccount {}` provides for free — see `batch_add_signer` in
  `latch-smart-account/src/lib.rs`, which follows the identical one-line
  `require_auth()`-then-delegate shape this would use.

---

## `upgrade()` — resolved

- **Function signature / host call**: `fn upgrade(e: &Env, new_wasm_hash: BytesN<32>, operator: Address)`,
  wrapping `env.deployer().update_current_contract_wasm(...)` via OZ's `upgradeable::upgrade` helper.
  Storage is preserved across the swap; the contract only changes after the invocation completes.
- **OZ's `Upgradeable` trait vs. hand-rolled**: using OZ's directly — see Decision 2 above.
- **Authorization tier**: `upgrade()` sits behind exactly the same `e.current_contract_address()
  .require_auth()` gate as every other administrative method on the account (`add_signer`,
  `remove_policy`, `add_context_rule`, ...). It does not introduce a new or weaker authorization
  surface — whatever context rule/policy configuration would already let a caller authorize
  `add_signer` today would equally authorize `upgrade()`. This is a pre-existing property of the
  whole account design, not something specific to `upgrade()`: OZ's own `v0.7.0` audit calls out
  that a `CallContract` rule grants full contract-level access to *every* `require_auth`-gated
  entrypoint on that contract, not just one function, and states plainly that "context rules are
  expected to be correctly configured" as a trust assumption of the library. Getting a session-key
  policy scoped too broadly is a real risk today for `add_signer` and `remove_policy` just as much
  as for `upgrade()` — worth being deliberate about when configuring any `CallContract` rule
  pointed at the account's own address, not something `upgrade()` specifically needs to solve.

## Still open

- Storage migration strategy, if a future account version changes stored state shape. OZ's own
  `upgradeable` module docs recommend one of: eager migration (bounded data), lazy migration
  (unbounded data), or enum wrappers for forward-compatible layouts — not yet decided which fits
  a future breaking `LatchSmartAccount` change, since no such change exists yet to migrate.
- Client-side (web extension / mobile / dApp) UX for surfacing "a new account version is
  available" and walking a user through authorizing the upgrade.
- No test yet proves an upgrade *changes behavior* (only that the mechanism succeeds end-to-end
  against the account's own current wasm) — would need a second, distinct compiled contract
  fixture to upgrade *to*, similar to how `factory-contract`'s tests embed pre-built `dummy-*`
  wasm files. Not done because OZ's own `stellar-contract-utils` test suite doesn't test this
  depth either — the mechanism itself (`update_current_contract_wasm`) is a thin, audited call
  into a Soroban host function, not custom logic of ours to re-verify.
