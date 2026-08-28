# Mainnet Readiness Checklist

What's actually still open before real user funds sit behind these contracts, pulled together
from `SECURITY.md`, `UPGRADE_PATH.md`, `TODO.md`, `BUILD.md`, and this session's own findings —
not a wishlist, a punch list. Update this as items close; don't let it drift the way `TODO.md`
almost did.

---

## 1. Security & Audit — the actual blocker

- [ ] **External security audit of Latch's own code.** `SECURITY.md` says this plainly: *"This
      project has not yet undergone an external security audit... do not deploy it to hold real
      value without your own independent review."* Nothing else on this list matters if this box
      stays unchecked. Note the scope distinction: OpenZeppelin's `stellar-accounts`/
      `stellar-contract-utils` (pinned `=0.7.2`) already went through OZ's own `v0.7.0` audit — the
      gap is specifically **Latch's own code built on top**: `account-factory`, `latch-smart-account`'s
      `upgrade()` wiring, all `policies/*` and `latch-verifiers/*` crates, and `session-policy`'s
      genuinely custom (non-OZ) logic.
- [ ] **Revisit the bug-bounty question.** Deferred in `TODO.md` "in favor of a plain email
      contact for now; reconsider closer to mainnet launch" — this is that moment.
- [ ] **Update `SECURITY.md`'s "Status" section** once the audit above actually happens — it's
      explicitly written to be revisited, not a permanent statement.
- [ ] **Cross-repo trust context, not this repo's bug, but relevant before real funds are at
      stake anywhere in the system**: a real, confirmed fund-loss incident on `latch-mobile`
      (logout destroying an unbacked-up wallet with no warning — private security advisory
      `GHSA-gcg8-6536-rvc3`) is still open as of this checklist. The contracts can be flawless and
      users can still lose funds through a client bug. Worth confirming client-side security
      posture before calling the *system* mainnet-ready, not just this repo.

## 2. Deployment freshness — everything on testnet is now stale

Per `BUILD.md`'s own stale-record annotations, **every current testnet deployment needs a fresh
redeploy before mainnet** — none of it can be "promoted" as-is:

- [ ] **Account Factory** — constructor signature changed (5 args → 4, `Secp256k1` dropped), salt
      version bumped `v1` → `v2`, instance TTL bumped 30 → 90 days. The deployed testnet factory
      predates all of this.
- [ ] **Ed25519 verifier decision.** The factory's `ed25519_verifier` slot currently points at what
      is now `demo/modified-ed25519-verifier` (the Phantom-popup-specific variant) — per this
      session's findings, that's explicitly *not* meant for production use. Before mainnet deploy,
      decide: point the factory at the new plain `ed25519-verifier` instead, and confirm that's
      actually compatible with every real client signer today (web-extension passkey path, mobile
      passkey path — neither currently uses raw Ed25519 without wrapping, so confirm nothing
      breaks).
- [ ] **Secp256k1 — in or out for launch?** The verifier crate was deleted entirely (unused,
      confirmed via both `latch-web-extension` and `latch-mobile` audits). Decide explicitly
      whether mainnet launches without secp256k1 support at all (recommended, given zero current
      usage) rather than leaving an implicit gap.
- [ ] **All verifier/policy singletons redeployed and re-recorded** — `ed25519-verifier`,
      `webauthn-verifier`, `threshold-policy`, `weighted-threshold-policy`, `session-policy`,
      `spending-limit-policy` — with `BUILD.md` updated with real mainnet addresses, not testnet
      ones carried over.
- [ ] **Deployer identity operational security.** The factory has deliberately no admin key
      (`UPGRADE_PATH.md` Decision 1), but *something* signs the mainnet deployment transactions
      themselves. Confirm who holds that key and how it's secured — a compromised deployer key
      during initial mainnet rollout is a real, if narrow, window of risk even though the deployed
      factory itself has no ongoing privileged function.

## 3. Architecture decisions still genuinely open

- [ ] **Storage migration strategy** for a future breaking `LatchSmartAccount` change —
      `UPGRADE_PATH.md`'s "Still open" section: not decided which of eager/lazy/enum-wrapper
      migration fits, since no breaking change exists yet to migrate. Fine to stay undecided until
      a real breaking change is proposed, but worth knowing this is unresolved going into mainnet,
      not forgotten.
- [ ] **Client-side upgrade-availability UX** (web extension / mobile / dApp) — nothing surfaces
      "a new account version is available" to a user today. Low urgency until the first real
      `upgrade()` actually ships, but should exist before it's needed in production.
- [ ] **No test proves `upgrade()` changes behavior**, only that the mechanism succeeds against
      the account's own current wasm — needs a second compiled fixture to upgrade *to*.
- [ ] **Guardian/account recovery — completely unresolved.** [Discussion #31](https://github.com/3K1-Labs/latch-contracts/discussions/31):
      open design questions on whether this is a `Policy` or needs a new account-level primitive.
      Given the mobile incident above is *exactly* the scenario recovery would address, this is
      worth prioritizing before mainnet, not treating as a nice-to-have.
- [ ] **Confidential token wallet support** — [Discussion #34](https://github.com/3K1-Labs/latch-contracts/discussions/34).
      Not a mainnet-*launch* blocker (it's a follow-on capability, not core wallet function), but
      flagging since it has its own external timeline risk (confidential tokens themselves were
      still in audit as of last check) independent of anything in this repo.

## 4. CI / test hardening

- [ ] **CI never verifies the actual WASM build succeeds** — only `cargo build`/`cargo test`
      (native target). We've already hit a real case where `cargo build --target wasm32v1-none`
      passed while `stellar contract build` would have failed
      (`experimental_spec_shaking_v2`). `CONTRIBUTING.md` makes `stellar contract build` a manual
      pre-PR step today — a human gate, not an automated one. Worth closing before mainnet, with
      the caveat already noted in `TODO.md`: check whether building all crates in one CI context
      hits OZ's own unresolved same-name-functions-across-contracts collision first.
- [ ] **No enforced test coverage threshold** (OZ uses `cargo llvm-cov --fail-under-lines 90`; we
      have nothing equivalent). Decide whether to adopt one before or shortly after mainnet.
- [ ] **`emit_*` event helper retrofit** for `session-policy` and `factory-contract` — style
      consistency, not a bug, low urgency.

## 5. Signer/verifier coverage — is Ed25519 + WebAuthn actually sufficient for launch?

Today's real, production-intended verifier lineup is just `ed25519-verifier` and
`webauthn-verifier`. That may be entirely fine for launch — but it should be a **stated decision**,
not a default nobody consciously made. If mainnet launch needs broader signer support on day one
(secp256r1 session keys, secp256k1, anything else from the earlier verifier brainstorm), that
needs its own timeline; if not, explicitly confirming "Ed25519 + WebAuthn is the v1 mainnet scope"
closes this out.

## 6. Cross-repo gaps found auditing this checklist's own premise

Not blockers for *this* repo's mainnet deploy, but worth tracking since they affect the system's
overall readiness:

- [ ] `latch-api` and `latch-relayer` are missing `LICENSE`, `CONTRIBUTING.md`, and (for `latch-api`)
      `SECURITY.md`/`CODE_OF_CONDUCT.md` entirely — see `OSS_READINESS_CHECKLIST.md`, which is still
      the reference implementation to work through for each.
- [ ] `latch-contracts-dapp`'s actual repository location is currently unverified — the expected
      `3K1-Labs/latch-contracts-dapp` doesn't resolve, and the local clone has no git remote
      configured. Track down where this actually lives if it's meant to ship alongside mainnet.
- [ ] `latch-web-extension` dead Phantom/Freighter code — [issue #28](https://github.com/3K1-Labs/latch-web-extension/issues/28).
      Hygiene, not a security issue (the code is unreachable behind a disabled flag), but worth
      clearing before a mainnet-facing audit so reviewers aren't spending time on dead paths.

---

## Not on this list on purpose

- Confidential tokens (tracked separately in Discussion #34 — a roadmap item, not a launch gate).
- New verifier kinds beyond what's already scoped (BLS, RSA, ZK-based signing, etc.) — deferred
  per `VERIFIER_USE_CASE_RESEARCH.md`'s own recommendation, revisit post-launch unless a concrete
  need appears.
