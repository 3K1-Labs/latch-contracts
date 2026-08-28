# Context Rules & Policies at Account Creation

> **Status: Decision record, not an active spec.** Written to evaluate whether the factory should
> accept extra context rules/policies at create time. Conclusion: the two creation-time scenarios
> that actually matter — single-admin and M-of-N multisig accounts — are already fully supported,
> atomically, by today's factory (`AccountInitParams.threshold` + `build_account_policies`,
> tested in `account-factory/contracts/factory-contract/src/test.rs`). The remaining case this doc
> covers (atomically attaching extra `CallContract`-scoped rules at create time) was tracked as
> [#54](https://github.com/3K1-Labs/latch-contracts/issues/54), closed as not planned in favor of
> the lighter alternative below. Kept here as a reference in case a concrete atomic-create
> requirement shows up later — see that issue's closing comment for the full reasoning, including
> which parts of "Settled design" below are genuinely settled vs. still open if this is ever
> revisited.

What it would take for Latch to let callers choose context rules and policies when creating a smart account — instead of always getting a single `Default` rule — and whether that path is worth taking.

---

## Today

Every Latch account is born the same way:

1. The factory’s `create_account` accepts only `signers`, optional `threshold`, and `account_salt` (`AccountInitParams` in `account-factory/contracts/factory-contract/src/lib.rs`).
2. It deploys the smart account with constructor args `(signers, policies)`.
3. The constructor **always** creates one `Default` rule named `"default"` with no expiry (`latch-smart-account/src/lib.rs`).

Policies at create time are factory-invented, not user-chosen:

| Account shape | Policies map |
|---------------|--------------|
| Single signer | Empty |
| Multisig | Threshold policy only (`build_account_policies`) |

Address salt is `latch.factory.account.v2` over salt + canonical signers + effective threshold. Initial rules/policies are **not** in the preimage (`factory-spec.md` §9).

---

## What full create-time choice requires

This is a **contract + factory** change, not a client-only tweak.

| Layer | Change |
|-------|--------|
| `latch-smart-account` | New `__constructor` that accepts rule inits (type, name, expiry, signers, policies) instead of hardcoding `Default` / `"default"` |
| Factory `AccountInitParams` | Carry optional extra rules / typed policy installs; validate and translate into constructor args |
| Address salt | Bump to **`v3`** and include canonical rule/policy material — otherwise same signers + different rules collide on one address |
| `FactoryConfig` | Whitelist known policy singletons (threshold, session, spending-limit, …); do not accept arbitrary policy addresses from the client |
| Deploy | New account WASM hash + **new factory** — factory config is immutable (`UPGRADE_PATH.md`) |
| Spec / tests / dummy | Update `factory-spec.md`, READMEs, factory tests, and `dummy-account` constructor signature |

---

## Hard constraints (do not ignore)

- **Spending-limit and session policies require `CallContract`.** They reject install on `Default`. A global “$200 spend limit on the account” is not expressible on the default rule with today’s policy crates.
- **Never create an account with no admin lane.** A create that only installs a narrow `CallContract(dApp)` rule can brick self-management (`add_signer`, `add_context_rule`, `upgrade`, …). Always keep a `Default` (or equivalent) admin rule.
- **OZ per-rule limits still apply** (max signers, max policies, name size; at least one signer or one policy).
- **Policy install params are typed.** Each policy expects its own `Val` shape; the factory must encode them correctly or the constructor panics mid-deploy.
- **Policy address trust.** Whitelist singletons in factory config. Passing arbitrary addresses makes authorization depend on untrusted code.

---

## Is this path worth it?

**For onboarding UX: prefer the lighter alternative below.**

Full create-time choice is justified only if product needs **atomic** “account exists fully configured in one deploy” — no follow-up auth, no second step. Otherwise it is a breaking factory API, salt version bump, and redeploy cycle for something clients can already approximate after create.

---

## Lighter alternative (recommended for onboarding)

Keep today’s factory create (always get `Default`).

Then, in the same transaction or a follow-up, call the account’s existing `SmartAccount` methods:

- `add_context_rule` — e.g. `CallContract(token_or_dapp)` with session signers / expiry
- `add_policy` — spending-limit, session allowlist, etc. on that rule

No salt bump, no new constructor, no new factory. The on-chain surface already exists; client UI and a worked reference are the gaps (see [latch-contracts#45](https://github.com/3K1-Labs/latch-contracts/issues/45)).

```
create_account  →  Default admin rule
       ↓
add_context_rule / add_policy  →  scoped rules (optional)
```

---

## Settled design if the full path is built later

1. Constructor **always** creates one **Default admin** rule first (signers + threshold policy when multisig).
2. Constructor may create **optional additional** rules in the same call (e.g. `CallContract` + spending-limit / session).
3. Factory whitelists policy contract addresses; install params are typed per policy kind.
4. Salt preimage bumps to `latch.factory.account.v3` and includes canonical extra-rule material.
5. Clients point at a newly deployed factory; existing accounts stay on the old factory.

Do **not** ship a create path that omits the Default admin rule.

**Not actually settled yet, despite the heading above** — flagged during #54's triage: which specific policies get whitelisted (point 3), the exact v3 preimage byte encoding (point 4), and the exact extra-rule parameter shape (point 2) all still need a maintainer decision before a contributor could implement this without inventing security-relevant details themselves. See #54's closing comment.
