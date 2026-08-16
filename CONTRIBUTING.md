# Contributing to Latch Contracts

Thanks for your interest in contributing. Please take a few minutes to review
this before opening a pull request — it'll save time for both of us.

## Start With an Issue, Not a PR

**Do not open a pull request without a corresponding issue and prior
discussion** (unless the change is trivial — e.g. fixing a typo or a broken
link).

Getting the design right is the most important step. Once we agree on the
approach, the implementation itself becomes straightforward. Skipping this
step often leads to PRs that don't fit the project's direction, which wastes
time for everyone.

**The expected workflow is:**

1. [Open an issue](https://github.com/3K1-Labs/latch-contracts/issues/new)
   describing what you want to add or change, and why.
2. Discuss the design with the maintainers in the issue.
3. Once the design is agreed on, start the implementation and open a PR.

PRs that arrive without prior discussion may be closed and redirected to an
issue first. This isn't bureaucracy for its own sake — it's to make sure your
effort results in something that can actually be merged.

## Creating Pull Requests

Fork this repository, work on your own fork, and submit pull requests from
there. See GitHub's ["Fork a repo"](https://docs.github.com/en/get-started/quickstart/fork-a-repo)
guide for how this works.

## Repository Layout

This repo is **not** a single Cargo workspace — every top-level crate is its
own independent workspace with its own `Cargo.lock`:

```
latch-smart-account/            the account contract
latch-account-factory/          factory + deploy-time test stubs
latch-threshold-policy/         wraps OZ's simple_threshold policy
latch-verifiers/
  ed25519-phantom-verifier/
  secp256k1-verifier/           stub, not yet implemented
  webauthn-verifier/
session-policy/                 method-allowlist policy
spending-limit-policy/          wraps OZ's spending_limit policy
```

All commands below (`cargo test`, etc.) are run from inside the specific
crate directory you're working on, not the repo root.

## A Typical Workflow

1. Make sure your fork is up to date:

    ```sh
    git remote add upstream https://github.com/3K1-Labs/latch-contracts.git
    git fetch upstream
    git pull --rebase upstream main
    ```

2. Branch out from `main`:

    ```sh
    git checkout -b fix/short-description-#123
    ```

    (The `#123` suffix links your branch to the issue it addresses.)

3. Make your changes, add tests, update documentation, commit, and push to
   your fork.

4. Run the checklist below and make sure it passes before opening a PR:

    ```bash
    cd <crate-you-changed>

    # Format (NIGHTLY required — rustfmt.toml uses unstable_features)
    cargo +nightly fmt --all -- --check

    # Lint
    cargo clippy --all-targets --all-features -- -D warnings

    # Test
    cargo test

    # WASM release build
    cargo build --target wasm32v1-none --release
    # or, equivalently:
    stellar contract build

    # Doc check
    cargo doc --no-deps
    ```

5. Open a pull request against `main`. Start the body with "Fixes #123" (or
   "Resolves #123") to link it to the issue it resolves, and follow the
   [PR template](.github/pull_request_template.md).

6. A maintainer will review your code, check that everything passes, and
   may ask for changes before merging.

## Tests

New features need tests. If you're not sure whether something needs a unit
test or an integration test, ask in the PR — we'd rather help early than
close it late.

## Use of AI Tools & Code Conventions

We welcome contributions regardless of how they're written — including with
the help of AI coding assistants. That said, AI-generated code needs the same
level of scrutiny as any other code, and in practice it often needs *more*.

Even capable AI models produce subtle mistakes: incorrect assumptions about
this library's conventions, unnecessary abstractions, or code that compiles
but doesn't match the design. These aren't always obvious at a glance, but
they add up during review.

**What we expect from AI-assisted contributions:**

- **You're responsible for the code you submit.** Treat AI output as a first
  draft, not a finished product. Review it thoroughly, understand every line,
  and check it against this repo's conventions.
- **Run the checklist above locally before opening a PR.** At minimum,
  `cargo test`, `cargo clippy`, and `cargo fmt` must all pass.
- **Match this repo's patterns and style.** All code, whether written by
  hand or with AI assistance, is expected to follow the conventions in
  [`.claude/commands/code-quality.md`](.claude/commands/code-quality.md).
  It covers file layout, naming, error/event conventions, the thin-wrapper
  vs. own-logic policy shapes, testing patterns, and more. This file is
  written to be human-readable too — read it before your first contribution.

**What happens with low-effort, unreviewed submissions:** a PR that's
obviously unreviewed AI output — full of basic mistakes or inconsistent
style — will be closed without a detailed review. This isn't about
discouraging AI usage, it's about respecting everyone's time. A good
contribution, however it was written, should feel like someone already
reviewed it.

## All Set

If you have questions, open an [issue](https://github.com/3K1-Labs/latch-contracts/issues).

Thanks for your time and code!
