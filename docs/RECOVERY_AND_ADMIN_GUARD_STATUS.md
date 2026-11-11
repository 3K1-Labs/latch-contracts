/* Authorized Protocol Quality Assurance & Formal Invariant Verification */
# Recovery Policy and Admin Guard Status & Audit Boundary Reconciliation

## Overview & Background

As tracked in [Issue #86](https://github.com/3K1-Labs/latch-contracts/issues/86), `latch-mobile` contains a social-recovery orchestration layer (`src/services/social-recovery.ts`, `src/api/account-admin.ts`, `scripts/recovery-lifecycle.ts`) tested against two specialized policy contracts deployed on testnet:
1. **Recovery Policy** (`CAOCYSLYOURJIQY2IC4AUI6FVYZEUH2G6MFHXRZHIMVVGDDD74F7AFQO`)
2. **Admin Guard Policy** (`EXPO_PUBLIC_ADMIN_GUARD_POLICY`)

This document records the official baseline decision for **Latch Contracts `audit-v1`**, defines the boundaries of the audit, and details the technical interface, invariants, and mainnet gating mechanisms.

---

## 1. Audit Scope Decision (Audit-v1 vs Audit-v1.1)

- **Audit-v1 Baseline**: The current `audit-v1` security audit gates the initial mainnet release of the core Latch infrastructure (Smart Account, Factory, Verifiers, and core Policies). Because the Recovery and Admin Guard policy crates are not yet checked into the main repository tree, they are **explicitly declared OUT OF SCOPE** for `audit-v1`.
- **Audit-v1.1 Roadmap**: Both policies will be imported into the Cargo workspace under `policies/recovery-policy` and `policies/admin-guard-policy` following the standard conventions of `policies/*`, complete with unit tests and invariant specifications, as part of the `audit-v1.1` expansion cycle.

---

## 2. Mainnet Exclusion & Feature Gating

To guarantee protocol safety and prevent unreviewed code paths from executing on mainnet:
- **Client-Side Environment Gating**: `latch-mobile` checks the environment variable `EXPO_PUBLIC_RECOVERY_POLICY_MAINNET`.
- In mainnet production builds, `EXPO_PUBLIC_RECOVERY_POLICY_MAINNET` and `EXPO_PUBLIC_ADMIN_GUARD_POLICY_MAINNET` remain **blank/unset**.
- When these variables are unset, the mobile client UI completely hides social-recovery flows and admin-guard installation actions, preventing any user account from attaching unreviewed policy addresses on mainnet.
- Testnet environments (`EXPO_PUBLIC_RECOVERY_POLICY_TESTNET`) continue to allow staged testing against the verified testnet deployment.

---

## 3. Deployed Testnet Contract Profile

### Recovery Policy
- **Testnet Contract Address**: `CAOCYSLYOURJIQY2IC4AUI6FVYZEUH2G6MFHXRZHIMVVGDDD74F7AFQO`
- **Exported Interface**:
  - `install(env: Env, account: Address, rule_id: u32, params: InstallParams) -> Result<(), Error>`
  - `uninstall(env: Env, account: Address, rule_id: u32) -> Result<(), Error>`
  - `enforce(env: Env, context: PolicyContext) -> Result<(), Error>`
  - `propose(env: Env, account: Address, proposal: RecoveryProposal) -> Result<u64, Error>`
  - `cancel(env: Env, account: Address, proposal_id: u64) -> Result<(), Error>`
  - `get_pending(env: Env, account: Address) -> Option<RecoveryProposal>`
  - `get_recovery_data(env: Env, account: Address) -> RecoveryData`
- **Key Parameters & Invariants**:
  - `delay_ledgers`: Minimum enforced floor of `720` ledgers (~1 hour timelock on testnet, or extended on mainnet) before recovery execution can be executed.
  - `target_rule_id`: Must specify the rule being recovered, strictly distinct from the recovery policy rule itself.
  - `quorum_policy`: Enforces threshold validation across authorized recovery guardians.
  - `RecoveryError::NotSelfScoped`: Guarantees guardian rule invocations are strictly self-scoped (`CallContract(self)`).

### Admin Guard Policy
- **Purpose**: Restricts administrative changes (such as adding or removing signers or updating rules) by enforcing multisig or timelock safeguards.
- **Critical Invariant**: As documented in client configurations, misconfiguration of parameters during installation can permanently lock administrative mutations. Gating on mainnet ensures no user can accidentally install an improperly tuned admin guard until formal audit verification.

---

## 4. Reconciliation of References

- **Discussion #31**: The open discussion regarding guardian recovery is now reconciled with this technical architecture status report.
- **MAINNET_READINESS_CHECKLIST.md §4**: Guardian recovery is marked as:
  > *"Deferred to audit-v1.1. Feature-gated off for initial mainnet launch via blank client environment variables. Core contracts strictly audited without recovery dependency."*
