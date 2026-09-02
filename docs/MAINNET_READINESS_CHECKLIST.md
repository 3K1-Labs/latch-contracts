# Mainnet Readiness Checklist

What's actually still open before real user funds sit behind these contracts, pulled together
from `SECURITY.md`, `UPGRADE_PATH.md`, `TODO.md`, `BUILD.md`, open issues/discussions, and a
verification pass over the code — not a wishlist, a punch list. Update this as items close; don't
let it drift the way `TODO.md` almost did.

**Last full verification: 2026-09-02** (every claim below was checked against the code, CI config,
GitHub, and the sibling repos on that date). The first revision of this file was written at
commit `451ffcf`; the contract surface has roughly doubled since, so several original items were
stale and have been rewritten or moved to "Closed" at the bottom.

**One-line status:** code readiness has moved a lot, gate readiness has not. No contract is
deployed anywhere with a recorded artifact, the audit has not started and its scope is still
growing, and the signer-removal lockout (#77, verified real) is by recorded decision mitigated
client-side, not on-chain — and that client-side mitigation does not exist yet. The critical path,
in order: settle #77 per the #38 decision → freeze the v1 crate set → audit → deploy and record.

---

## 1. Security & Audit — the actual blocker

- [ ] **Settle [#77](https://github.com/3K1-Labs/latch-contracts/issues/77) in line with the
      recorded #38 decision.** The lockout is real — verified 2026-09-02 against both OZ 0.7.2 and
      `LatchSmartAccount` on `main`: OZ's `validate_signers_and_policies` only requires "at least
      one signer *or* one policy", OZ's own test `remove_signer_with_policy_present_success`
      asserts a rule may end up with zero signers, and on Latch's account removing the sole signer
      from a rule carrying `spending-limit-policy` succeeds, after which every authorization
      against that rule fails (`UnauthorizedSigner` for the old key, `NotAllowed` with no key).
      But the **decision already on record** (issue #38 / PR #53, 2026-08-27) is that on-chain
      enforcement of signer-change safety is *shelved*: overriding `SmartAccount`'s core trait
      methods is the most sensitive change in the system and repeatedly surfaced adjacent
      bypasses during review, so it stays parked on `experimental` until an external audit, and
      the near-term mitigation is a **client-side confirmation warning**, with the "only protects
      callers using Latch's own UI" limitation explicitly accepted. #77 (filed 2026-08-31, after
      that decision) asks for a new on-chain check in a `remove_signer_checked` that doesn't exist
      on `main`, and a contributor volunteered on 2026-09-01. Actions:
      - [x] Rescope #77 to match the decision — done 2026-09-02: status comment, retitled,
        relabelled `blocked`/`documentation`, `help wanted` removed, volunteer redirected.
      - [x] File the UI warning itself — done 2026-09-02 as
        [latch-mobile#69](https://github.com/3K1-Labs/latch-mobile/issues/69), cross-linked from
        [latch-mobile#68](https://github.com/3K1-Labs/latch-mobile/issues/68) (real on-chain
        removal, currently a local-state mock). `latch-web-extension` has no removal flow at all,
        so nothing to file there yet; its self-management UI issue (#42) inherits this when it
        starts. Net today: Latch's own clients can't remove signers on-chain, so exposure is
        limited to direct contract callers — but **latch-mobile#69 must ship before mainnet**;
        it is the accepted mitigation.
      - [ ] Decide whether the audit scope includes PR #53's approach, so on-chain enforcement can
        be reconsidered post-audit instead of re-litigated. Note `experimental` is 13 commits
        behind `main`; #53 would need a rebase before it's a usable reference again.
- [ ] **Freeze the audit / v1 deploy scope.** The lineup the original checklist described no
      longer exists. Since `451ffcf` the workspace gained `p256-verifier`, `secp256k1-verifier`,
      `parameter-scoped-policy`, `recipient-allowlist-policy`,
      `multi-token-spending-limit-policy`, `fee-forwarder`, the account's `deploy_contract`
      entrypoint, and two `templates/*` contracts — and four open PRs (#81 recurring-escrow,
      #72 inheritance-vault, #55 timelock-policy, #52 rate-limit-policy) would add more. An
      auditor needs a fixed target. Decide explicitly which crates are in the v1 mainnet deploy
      set, merge or defer the open PRs accordingly, and tag the commit the audit runs against.
- [ ] **External security audit of Latch's own code.** `SECURITY.md` says this plainly: *"This
      project has not yet undergone an external security audit... do not deploy it to hold real
      value without your own independent review."* Nothing else on this list matters if this box
      stays unchecked. Scope distinction: OpenZeppelin's `stellar-accounts`/`stellar-contract-utils`
      /`stellar-fee-abstraction`/`stellar-access` (pinned `=0.7.2`, the latest stable as of this
      revision; `0.8.0-rc.3` exists as a pre-release) went through OZ's own `v0.7.0` audit. The gap
      is **Latch's own code on top**: `latch-smart-account` (`upgrade()` and `deploy_contract`
      wiring), `factory-contract`, every `latch-verifiers/*` and `policies/*` crate, and
      `fee-forwarder`'s role gating. `SECURITY.md`'s Scope section now lists the full set.
- [ ] **Revisit the bug-bounty question.** Deferred in `TODO.md` "in favor of a plain email
      contact for now; reconsider closer to mainnet launch" — this is that moment.
- [ ] **Update `SECURITY.md`'s "Status" section** once the audit above actually happens — it's
      explicitly written to be revisited, not a permanent statement.
- [ ] **Cross-repo trust context, not this repo's bug, but relevant before real funds are at
      stake anywhere in the system**: the confirmed fund-loss incident on `latch-mobile` (logout
      destroying an unbacked-up wallet with no warning — private advisory `GHSA-gcg8-6536-rvc3`)
      is **still in `draft` state as of 2026-09-02**. The contracts can be flawless and users can
      still lose funds through a client bug. Confirm client-side security posture before calling
      the *system* mainnet-ready, not just this repo.

## 2. Deployment — nothing is deployed or recorded yet

`BUILD.md` was reset in #67 and `deployments/` is empty: **there is no recorded deployment of any
contract on any network.** Every item here is a first deploy under the artifact-archiving
discipline `deployments/README.md` describes, not a "redeploy" of something stale.

- [ ] **Deploy and record every singleton in the frozen v1 set**, with `BUILD.md` entries pointing
      at archived artifacts in `deployments/artifacts/`. The current full set is 4 verifiers
      (`ed25519`, `p256`, `secp256k1`, `webauthn`), 7 policies, `fee-forwarder`, the
      `latch-smart-account` wasm upload, and the factory — trimmed to whatever section 1's scope
      freeze decides.
- [ ] **Account Factory.** Code verified 2026-09-02: 4-arg constructor
      (`smart_account_wasm_hash`, `ed25519_verifier`, `webauthn_verifier`, `threshold_policy`),
      salt preimage `latch.factory.account.v2`, instance TTL 1,555,200 ledgers (~90 days). Deploy
      after its four inputs exist.
- [ ] **Ed25519 verifier — confirm the clients.** The Phantom-popup variant now lives in
      `demo/modified-ed25519-verifier` with a "do not deploy" module doc, and the README's
      deployment order uses the plain `ed25519-verifier`, so the *contract-side* decision is made.
      Still unverified from this repo: that every real client signer (web-extension passkey path,
      mobile passkey path, any raw-Ed25519 SDK path) works against the plain verifier's raw
      32-byte hash. Check before deploy, not after.
- [ ] **Deployer identity operational security.** The factory has deliberately no admin key
      (`UPGRADE_PATH.md` Decision 1), but *something* signs the mainnet deployment transactions.
      Confirm who holds that key and how it's secured — a compromised deployer during initial
      rollout is a real, if narrow, window even though the deployed factory has no ongoing
      privileged function.
- [ ] **`fee-forwarder` key custody — new since the original checklist.** Unlike everything else in
      the lineup, `fee-forwarder` has live privileged roles on mainnet: `admin`, `manager`, and
      the `executor` set (`latch-relayer`'s operating addresses), with `grant_role`/`revoke_role`
      available after deploy. Decide who holds `admin` and `manager` (hardware key? multisig?
      a Latch smart account?), document it, and make sure the relayer's executor key rotation
      story exists before it's needed.

## 3. Signer / verifier scope — decide what v1 actually supports

The original question ("is Ed25519 + WebAuthn sufficient?") is stale: the shipped lineup is now
**four** verifiers. But no decision has been recorded, and two facts make it non-trivial:

- [ ] **The factory only creates accounts with `Ed25519` or `WebAuthn` signers.** Its
      `SignerKind` enum has exactly those two variants, so a raw P-256 or secp256k1 key can't be
      part of account creation — only added afterward through the account's own signer
      management (`Signer::External(verifier, key_data)` is generic). Decide whether that's the
      intended v1 shape (P-256/secp256k1 as *add-on* signers only) or whether the factory needs
      the extra variants before launch. Either is defensible; leaving it implicit isn't.
- [ ] **Raw P-256 session keys have open work.** Initiative
      [#19](https://github.com/3K1-Labs/latch-contracts/issues/19) is open with
      [#22](https://github.com/3K1-Labs/latch-contracts/issues/22) (end-to-end session
      authorization coverage) and [#23](https://github.com/3K1-Labs/latch-contracts/issues/23)
      (lifecycle and threat-model docs) unresolved. If `p256-verifier` is in the v1 deploy set,
      both should close first; if it isn't, say so and defer.
- [ ] **secp256k1 wallet integration is unvalidated.** The README says as much: the verifier is
      implemented, but no real wallet flow has exercised it. Deploying an unused singleton is
      cheap, but it's still audit surface. Include it in v1 deliberately or leave it out
      deliberately.

## 4. Architecture decisions still genuinely open

- [ ] **Guardian/account recovery — completely unresolved.**
      [Discussion #31](https://github.com/3K1-Labs/latch-contracts/discussions/31) is open with
      zero comments. Open design questions on whether this is a `Policy` or needs a new
      account-level primitive. The mobile incident above *and* #77 are both exactly the scenario
      recovery would address — prioritize before mainnet, not treat as a nice-to-have.
- [ ] **Storage migration strategy** for a future breaking `LatchSmartAccount` change —
      `UPGRADE_PATH.md`'s "Still open" section: not decided which of eager/lazy/enum-wrapper
      migration fits, since no breaking change exists yet to migrate. Fine to stay undecided
      until a real breaking change is proposed, but worth knowing this is unresolved going into
      mainnet, not forgotten.
- [ ] **Client-side upgrade-availability UX** (web extension / mobile / dApp) — nothing surfaces
      "a new account version is available" to a user today. Low urgency until the first real
      `upgrade()` ships, but should exist before it's needed in production.
- [ ] **No test proves `upgrade()` changes behavior.** `latch-smart-account/src/test.rs` still
      only upgrades the account to its own current wasm — proves the mechanism and the self-auth
      gate, not a behavior change. Needs a second compiled fixture to upgrade *to*.
- [ ] **Confidential token wallet support** —
      [Discussion #34](https://github.com/3K1-Labs/latch-contracts/discussions/34), zero comments.
      Not a mainnet-*launch* blocker (a follow-on capability, not core wallet function), but it
      has its own external timeline risk (confidential tokens themselves were still in audit as
      of last check) independent of anything in this repo.

## 5. CI / test hardening

- [ ] **No enforced test coverage threshold** (OZ uses `cargo llvm-cov --fail-under-lines 90`; we
      have nothing equivalent). Decide whether to adopt one before or shortly after mainnet.
- [ ] **No dependency audit in CI** — no `cargo-deny` or `cargo-audit` config exists. Cheap to add;
      worth having before the audit so the auditor isn't the one who finds a yanked crate.
- [ ] **Resource-cost regression report** —
      [#70](https://github.com/3K1-Labs/latch-contracts/issues/70), open. Not a launch gate, but
      the kind of thing that's much easier to add before the first mainnet fee complaint.
- [ ] **`emit_*` event helper retrofit** for `session-policy` and `factory-contract` — the newer
      crates (`recipient-allowlist-policy`, `parameter-scoped-policy`, `vesting-schedule`) already
      use the helper pattern; the factory still publishes inline. Style consistency, not a bug,
      low urgency.

## 6. Cross-repo gaps

Not blockers for *this* repo's mainnet deploy, but they affect the system's overall readiness.
Verified against the GitHub remotes 2026-09-02:

- [ ] **`latch-api`** — none of `LICENSE`, `CONTRIBUTING.md`, `SECURITY.md`, or
      `CODE_OF_CONDUCT.md` exist on the remote. See `OSS_READINESS_CHECKLIST.md`.
- [ ] **`latch-relayer`** — has `SECURITY.md` only; missing `LICENSE`, `CONTRIBUTING.md`,
      `CODE_OF_CONDUCT.md`.
- [ ] **`latch-contracts-dapp`'s repository location is still unverified.**
      `3K1-Labs/latch-contracts-dapp` doesn't resolve, and the local clone has no git remote and a
      single "initial commit". (The local copy *does* have all four governance files.) Track down
      where this lives — or create it — if it ships alongside mainnet.

---

## Closed since the previous revision

Kept so nobody re-investigates them:

- **CI now verifies the WASM build** (2026-09-02). `rust.yml` gained a `wasm-build` job running
  `stellar contract build` over the whole workspace. The feared OZ same-name-function collision
  doesn't apply: stellar-cli builds each member as a separate `cargo rustc --manifest-path`
  invocation. Verified across all 19 crates before adding.
- **`SECURITY.md` Scope section** updated (2026-09-02) to list every current crate, including
  `fee-forwarder` and `templates/*`.
- **Ed25519 demo-vs-production split** — resolved in code (#33): the wrapped variant is in `demo/`
  and documented as not for deployment. Only the client-compatibility confirmation remains (§2).
- **"Secp256k1 — in or out?"** in its original form — the stub was deleted (#33) and then a real
  stateless verifier was added (#79). The live question is now the scope decision in §3.
- **"Everything on testnet is stale"** — superseded by the `BUILD.md` reset (#67). There is no
  testnet record to be stale anymore; §2 is a clean-slate deploy list.
- **`latch-web-extension` dead Phantom/Freighter code** —
  [issue #28](https://github.com/3K1-Labs/latch-web-extension/issues/28) is closed.

## Not on this list on purpose

- Confidential tokens (tracked in Discussion #34 — a roadmap item, not a launch gate).
- New verifier kinds beyond the four shipped (BLS, RSA, ZK-based signing, email) — deferred per
  `VERIFIER_USE_CASE_RESEARCH.md`'s own recommendation; revisit post-launch unless a concrete
  need appears.
- The open template/policy PRs (#81, #72, #55, #52) as individual items — they're covered by the
  scope-freeze decision in §1.
