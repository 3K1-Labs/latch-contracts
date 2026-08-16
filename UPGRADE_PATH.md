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

## Decision 2: The smart account should get a self-authorized `upgrade()`

**Goal, not yet built.** The smart account should gain an `upgrade()` entry point, gated the same
way every other mutation on the account already is: `e.current_contract_address().require_auth()`,
at the account's full/default authorization tier — not reachable through a narrow session-key
policy scope.

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

## Not Yet Decided (next step, after this doc is reviewed)

- Exact `upgrade()` function signature and which Soroban host call it wraps.
- Which context-rule / authorization tier it must sit behind, and how to guarantee a session-key
  scoped policy can never accidentally authorize it.
- Whether to build on OZ's `Upgradeable` trait (`stellar-contract-utils`) directly, or hand-roll
  the equivalent — OZ's version is admin-role-gated by default and would need adapting to the
  self-authorized model described here.
- Storage migration strategy, if a future account version changes stored state shape.
- Client-side (web extension / mobile / dApp) UX for surfacing "a new account version is
  available" and walking a user through authorizing the upgrade.
