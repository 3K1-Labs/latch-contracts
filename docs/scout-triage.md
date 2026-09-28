# Scout Audit — Pre-Audit Run

Companion to [`almanax-triage.md`](almanax-triage.md). The audit team asked for a
[Scout](https://github.com/CoinFabrik/scout-audit) run alongside Almanax.

## Status: BLOCKED — Scout 0.3.16 cannot analyze a `soroban-sdk` 26.1 workspace

**Scout `cargo-scout-audit` 0.3.16** (2026-02-13, the latest published version — nothing
newer on crates.io) does not run against this codebase. Every attempt fails during
Scout's own `cargo check` step, and **Scout reports "Analyzed / 0 findings" for every
crate anyway** — the failure is only visible in the run log, plus a one-line
"This report is incomplete because some crates failed to compile" notice. A naive run
produces a false all-clear.

### What was tried

| # | Command | Outcome |
|---|---------|---------|
| 1 | `cargo scout-audit` | dylint driver build hit transient crates.io timeouts (`curl-sys`, `libz-sys`); run aborted, reported 0 findings |
| 2 | `cargo scout-audit` (retries bumped) | driver built OK; `cargo check` at **`wasm32-unknown-unknown`** → `soroban-sdk 26.1` `build.rs` panics: *"target 'wasm32-unknown-unknown' is unsupported … use 'wasm32v1-none'"* |
| 3 | `cargo scout-audit -- --target wasm32v1-none` | got past the panic; then `experimental_spec_shaking_v2` feature error: *"requires building with `stellar contract build` from stellar-cli v25.2.0+"* (feature pulled in transitively by `stellar-accounts` 0.7.2) |
| 4 | `SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1 cargo scout-audit -- --target wasm32v1-none` | spec-shaking gate cleared; **`wasm32-unknown-unknown` panic returns** — Scout/dylint still runs a check pass at that target and there is no user-facing flag to stop it. `soroban-sdk-macros` then fails to compile. |

Run logs: `scout-run{1..4}.log` (kept out of the repo).

### Root cause

Two independent, hard incompatibilities:

1. **Target.** Scout 0.3.16 drives Soroban analysis at `wasm32-unknown-unknown`.
   `soroban-sdk` ≥ 26 (on rustc ≥ 1.82) *unconditionally panics* in its build script for
   that target — it requires `wasm32v1-none`. Forcing `--target wasm32v1-none` through
   Scout's passthrough args is not honored for every check pass.
2. **Build system.** `stellar-accounts` 0.7.2 enables `soroban-sdk`'s
   `experimental_spec_shaking_v2`, which refuses a plain `cargo check` and requires
   `stellar contract build`. Scout runs plain `cargo check`.

Scout 0.3.16 predates soroban-sdk 26's target/build changes and has not been updated
(last commit / release 2026-02-13).

### Options (decision needed)

1. **Ask the audit team how they run Scout on soroban-sdk 26+ projects.** They may have
   a working Docker image, an internal build, or a pinned older toolchain. This is the
   preferred path — it's their required tool.
2. **Document the incompatibility as a known tool limitation** (this file + the logs) and
   rely on Almanax + the manual review + the auditors' own tooling for that coverage.
3. **Throwaway analysis branch** — patch `[patch.crates-io]` `soroban-sdk` to strip the
   `wasm32-unknown-unknown` panic and disable `experimental_spec_shaking_v2`, run Scout
   against that, discard the branch. Downside: Scout then analyzes a modified SDK and the
   result needs a caveat; effort is non-trivial and may surface further breakage.

### If Scout does run later

Its 36 Soroban detectors overlap Almanax on: `unsafe-unwrap`/`unsafe-expect` (the
`timelock-vault` `.unwrap()`s — expected noise, provably-safe reads),
`ineffective-extend-ttl` (cross-check on the #3 fix and the #10/#12 getters),
`unprotected-update-current-contract-wasm` (`LatchSmartAccount::upgrade`),
`divide-before-multiply` / `incorrect-exponentiation` (multi-token USD math),
`front-running` (the oracle read), `soroban-version`. Triage findings into this file in
the same format as the Almanax doc.
