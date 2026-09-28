# Audit Readiness Test Evidence

All runs below are at the `audit-v1` commit — the `main` merge of PR #88 (Almanax triage),
PR #89 (this packet) and PR #90 (threat-model `Tamper.2` fix) — on a local machine
(stellar-cli 27.1.0, `rustc`/`cargo` 1.94.1). Raw logs are kept under `docs/audit-evidence/`
and attached to the audit submission package; they are not committed to the repo (SHA-256
recorded here so an attached copy can be checked).

## Test run

| Field | Value |
|---|---|
| Date | 2026-09-09 |
| Command | `cargo test --workspace --locked` |
| Exit status | 0 |
| Result | 261 passed; 0 failed; 0 ignored |
| Production-scope subset | 251 passed; 10 additional tests are the excluded demo verifier |
| Environment | Local native Soroban test harness, not a network run |
| Raw log | `docs/audit-evidence/workspace-tests-2026-09-09.log` (submission attachment) |
| Log SHA-256 | `de35c50cfbc442cd5a4057067a3d4b7b97e6cd5f890def0e4b6fde0b75b62344` |

## Build and lint run

| Field | Value |
|---|---|
| Date | 2026-09-09 |
| Commands | `stellar contract build` · `cargo clippy --workspace --all-targets --locked` |
| Exit status | 0 · 0 |
| Build result | 21 WASM modules built; the 16 in-scope contract hashes are reconciled against the recorded testnet deployment in [BUILD.md](BUILD.md) (12 byte-identical, 4 changed) |
| Clippy result | clean — no warnings across all 19 workspace crates |
| Raw log | `docs/audit-evidence/build-clippy-2026-09-09.log` (submission attachment) |
| Log SHA-256 | `c47506b014d80cb897cbd5080c52c30a1a22cac1b7848593e9f523c82bd02811` |

The `multi-token-spending-limit-policy` source changed in PR #90 (the `Tamper.2` fix); its
7 new tests are in the 261 above and its WASM hash at `audit-v1` is
`66c3f9af3f1323a3ee788612f2508d11644a3cce3637dd8f865508a8886cec15` (see [BUILD.md](BUILD.md)).
All other crates are unchanged from PR #88.

## Test suite breakdown

| Suite | Passed |
|---|---:|
| Smart account | 23 |
| Factory | 24 |
| Ed25519 / P-256 / secp256k1 / WebAuthn | 8 / 19 / 25 / 19 |
| Session / parameter-scoped / recipient allowlist | 14 / 28 / 8 |
| Multi-token spending limit | 33 |
| Fee forwarder | 15 |
| Timelock vault / vesting schedule | 17 / 18 |
| Threshold / weighted threshold / spending limit wrappers | 0 / 0 / 0 |
| Dummy account / dummy singleton | 0 / 0 |
| Modified Ed25519 demo (excluded) | 10 |

## Integration coverage and its limits

| Flow | Test evidence | What it establishes / limitations |
|---|---|---|
| Deterministic factory deployment | [factory tests](../account-factory/contracts/factory-contract/src/test.rs): `create_account_deploys_at_precomputed_address`, `create_account_is_idempotent`, mixed-signer cases | Exercises deployment using dummy account/singleton WASM. Does not establish real account/verifier/threshold behavior after factory creation. |
| Account with session and spending policies | [account tests](../latch-smart-account/src/test.rs): `session_signer_allowed_call_succeeds`, disallowed/expiry/removal cases and `session_plus_spending_limit_*` | Tests account-policy composition and auth-check cases. Setup uses mocks; not a real wallet-signed submitted transaction. |
| Account satellite creation | Same account suite: `deploy_contract_deploys_real_wasm_and_derives_deterministic_address`, `create_contract_rule_*` | Separates mocked-auth deployment mechanics from auth-context checking; does not prove a complete host-generated, cryptographically signed creation path. |
| Sponsored calls | [forwarder tests](../fee-forwarder/src/test.rs): `forward_collects_fee_and_invokes_target`, `forward_reverts_fee_collection_when_target_call_fails`, nested-auth and missing-auth cases | Exercises fee/target composition, role gating and rollback in the harness. Does not test the deployed relayer with real account signatures. |
| Account-deployed vesting contract | [vesting tests](../templates/vesting-schedule/src/test.rs): `test_end_to_end_deployment_and_claim_via_smart_account` | Deploys fixture WASM through an account and checks phased claims with a mock token. Uses `mock_all_auths`; is not proof of signature enforcement. |
| Oracle-based policy | [multi-token tests](../policies/multi-token-spending-limit-policy/src/test.rs) | Calls a mock oracle for normal and malformed price responses; covers per-token `decimals()` normalization, mixed-decimal windows, dust truncation and the conversion-overflow path (`Tamper.2`). Uses a mock oracle and mock tokens — does not establish real feed accuracy or oracle-manipulation resistance. |

The threshold and weighted-threshold wrappers have no dedicated test suites and no demonstrated
real-wrapper integration here; factory tests use fixtures. The spending-limit wrapper has no
in-crate tests but account composition tests exercise spending-limit behavior. No count in this
document is labeled “number of integration tests”: the total includes unit and cross-contract
tests together.

## Reproduction and additional evidence

From the workspace root at the source revision above, run:

```sh
cargo test --workspace --locked
```

The lockfile and checked-in test WASM are inputs to this run. Tests that load those binaries
do not by themselves prove they match current Rust source; build/reconcile fixture provenance
when source changes. CI configuration is in [rust.yml](../.github/workflows/rust.yml).

Run at this commit and recorded above: `cargo test`, `stellar contract build`, `cargo clippy`.
**Not** run in this pass, and still required for the submission: a fresh security scan
(Almanax rescan — see [almanax-triage.md](almanax-triage.md); Scout is blocked, see
[scout-triage.md](scout-triage.md)) and any on-network transaction, including the fresh
testnet deploy of the 4 changed contracts. Also record the CI run URL for the final
submission revision. Open test-coverage work is mapped to threats in [THREAT_MODEL.md](THREAT_MODEL.md).
