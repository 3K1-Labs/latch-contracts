# Almanax Pre-Audit Scan — Triage

**Reconciliation 2026-09-08:** the two code fixes (#3, #7/#8) and the four CI changes are
merged at `6b34ccc`. A fresh `cargo test --workspace`, `stellar contract build` and
`cargo clippy --workspace` all pass at that commit — see [TEST_EVIDENCE.md](TEST_EVIDENCE.md).
The `audit-v1` tag has been force-repointed to `6b34ccc` on the local repo and `origin`, as
the "on completion" step below intended. Still to attach to the audit submission itself: the
original Almanax scan export, and a post-fix rescan with its date, tool version and source SHA.

- **Scan:** `3K1-Labs/latch-contracts @ f2ee2db` (was `main` HEAD when the scan ran)
- **Resolution:** PR [#88](https://github.com/3K1-Labs/latch-contracts/pull/88); the
  `audit-v1` tag was force-repointed to that PR's merge commit `6b34ccc` on 2026-09-08
- **Run:** 2026-09-04, Almanax project `7191295a-…`, scan `b472be66-…`
- **Result:** 12 findings — 1 CRITICAL, 2 HIGH, 5 MEDIUM, 4 LOW
- **Purpose:** intake gate for the external audit. Every finding is dispositioned below:
  fixed (with commit), or dismissed (with written justification recorded in Almanax).

Each finding carries Almanax's auto-verdict, our independent verdict after reading the
code, and the resolution.

| # | Sev | Finding | Almanax | Our verdict | Resolution | Status |
|---|-----|---------|---------|-------------|------------|--------|
| 1 | CRIT | Constructor callable post-deploy (smart-account) | FP | **False positive** | Doc comment + dismiss | ✅ done |
| 2 | HIGH | Anyone can initialize factory if uninitialized | FP | **False positive** | Doc comment + dismiss | ✅ done |
| 3 | HIGH | Timelock-vault state TTL expires before unlock | TP | **True positive** | `bump_ttl()` + deposit refresh + docs | ✅ done |
| 4 | MED | Unpinned GitHub Actions — typos.yml | Monitor | **Valid (hardening)** | SHA-pinned all actions | ✅ done |
| 5 | MED | Unpinned GitHub Actions — rust.yml | TP | **Valid (hardening)** | SHA-pinned all actions | ✅ done |
| 6 | MED | No least-privilege GITHUB_TOKEN — rust.yml | Monitor | **Valid (hardening)** | `permissions: contents: read` | ✅ done |
| 7 | MED | Oracle price not validated (oracle.rs) | TP | **True positive** | `fetch_price` validates price/timestamp | ✅ done |
| 8 | MED | Oracle price sign not validated (lib.rs) | TP | **True positive — dup of #7** | Fixed by #7 | ✅ done |
| 9 | LOW | No least-privilege GITHUB_TOKEN — typos.yml | Monitor | **Valid (hardening)** | `permissions: contents: read` | ✅ done |
| 10 | LOW | Public getter extends storage TTL (parameter-scoped) | FP | **Informational** | Doc comment + dismiss | ✅ done |
| 11 | LOW | Fixed threshold can diverge from signer set | Monitor | **Known accepted risk** | Dismiss (Accepted Risk) | ✅ done |
| 12 | LOW | Public getter extends storage TTL (multi-token) | Monitor | **Informational — same as #10** | Doc comment + dismiss | ✅ done |

Net: **2 real code bugs** (#3, #7/#8), **4 cheap CI hardening items** (#4/#5/#6/#9),
**5 dismissals** (#1, #2, #10, #11, #12).

---

## #1 — CRITICAL — Constructor callable post-deploy enables account takeover — FALSE POSITIVE

`latch-smart-account/src/lib.rs:60`

The finding treats `__constructor` as an EVM-style `initialize()` — a normal function
anyone can call again. On Soroban that is wrong: `__constructor` is a name the host
reserves. It is executed exactly once, atomically, inside `create_contract` /
`deploy_v2`, and the host **rejects any direct post-deployment invocation** of a
function named `__constructor`. There is no "call it again" — not "no code path calls
it," but the runtime forbids the call.

The exploit chain fails at step 1 ("attacker calls `__constructor(...)` after
deployment"). This maps to `docs/AUDIT_SCOPE.md` §Trust assumptions: "Config is set
once in the constructor and is immutable."

**Dismissal note:** `False Positive — __constructor is a Soroban constructor,
host-invoked once during deploy_v2 and not callable afterward (host reserves the name).
The re-initialization path is not reachable. soroban-sdk 26.1.`

### Background — the bug class this pattern-matches

The **unprotected initializer** ("uninitialized proxy") bug. On EVM, a contract behind
an upgradeable proxy never runs its `constructor` in the proxy's storage context, so
setup moves into a regular `initialize()` function. If that function lacks an
"already initialized" guard and access control, anyone can call it and seize the
contract (the 2017 Parity multisig freeze — ~514k ETH locked — was exactly this).
Almanax sees a `pub` function that writes state with no `require_auth` and no
init-guard, and matches the fingerprint.

### Why it does not apply here

Soroban has real constructors (protocol 22 / soroban-sdk ≥ 22; this repo is on 26.1).
`__constructor` is a **host-reserved name**: the `create_contract_v2` host function
(what `deployer.deploy_v2(wasm_hash, args)` emits) instantiates the contract **and**
runs `__constructor(args)` in the same atomic operation, and the host does **not**
dispatch any later `InvokeContract` to `__constructor`. Exactly-once is enforced by the
runtime, so the requested `Initialized` flag would be unreachable code. The pattern that
*would* be dangerous on Soroban is a self-exposed `pub fn initialize()` called in a
second transaction — not `__constructor`.

### Reachability check (this repo)

- Factory creates accounts via `deployer.deploy_v2(config.smart_account_wasm_hash, (&signers, &policies))` — args passed at creation only.
- No workspace code invokes `__constructor` by name (impossible).
- `docs/AUDIT_SCOPE.md` §Trust assumptions already states account rules are set at construction with no external admin.

### Resolution

- **Code:** added a doc comment on `__constructor` in `latch-smart-account/src/lib.rs`
  explaining the host-once semantics (no functional change), so a human reviewer and any
  re-scan see the rationale inline. Commit: PR #88 (https://github.com/3K1-Labs/latch-contracts/pull/88).
- **Almanax:** dismiss as False Positive with the note above.
- **Status:** ✅ resolved — no functional change required.

## #2 — HIGH — Anyone can initialize factory if uninitialized — FALSE POSITIVE

`account-factory/contracts/factory-contract/src/lib.rs:82`

Same root cause as #1. Additionally the factory constructor already has an explicit
`if env.storage().instance().has(&DataKey::Config) { panic … AlreadyInitialized }`
guard (line 89) — the finding even quotes it. And the "deployed without running the
constructor" precondition is not reachable: `deploy_v2` / `env.register(Contract, (…))`
run the constructor in the same transaction as creation. `get_config()` fails closed
with `MissingConfig`, so an uninitialized factory can't deploy anything.

**Dismissal note:** `False Positive — Soroban constructor (host-invoked once);
additionally guarded by has(Config) -> AlreadyInitialized. No post-deploy call path.`

### Background

Same bug class as #1 (unprotected initializer), one level up: the factory `__constructor`
sets `FactoryConfig` (smart-account WASM hash + verifier / threshold-policy addresses).
If reachable, impact would genuinely be HIGH — a hostile config would make every later
`create_account` deploy attacker code at the expected deterministic address. Reachability
is the only question.

### Why it does not apply here

The finding needs two preconditions and both fail:

1. *"Deployed without invoking `__constructor`."* Not possible — when a contract's WASM
   declares a constructor with arguments (this one takes four), the host runs it as part
   of `create_contract_v2`; soroban-sdk's deploy/registration APIs have no
   instantiate-then-initialize-later path to race.
2. *"Call `__constructor` first / later."* Same as #1 — the host does not dispatch a
   post-creation invocation to `__constructor`.

The explicit `has(&DataKey::Config)` → `AlreadyInitialized` guard (line 89) only defends
a *second* init, not a hostile first one; it is defensive tidiness, not the load-bearing
control. `get_config()` fails closed with `MissingConfig`, so an uninitialized factory is
inert, not hijackable. Reasoned design, recorded in `docs/AUDIT_SCOPE.md` §Trust
assumptions ("Config is set once in the constructor and is immutable").

### On the recommendation

We are **not** adding `admin: Address` + `admin.require_auth()` to the constructor: a
Soroban constructor already runs under the deploying transaction's authority, so there is
no unauthenticated window; the extra auth entry would be non-idiomatic and add no real
security. OZ's own factory examples don't do it.

### Resolution

- **Code:** added a doc comment on the factory `__constructor`
  (`account-factory/contracts/factory-contract/src/lib.rs`) — no functional change.
  Commit: PR #88 (https://github.com/3K1-Labs/latch-contracts/pull/88).
- **Almanax:** dismiss as False Positive with the note above.
- **Status:** ✅ resolved — no functional change required.

## #3 — HIGH — Timelock-vault state TTL may expire before timelock ends — TRUE POSITIVE

`templates/timelock-vault/src/lib.rs`

Confirmed a real defect:

- `__constructor` (line 135) accepts any `unlock_ledger > current sequence` — **no upper
  bound**.
- It stores `Owner` + `UnlockLedger` in persistent storage and extends their TTL by a
  fixed `EXTEND_AMOUNT` ≈ 120 days (line 106).
- The only other TTL bump is in `withdraw` (lines 204-205), which cannot run until
  `sequence >= unlock_ledger`.
- `deposit`, `get_owner`, `get_unlock_ledger`, `get_balance` do **not** touch those keys,
  so they never refresh the TTL.

For a timelock longer than ~120 days (the module docs explicitly imagine "a long
timelock, e.g. 1 year"), `Owner`/`UnlockLedger` can reach end-of-TTL before the first
permitted withdrawal. When that happens the entries are **archived** (not deleted —
Soroban persistent entries are restorable via `RestoreFootprint`), but:

- `withdraw` does `…get(&DataKey::Owner).unwrap()` → the invocation traps on the
  archived entry; withdrawal is impossible until someone restores it.
- The vault exposes no method to help — no `keep_alive`, no `restore`. Recovery requires
  a client that knows the exact ledger keys and pays restoration rent out of band.
- Any third party can also `token::transfer` into the (known, deterministic) vault
  address before expiry, adding more stranded funds.

So "permanently bricked / funds lost forever" overstates it — restoration is
technically possible — but a user-facing savings template should not require
understanding state archival to retrieve funds. This is in audit scope
(`docs/AUDIT_SCOPE.md` lists `templates/timelock-vault`, 80 src lines).

### Background — Soroban storage TTL / archival

Persistent entries carry a per-entry TTL in ledgers. On expiry they are **archived**
(evicted from live state), not deleted, and can be brought back with a
`RestoreFootprint` operation. While archived, any contract call that touches the key
**traps** unless the same transaction restores it first — and restoration is a
transaction-level op against explicit ledger keys that wallets do not perform
automatically. `extend_ttl(threshold, extend_to)` pays rent forward: "if remaining TTL
< `threshold`, bump to `extend_to`."

### Resolution — FIXED

`templates/timelock-vault/src/lib.rs` + `test.rs`:

1. **`extend_state_ttl(e)` helper** — the two `extend_ttl` calls, deduped.
2. **New permissionless `bump_ttl()`** — extends `Owner` + `UnlockLedger`. No `Address`
   argument and no `require_auth`, so anyone (keeper, wallet, depositor) can pay to keep
   a long-lock vault alive; it moves no funds and changes nothing.
3. **`deposit()` now refreshes the TTL** — an actively-funded vault self-maintains.
4. **Docs** — a `# State lifetime` section in the module header + a constructor note
   spelling out that locks > ~120 days need periodic `bump_ttl()` (or deposit) calls,
   and that anyone can make them.
5. **Tests** — `bump_ttl_refreshes_state_before_unlock`, `deposit_refreshes_state_ttl`,
   `bump_ttl_is_permissionless` (assert via `testutils::storage::Persistent::get_ttl`).
6. **`testdata/timelock_vault.wasm` fixture regenerated** (`stellar contract build`) so
   it carries the new `bump_ttl` export. All 17 tests pass; clippy + fmt clean.

Deliberately **not** done: constructor-time TTL sizing from lock length, and a hard cap
on `unlock_ledger`. `bump_ttl()` covers every case without arithmetic that would itself
need review; the template's docs value "simpler and easier to audit." A generous sanity
cap can be added if the auditor prefers.

Commit: PR #88 (https://github.com/3K1-Labs/latch-contracts/pull/88).

## #4 / #5 — MEDIUM — Third-party GitHub Actions not pinned to commit SHAs — VALID (hardening)

`.github/workflows/typos.yml` (`actions/checkout@v4`, `crate-ci/typos@v1.24.5`)
`.github/workflows/rust.yml` (`actions/checkout@v4`, `dorny/paths-filter@v3`,
`actions-rust-lang/setup-rust-toolchain@v1`, `stellar/stellar-cli@v27.1.0`)

All referenced by mutable tags. A moved tag or a compromised upstream action would run
attacker code in CI. `rust.yml` also triggers on `push` to `main`/`experimental`.
Neither workflow references secrets or has write steps, so there is no demonstrated
takeover path today — this is standard supply-chain hardening, not an active vuln. A
smart-contract auditor will still flag it as informational, and the fix is cheap.

### Resolution — FIXED

Every `uses:` in both workflows is now pinned to a full commit SHA, with the version as
a trailing comment. SHAs resolved from the upstream repos via `git ls-remote` — each is
the commit the previously-floating tag pointed at, so behaviour is unchanged:

| Action | Was | Now |
|---|---|---|
| `actions/checkout` | `@v4` | `@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0` |
| `dorny/paths-filter` | `@v3` | `@0e4a8c6effa4802afeda77dc8d303f8176d7dfad # v3.0.4` |
| `actions-rust-lang/setup-rust-toolchain` | `@v1` | `@166cdcfd11aee3cb47222f9ddb555ce30ddb9659 # v1` |
| `stellar/stellar-cli` | `@v27.1.0` | `@8e402ea28202950b272fbabc34caad4d2f64fe87 # v27.1.0` + `version: "27.1.0"` |
| `crate-ci/typos` | `@v1.24.5` | `@945d407a5fc9097f020969446a16f581612ab4df # v1.24.5` |

`stellar/stellar-cli` picks the CLI binary to download from its own git ref, so a SHA
pin alone makes it request a release named after the SHA (404). The documented
`version:` input is passed alongside the pin to fix this; SHA, `# vX` comment and
`version:` are kept in lockstep.

Commit: PR [#88](https://github.com/3K1-Labs/latch-contracts/pull/88).

## #6 / #9 — MEDIUM / LOW — Workflows run without an explicit least-privilege token — VALID (hardening)

`.github/workflows/rust.yml`, `.github/workflows/typos.yml`

Neither workflow declared a `permissions:` block, so `GITHUB_TOKEN` fell back to the
repo/org default. On standard fork PRs GitHub already issues a read-only token, so
impact is conditional on repo settings — hence "monitor." Closed anyway.

### Resolution — FIXED

Added `permissions: { contents: read }` at the workflow level of both files. Every job
in both only checks out and runs cargo / the typo checker / the WASM build — nothing
pushes, comments, or releases — so no job needs an opt-back-in. YAML validated. Same
change as #4/#5.

Commit: PR #88 (https://github.com/3K1-Labs/latch-contracts/pull/88).

## #7 — MEDIUM — Unvalidated oracle price allows spending-limit bypass — TRUE POSITIVE

`policies/multi-token-spending-limit-policy/src/oracle.rs:64`

`fetch_price` returns `PriceData` after rejecting only `None`. No check that
`price > 0`, no check that `timestamp <= now`. Downstream in `lib.rs::enforce`:

- `current_timestamp.saturating_sub(price_data.timestamp)` — a **future** timestamp
  saturates to `0`, so the staleness gate (line 341) passes a future-dated price as
  fresh.
- `amount_usd = amount.saturating_mul(price_data.price).saturating_div(usd_divisor)`
  (line 345) — a **negative** price makes `amount_usd` negative, and
  `cached_total_spent_usd.saturating_add(amount_usd)` (line 353) then *lowers* the
  running total → transfers that should exceed the cap pass, and future spend headroom
  grows. A **zero** price makes every transfer cost `0` against the limit.

The policy's own module docs promise it "fails closed" against "a
misconfigured or malicious" oracle, and the oracle address is fixed at install with no
sanity check. Reflector (the intended oracle) won't return negative prices in practice,
which is why severity is MEDIUM not HIGH — but the stated threat model covers a bad
oracle, so this is a legitimate gap.

### Resolution — FIXED

`policies/multi-token-spending-limit-policy/src/oracle.rs` — `fetch_price` now validates
the decoded `PriceData` at the single choke point every price flows through:

```rust
if price_data.price <= 0 || price_data.timestamp > e.ledger().timestamp() {
    panic_with_error!(e, Error::InvalidOracleResponse)
}
```

Its doc comment now states the guarantee (`price > 0`, `timestamp <= now`), the
`Error::InvalidOracleResponse` variant doc and the module-level "Oracle trust model"
section were updated to match. Tests added: `test_enforce_rejects_negative_oracle_price`,
`test_enforce_rejects_zero_oracle_price`, `test_enforce_rejects_future_dated_oracle_price`
(all expect `#10`). 26 tests pass; clippy + fmt clean.

Commit: PR #88 (https://github.com/3K1-Labs/latch-contracts/pull/88).

## #8 — MEDIUM — Oracle price sign not validated (enforce call site) — TRUE POSITIVE, DUPLICATE OF #7

`policies/multi-token-spending-limit-policy/src/lib.rs:331-346`

Same defect as #7, reported at the `enforce` call site instead of the `fetch_price`
helper. **Fixed by the #7 change** — validation lives in `fetch_price`, which `enforce`
calls. In Almanax, resolve alongside #7 (or dismiss as a duplicate referencing #7).

## #10 / #12 — LOW — Public getter extends storage TTL — INFORMATIONAL

`policies/parameter-scoped-policy/src/lib.rs:48` → `conditions.rs:148` (`extend_ttl` on
read hit, no auth)
`policies/multi-token-spending-limit-policy/src/lib.rs:377` → internal
`get_policy_data:158` (same)

Anyone who knows a valid `(smart_account, context_rule_id)` pair can pay to keep an
already-installed policy entry alive by calling the getter repeatedly. It does **not**
create state, mutate policy contents, bypass `enforce`, or expose privileged data — the
same refresh already happens inside authenticated `enforce`. The identical
refresh-on-read pattern is used deliberately in `session-policy/src/allowlist.rs:128`
and `recipient-allowlist-policy/src/lib.rs:334`. The only "impact" is that you cannot
force someone's policy entry to expire by waiting them out — arguably desirable.

Almanax rated #10 FP and #12 Monitor; they are the same thing. Treat both as
informational.

**Dismissal note:** `Informational — TTL refresh on read is a deliberate, workspace-wide
pattern (see session-policy, recipient-allowlist-policy). No state creation, mutation,
or enforcement bypass; caller pays fees. Cannot be used to force-expire another
account's entry.` (If we want it gone: split into a non-extending public read + an
internal extending helper.)

### Resolution — DISMISS (informational)

No behavioural change. Added a doc comment at each flagged site explaining the
intentional refresh-on-read (`policies/multi-token-spending-limit-policy/src/lib.rs`
`get_policy_data` helper; `policies/parameter-scoped-policy/src/conditions.rs`
`get_conditions`) so a re-scan does not re-raise it. Dismiss both in Almanax with the
note above. Commit: PR #88 (https://github.com/3K1-Labs/latch-contracts/pull/88).

## #11 — LOW — Fixed threshold can diverge from signer set — KNOWN ACCEPTED RISK

`policies/threshold-policy/src/lib.rs` (thin wrapper over OZ `simple_threshold`)

Real behaviour: the threshold is validated against signer count only at install; later
`add_signer` / `remove_signer` don't update it, so removals can make a rule
permanently un-authorizable (DoS) and additions can silently weaken N-of-N to
N-of-(N+M).

This is **already documented and accepted**:

- `docs/AUDIT_SCOPE.md` §"Known and accepted risks" #1 — "Signer-set divergence on
  policy-attached rules," with reproduction, issues
  [#38](https://github.com/3K1-Labs/latch-contracts/issues/38) /
  [#77](https://github.com/3K1-Labs/latch-contracts/issues/77), on-chain enforcement
  parked on `experimental`
  ([PR #53](https://github.com/3K1-Labs/latch-contracts/pull/53)), v1 mitigated
  client-side ([latch-mobile#69](https://github.com/3K1-Labs/latch-mobile/issues/69)).
- The contract's own module docs carry a "Security Warning: Signer Set Divergence"
  section.

**Dismissal (Accepted Risk):** `Accepted Risk — documented in docs/AUDIT_SCOPE.md
§Known and accepted risks #1. On-chain enforcement deliberately deferred past v1 launch
(PR #53, experimental); v1 mitigation is client-side. Auditor asked to confirm, not
rediscover.`

### Resolution — DISMISS (Accepted Risk)

No code change. The behaviour is already called out in the contract's own module docs
("Security Warning: Signer Set Divergence") and accepted in `docs/AUDIT_SCOPE.md` §Known
and accepted risks #1. Dismiss in Almanax as **Accepted Risk** with the note above.

---

## Outcome

All 12 findings resolved via PR [#88](https://github.com/3K1-Labs/latch-contracts/pull/88)
before the audit (25/25 CI checks green, including a fresh Almanax scan of the branch):

| Disposition | Findings | Action |
|---|---|---|
| Fixed (code) | #3, #7, #8 | `bump_ttl()` keep-alive; `fetch_price` price/timestamp validation |
| Fixed (CI hardening) | #4, #5, #6, #9 | SHA-pinned all GitHub Actions; `permissions: { contents: read }` |
| Dismissed — false positive | #1, #2 | Soroban constructor semantics; doc comments added |
| Dismissed — informational | #10, #12 | Intentional TTL-on-read; doc comments added |
| Dismissed — accepted risk | #11 | Pre-existing, `docs/AUDIT_SCOPE.md` §Known risks #1 |

**Verification:** full workspace `cargo test` (all suites green), `cargo fmt --all --check`
clean, `cargo clippy --workspace --all-targets --all-features -D warnings` clean,
`stellar contract build` green for every crate. `timelock_vault.wasm` test fixture
regenerated.

## Almanax actions still to do (in the web UI)

1. **Dismiss** #1, #2 (False Positive), #10, #12 (False Positive), #11 (Accepted Risk) —
   paste the per-finding dismissal notes above.
2. **Resolve** #3, #4, #5, #6, #7, #8, #9 once the commit lands — reference the commit
   SHA. #8 resolves together with #7.
3. Keep this file as the audit-team hand-off: it records that the scan was run, every
   finding was reviewed against the code, and what was done.

## Tag

After PR #88 merges to `main`, the annotated `audit-v1` tag is force-repointed from
`ace9dbd` to the merge commit and force-pushed to `origin`:

```
git checkout main && git pull
git tag -f -a audit-v1 -m "Latch Contracts v1 audit baseline (post-Almanax pre-scan triage)"
git push --force origin audit-v1
```

`docs/AUDIT_SCOPE.md` is updated (2026-09-04 note + crate-table note). The "nothing else
lands on `main`" rule continues from the new tag.
