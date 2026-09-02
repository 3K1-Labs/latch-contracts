# Audit Scope — Latch Contracts v1

Scope statement for the external security audit that gates mainnet. Decided 2026-09-02: **the
audit covers everything on `main` as it stands**, at a commit to be tagged `audit-v1` once
[PR #82](https://github.com/3K1-Labs/latch-contracts/pull/82) merges. Nothing else lands on
`main` between that tag and the audit report except fixes for audit findings.

## In scope

Every deployable crate in the Cargo workspace. Line counts are non-blank, non-comment Rust in
`src/` excluding test modules; test counts are `cargo test` results on 2026-09-02, all passing.

| Crate | Role | Src lines | Tests |
|---|---|---|---|
| `latch-smart-account` | The account. Thin over OZ `SmartAccount`; Latch-specific: self-authorized `upgrade()`, `deploy_contract`, `batch_add_signer` | 127 | 23 |
| `account-factory/contracts/factory-contract` | Deterministic account deployment, signer canonicalization, salt derivation (`latch.factory.account.v2`) | 296 | 24 |
| `latch-verifiers/ed25519-verifier` | Wrapper over OZ `ed25519` verifier | 21 | 8 |
| `latch-verifiers/p256-verifier` | Stateless raw P-256, own input validation | 45 | 19 |
| `latch-verifiers/secp256k1-verifier` | Stateless raw secp256k1, recover-and-compare | 58 | 25 |
| `latch-verifiers/webauthn-verifier` | WebAuthn assertion parsing over P-256 | 29 | 19 |
| `policies/threshold-policy` | Wrapper over OZ `simple_threshold` | 52 | 0 (see below) |
| `policies/weighted-threshold-policy` | Wrapper over OZ `weighted_threshold` | 68 | 0 (see below) |
| `policies/session-policy` | Latch-original: per-rule allowed-function list | 172 | 14 |
| `policies/spending-limit-policy` | Wrapper over OZ `spending_limit` | 50 | 0 (see below) |
| `policies/multi-token-spending-limit-policy` | Latch-original: oracle-denominated limit across tokens | 225 | 23 |
| `policies/parameter-scoped-policy` | Latch-original: Zodiac-Roles-style argument conditions | 304 | 28 |
| `policies/recipient-allowlist-policy` | Latch-original: recipient allowlist | 202 | 8 |
| `fee-forwarder` | Wrapper over OZ `fee-abstraction` + `access` roles; relayer-sponsored gasless calls | 65 | 15 |
| `templates/timelock-vault` | User-deployed personal contract | 80 | 14 |
| `templates/vesting-schedule` | User-deployed personal contract | 242 | 18 |

**Where to spend the time.** The Latch-original logic is concentrated in `factory-contract`,
`session-policy`, `parameter-scoped-policy`, `multi-token-spending-limit-policy`,
`recipient-allowlist-policy`, the two raw verifiers, and `vesting-schedule`. Everything else is a
deployable shell over audited OZ code, where the interesting questions are wiring and
authorization (who can call what), not algorithmic.

**Crates with no in-crate tests.** `threshold-policy`, `weighted-threshold-policy` and
`spending-limit-policy` are single-file wrappers with no test module. `spending-limit-policy`
is exercised end-to-end by `latch-smart-account`'s tests; the two threshold crates are not
exercised anywhere in the workspace. Flagging rather than hiding it.

## Explicitly out of scope

- `demo/modified-ed25519-verifier` — reference only, documented as never-to-be-deployed.
- `account-factory/contracts/dummy-account`, `dummy-singleton` — test fixtures.
- `references/` — gitignored third-party clones for reading, not built or shipped.
- The `experimental` branch, in particular [PR #53](https://github.com/3K1-Labs/latch-contracts/pull/53)
  (on-chain signer-change enforcement). Deliberately parked until after launch; see
  "Known and accepted risks" below.
- Open PRs [#81](https://github.com/3K1-Labs/latch-contracts/pull/81),
  [#72](https://github.com/3K1-Labs/latch-contracts/pull/72),
  [#55](https://github.com/3K1-Labs/latch-contracts/pull/55),
  [#52](https://github.com/3K1-Labs/latch-contracts/pull/52) — held until after the audit
  snapshot; they add new contracts, not changes to audited ones.
- Client code (`latch-web-extension`, `latch-mobile`, `latch-relayer`, `latch-api`).

## Dependencies

All OpenZeppelin crates are pinned to `=0.7.2` (`stellar-accounts`, `stellar-contract-utils`,
`stellar-fee-abstraction`, `stellar-access`, `stellar-macros`, `stellar-tokens`), the latest
stable release, covered by OZ's own `v0.7.0` audit. `soroban-sdk` is `26.1`. No git or path
dependencies. `Cargo.lock` is committed.

## Trust assumptions and privileged roles

- **Factory** has no admin, no upgrade, no pause. Config is set once in the constructor and is
  immutable. The only privileged moment is the deployment transaction itself.
- **Smart account** authorizes its own `upgrade()` and signer/policy management through its own
  `__check_auth`; there is no external admin. `deploy_contract` relies on the host's
  `CreateContract` auth context rather than an explicit self-`require_auth`.
- **Verifiers and policies** are stateless (verifiers) or keyed per `(smart_account, rule_id)`
  (policies); none has an admin.
- **`fee-forwarder`** is the one contract with live roles: `admin`, `manager`, and the
  `executor` set (the relayer's operating addresses). Role grant/revoke is available
  post-deploy. Key custody for `admin`/`manager` is an operational decision tracked in
  `MAINNET_READINESS_CHECKLIST.md`, not a code question.
- **Templates** are deployed by end users for themselves; Latch holds no role in them.

## Known and accepted risks (please confirm, don't rediscover)

1. **Signer-set divergence on policy-attached rules.** OZ's `remove_signer` only requires "at
   least one signer *or* one policy" per rule; it does not consult attached policies. So a
   removal can leave a threshold unreachable, or leave a rule with zero signers and a policy
   that rejects zero signers, permanently disabling that rule. OZ documents this as an
   administrator responsibility (`simple_threshold` module docs; `v0.7.0` audit trust
   assumptions for `weighted_threshold`). We reproduced both cases against 0.7.2 and against
   `LatchSmartAccount` (issues [#38](https://github.com/3K1-Labs/latch-contracts/issues/38),
   [#77](https://github.com/3K1-Labs/latch-contracts/issues/77)). **Decision:** on-chain
   enforcement exists on `experimental` (PR #53) but overriding `SmartAccount`'s core methods
   is deferred to after launch; v1 mitigates client-side
   ([latch-mobile#69](https://github.com/3K1-Labs/latch-mobile/issues/69)). We would value
   the auditor's view on whether OZ has since shipped or recommended a contract-level answer.
2. **Factory creates accounts with `Ed25519` and `WebAuthn` signers only.** `p256-verifier` and
   `secp256k1-verifier` are deployed singletons but can only be attached after creation via the
   account's own signer management. This is the intended v1 shape.
3. **`secp256k1-verifier` has no validated wallet integration.** Deployed for completeness;
   the verifier is in scope, the absence of a client flow is not a finding.
4. **No storage-migration strategy** for a future breaking account change. None exists yet to
   migrate; the `upgrade()` mechanism is OZ's `update_current_contract_wasm` wrapper.

## Build reproducibility

`stellar contract build` (stellar-cli ≥ 25.2.0; CI uses 27.1.0) at the workspace root builds all
crates. A plain `cargo build --target wasm32v1-none` fails by design (soroban-sdk
`experimental_spec_shaking_v2`). WASM hashes for the audited commit will be recorded in
`BUILD.md` with archived artifacts under `deployments/artifacts/` at deployment time.
