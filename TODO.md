# TODO

Tracking known gaps and follow-ups.

## Dependency hygiene

- [ ] **Set up a recurring OZ upstream drift check.** We manually audited
      the pinned `stellar-accounts` version against upstream once this
      session (see memory: `oz_stellar_contracts_drift_check`). Consider
      turning that into a scheduled check (e.g. via the `schedule` skill)
      instead of relying on remembering to do it by hand every few weeks.
      Now that the pin is a real crates.io version (`=0.7.2`) instead of a
      git rev, this is simpler than before — just compare against the
      latest published version and its own audit status.
- [ ] **`ed25519-phantom-verifier`'s `assert!()`** (in `verify()`) should be
      `panic_with_error!` with a proper error variant instead — flagged as a
      discrepancy in `.claude/commands/code-quality.md`'s "Errors and
      panics" rule, not yet fixed.

## Code conventions (flagged, not yet applied)

- [ ] **Retrofit `session-policy` and `factory-contract` to use `emit_*`
      event helpers** instead of inline `.publish(e)` calls, to match the
      target convention documented in `.claude/commands/code-quality.md`
      (marked "target, not yet universal" there). Low urgency — purely a
      style/consistency cleanup, not a bug.
- [ ] **No test coverage tooling** (OZ uses `cargo llvm-cov --fail-under-lines
      90`; we have nothing equivalent). `code-quality.md` explicitly notes
      this rather than inventing a threshold that isn't enforced. Decide
      later whether it's worth adding.

## CI

- [ ] **CI never verifies the WASM build actually succeeds.** `rust.yml` only
      runs `cargo build`/`cargo test` (native/default target) — never
      `stellar contract build`. Tests don't need a compiled WASM artifact to
      run (`soroban-sdk`'s test utilities simulate the host in-process), so
      this passes today even when the actual deployable build would fail —
      we hit exactly this case firsthand (`experimental_spec_shaking_v2`
      failing under plain `cargo build --target wasm32v1-none`, only caught
      by manually running `stellar contract build`). `CONTRIBUTING.md`'s
      checklist makes `stellar contract build` a required manual step before
      opening a PR, so there's a human gate today, just not an automated one.
      **Before adding this to CI**, investigate whether it hits the same
      issue OpenZeppelin's own `stellar-contracts` CI has left unresolved —
      their `generic.yml` has a WASM build step commented out with:
      `"TODO: re-enable this after we find a stable solution to same name
      functions across multiple contracts."` We've only ever built one crate
      at a time (`--package <name>`); unclear whether building all 10 in one
      CI context would hit that same collision. Not urgent — revisit later.

## Governance docs

- [ ] Revisit `SECURITY.md`'s "Status" section once an audit happens.
- [ ] Revisit the bug-bounty question — deferred in favor of a plain email
      contact for now; reconsider closer to mainnet launch.
- [ ] `CODEOWNERS` was deliberately skipped (single maintainer today) —
      add once there's more than one person merging PRs.

## Production readiness (from UPGRADE_PATH.md)

- [x] Smart account's self-authorized `upgrade()` — implemented 2026-08-20,
      built on OZ's `Upgradeable` trait. See `UPGRADE_PATH.md`.
- [ ] Storage migration strategy for a future breaking account change — no
      such change exists yet to migrate, so not decided. See
      `UPGRADE_PATH.md`'s "Still open" section.
- [ ] Client-side (web extension / mobile / dApp) UX for surfacing "a new
      account version is available" and walking a user through
      authorizing the upgrade.
- [ ] A test proving an upgrade actually changes behavior, not just that
      the mechanism succeeds — needs a second compiled contract fixture
      to upgrade *to*. Low urgency: OZ's own test suite doesn't test this
      depth either, since the mechanism is a thin, audited host call, not
      custom logic of ours.
