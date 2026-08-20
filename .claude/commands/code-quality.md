---
name: code-quality
description: >
  Review and (optionally) fix Rust source files against the Latch Contracts
  Code Quality Checklist. Use this skill whenever the user wants to audit,
  review, or improve code quality of a contract or policy crate in this repo,
  or mentions "/code-quality". Triggers on phrases like "check code quality",
  "review latch style", "review this contract".
user_invocable: true
---

# Latch Contracts — Code Quality Checklist

Reviews `.rs` files in this repo for convention violations and either reports
them or fixes them in place — the user picks.

This is a local maintainer tool. It is not wired into CI.

This checklist is adapted from
[OpenZeppelin's stellar-contracts code-quality checklist](https://github.com/OpenZeppelin/stellar-contracts/blob/main/.claude/commands/code-quality.md),
since this project is built on top of their `stellar-accounts` library. Where
this repo's own conventions differ from OZ's — because our crates are smaller,
single-purpose contracts rather than a large multi-extension library — that's
called out explicitly below rather than silently inherited.

## Repository shape

Like OZ's `stellar-contracts`, this repo is **one Cargo workspace** — the root
`Cargo.toml` lists every crate below as a member, and `[workspace.dependencies]`
pins `soroban-sdk` and `stellar-accounts` (from crates.io, not a git rev) once
for all of them to share:

```
latch-smart-account/                # the account contract itself
account-factory/contracts/
  factory-contract/                 # the factory
  dummy-account/                    # test-only stub, no tests of its own
  dummy-singleton/                  # test-only stub, no tests of its own
latch-verifiers/
  ed25519-phantom-verifier/
  secp256k1-verifier/               # stub, not yet implemented
  webauthn-verifier/
policies/
  threshold-policy/                 # thin wrapper around OZ's simple_threshold
  session-policy/                   # method-allowlist policy (own logic)
  spending-limit-policy/            # thin wrapper around OZ's spending_limit
```

One shared `Cargo.lock`, one `stellar-accounts` version for everything —
bumping it is a single-line change instead of nine. `cargo build`/`test`/
`clippy` still scope to one crate when run from inside its directory (or with
`--package <name>`); only `cargo fmt --all` always covers the whole workspace
regardless of cwd. See the memory entry on auditing the OZ pin
(`oz_stellar_contracts_drift_check`) for how to check it against upstream.

## Usage

- `/code-quality` — review the file(s) changed on the current branch
  (`git diff main...HEAD --name-only`, plus any uncommitted edits).
- `/code-quality <path>` — review a specific file or directory.
- `/code-quality <crate-name>` — review one crate by its top-level directory
  name (e.g. `policies/session-policy`, `latch-verifiers/webauthn-verifier`).

## Workflow

### 1. Check working tree

```bash
git rev-parse --abbrev-ref HEAD
git status --short
```

If the working tree is dirty, warn the user and ask whether to continue,
stash first, or abort. Do not silently mix unrelated changes. If the current
branch is `main`, do not commit there — require a new branch.

### 2. Discover the file set

If a path or crate name was provided, expand it: glob `**/*.rs` under it,
excluding `target/`. If no argument was given, derive the file set from git
(committed + uncommitted changes since `main`), filtered to `*.rs`.

Read every file in scope before checking rules — partial reads produce
partial reviews.

### 3. Identify violations & discrepancies

Walk the file set against **two** reference points:

1. **The rules in the `Rules` section below.**
2. **The closest existing sibling crate.** A "thin wrapper" policy
   (`threshold-policy`, `spending-limit-policy`) should look like its
   sibling wrapper, not like `session-policy`'s own-logic style, and vice
   versa. A verifier should look like the other verifiers.

Build a numbered list of findings: file path, line number, kind
(`rule:<name>` or `discrepancy:<sibling-path>`), what differs, and a one- or
two-sentence proposed fix.

### 4. Choose an action

Ask the user: **apply all**, **one-by-one** (describe each, get approval,
edit), or **report only** (list findings, change nothing). The user may
cancel entirely. If the list is empty, say so and stop.

### 5. Apply fixes

Use the `Edit` tool. After all edits, run from the repo root (fmt covers the
whole workspace regardless of cwd) then from the crate's own directory:

```bash
# Format (NIGHTLY required — rustfmt.toml uses unstable_features)
cargo +nightly fmt --all

cd <crate-dir>

# Lint, warnings as errors
cargo clippy --all-targets --all-features -- -D warnings
```

Both must succeed. Do not paper over a clippy warning with `#[allow(...)]` —
that's itself a violation (see "Lint suppression"). If a warning genuinely
can't be fixed, stop and escalate to the user.

### 6. Build, test, doc

```bash
cargo test
stellar contract build   # NOT `cargo build --target wasm32v1-none` — that
                          # fails: soroban-sdk's experimental_spec_shaking_v2
                          # feature requires going through the Stellar CLI
cargo doc --no-deps
```

There is no coverage tooling configured in this repo yet (unlike OZ's
`cargo llvm-cov --fail-under-lines 90`) — don't invent a threshold that isn't
actually enforced.

### 7. Report

Summarize what changed, grouped by file. If nothing was edited, say so.

## Rules

These rules are derived from the existing crates — the three `policies/*`
crates (`session-policy`, `spending-limit-policy`, `threshold-policy`),
`account-factory`, and the three `latch-verifiers/*` crates. A few are
marked **(target, not yet universal)** — a convention this repo has decided on going forward, that
older code doesn't fully follow yet. Don't silently rewrite old code to match
without flagging it; surface it as a discrepancy per the workflow above.

### File layout

Each crate is a single `src/lib.rs` plus `src/test.rs`, not OZ's
`mod.rs`/`storage.rs`/`test.rs` split — these are small, single-purpose
contracts, not multi-extension library packages, so one file is the right
size. If a crate's logic genuinely splits into more than one concern, pull
the second concern into its own named submodule (the only current example:
`policies/session-policy/src/allowlist.rs`, declared via `mod allowlist;` in `lib.rs`
and re-exported with `pub use allowlist::*;`).

Ordering inside `lib.rs`:

1. `#![no_std]`
2. `use` imports — `soroban_sdk` first, then `stellar_accounts`, one grouped
   block per crate (see "Imports").
3. `#[contracttype]` structs/enums (config, params, storage keys).
4. `#[contracterror]` enum.
5. `#[contractevent]` structs.
6. `#[contract] pub struct X;`
7. `#[contractimpl] impl X { ... }` — public entrypoints, constructor first.
8. Private helper functions.
9. `#[cfg(test)] mod test;` — always last.

Two established shapes for policy crates, pick the one that matches what the
crate is actually doing:

- **Thin wrapper** (`threshold-policy`, `spending-limit-policy`) — the
  crate has no logic of its own. Every `Policy` trait method, and any extra
  query/mutation methods, is a one-line delegation into the matching
  `stellar_accounts::policies::*` free function.
- **Own logic** (`session-policy`) — the crate implements real logic OZ
  doesn't provide. Follow OZ's own internal module style here: a
  module-level `//!` docstring, `// ################## NAME
  ##################` section banners (`CONSTANTS`, `QUERY STATE`, `CHANGE
  STATE`), and full rustdoc (`# Arguments` / `# Errors` / `# Events`) on
  every public function.

### Naming

- **Errors**: `<Thing>Error`, `#[contracterror] #[derive(Copy, Clone, Debug,
  Eq, PartialEq)] #[repr(u32)]`, PascalCase variants. Each crate is an
  independently deployed contract, not a module of a shared library, so
  numbering starts fresh at `1` per crate — there is no shared range to
  respect (contrast with OZ, where e.g. `fungible` owns the 100s and
  `access-control` the 2000s). `policies/session-policy/src/allowlist.rs`'s
  `SessionError` doc comment states this explicitly; new crates should say
  the same.
- **Events**: `#[contractevent]` struct, PascalCase, past-tense or noun
  (`SessionEnforced`, `AccountCreated`). `#[topic]` on the smart-account /
  identity field(s), appearing before non-topic fields. **(target, not yet
  universal)** every event should be paired with a snake_case `pub fn
  emit_<event>(e: &Env, ...)` helper that constructs and `.publish(e)`s it —
  don't call `.publish(e)` inline at the call site. `session-policy` and
  `factory-contract` currently publish inline; that predates this
  convention and is a known gap, not a blocker.
- **Storage keys**: `<Thing>StorageKey` enum, `#[contracttype]`.
  Parameterized variants inline their data directly (e.g.
  `AccountContext(Address, u32)`).
- **Constructors**: `__constructor`, defined in the contract's
  `#[contractimpl] impl X { ... }` block, first parameter `env: Env` (owned,
  not borrowed — this is the top-level contract entrypoint, not a library
  free function).
- **Verifier associated types**: keep `KeyData`/`SigData` to minimal wire
  types (`Bytes`, `BytesN<32>`, `BytesN<64>`). Document the wire format and
  any protocol quirk (e.g. why Phantom needs a text prefix, what bytes a
  WebAuthn `sig_data` XDR blob contains) directly on `verify`'s doc comment —
  this is a real strength of the existing verifiers, keep doing it.
- **Stub contracts**: an intentionally-unimplemented contract (e.g.
  `secp256k1-verifier`) gets a `// STUB — ...` block comment at the top of
  the file explaining what's missing and why it's still deployed, plus its
  own `NotImplemented` error variant — not a bare `panic!` or `todo!()`.

### Errors and panics

- Raise contract errors with `panic_with_error!(e, EnumName::Variant)`. Bare
  `panic!`, `unreachable!`, and `unwrap()` are violations outside test code
  (`clippy.toml` sets `allow-unwrap-in-tests = true`). `expect("...")` is
  fine when the message explains *why* the value is guaranteed present.
  `ed25519-phantom-verifier`'s `assert!(hash.len() == PAYLOAD_LEN, ...)` is
  an existing discrepancy against this rule — flag it, don't silently copy
  it into new code.
- Document errors in a `# Errors` section on public functions that can
  panic, one bullet per variant, `` [`ModuleError::Variant`] - <reason> ``.

### Cargo.toml shape

- One workspace, defined in the root `Cargo.toml`. New crates get added to
  its `members` list — they do not declare their own `[workspace]` table.
- `soroban-sdk` and `stellar-accounts` are pinned once in the root
  `[workspace.dependencies]` (from crates.io, e.g. `"=0.7.2"`, not a git
  `rev`). Member crates reference them as `{ workspace = true }` in both
  `[dependencies]` and `[dev-dependencies]` — never an inline version string.
  A crate-specific extra dependency used by only one or two crates (e.g.
  `ed25519-dalek` in `ed25519-phantom-verifier`, `p256` in
  `webauthn-verifier`) stays a direct dependency in that crate's own
  `Cargo.toml`, not promoted to the workspace level.
- `[profile.release]` / `[profile.release-with-logs]` live once in the root
  `Cargo.toml` — Cargo only honors profile tables in the workspace root: a
  `[profile.*]` table in a member crate's `Cargo.toml` is silently ignored.

### Imports

- One grouped `use` block per crate (`soroban_sdk`, then `stellar_accounts`,
  then anything else), matching `rustfmt.toml`'s `imports_granularity =
  "Crate"` / `group_imports = "StdExternalCrate"`.
- No wildcard imports (`use foo::*;`) outside test files pulling in a
  sibling module's items (e.g. `use crate::allowlist::*;` in
  `policies/session-policy/src/test.rs`).

### Documentation

- Triple-slash `///` for rustdoc, double-slash `//` for inline notes.
- Every public item gets at least a one-line summary. Functions with
  non-trivial behavior get the fuller `# Arguments` / `# Errors` / `#
  Events` block, in that order, skipping sections that don't apply.
- `cargo doc --no-deps` should run cleanly with no broken intra-doc links.

### Testing

- Tests live in `src/test.rs`, gated by `#[cfg(test)] mod test;` in
  `lib.rs`. Test files start with `#![cfg(test)] extern crate std;`.
- Setup: `Env::default()`, `Address::generate(&e)`, `e.register(...)`. Mock
  contracts are named `Mock<X>Contract`.
- Prefer `e.mock_all_auths()` for ordinary tests. If a test needs to
  exercise the auth machinery directly instead (calling
  `smart_account::do_check_auth` rather than going through
  `require_auth()`), explain why in a doc comment on the test module — see
  `latch-smart-account/src/test.rs`'s session-integration-tests header
  comment as the model for how to justify the deviation.
- Panic tests should use the numeric form,
  `#[should_panic(expected = "Error(Contract, #<code>)")]`, matching the
  `#[repr(u32)]` value — not a string-pattern match on the variant name.

### Lint suppression

- `#[allow(...)]` silencing clippy or compiler warnings is a violation.
  Fix the underlying code. If a warning genuinely can't be addressed, stop
  and escalate to the user instead of suppressing it.

---

## Important notes

- When unsure whether something is "wrong" or just "different from OZ",
  check whether it appears consistently in this repo's own crates first.
  Our conventions are allowed to diverge from OZ's where our crates are
  simpler — the point is internal consistency, not mirroring OZ line for
  line.
- Items marked **(target, not yet universal)** are conventions this repo
  has decided on for new code. Don't retroactively rewrite old crates to
  match as a side effect of an unrelated change — raise it as its own,
  explicit cleanup.
