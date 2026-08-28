# Open-Source Readiness Checklist

A repo-agnostic checklist for getting any Latch repo ready for outside contributors. Apply this
to every repo (`latch-web-extension`, `latch-mobile`, `latch-api`, `latch-relayer`,
`latch-contracts-dapp`, and any future ones) — the checkboxes below reflect **this repo's**
(`latch-contracts`) current status, use it as the reference implementation for each item.

**How to use this**: work through it top to bottom. Items marked 🔧 need adapting to your
repo's language/stack (the concept is universal, the tool isn't). Items marked ⚠️ are things we
got wrong on the first pass here — read the note before you repeat the mistake.

---

## 1. Legal & Governance

- [x] **`LICENSE`** — pick one and commit it at the repo root. We went with plain MIT
      (`Copyright (c) 2026 3K1 Labs`). Don't overthink this one unless you have a specific reason
      to want Apache-2.0's explicit patent grant instead.
- [x] **`CODE_OF_CONDUCT.md`** — decide per-repo, don't assume the same answer everywhere. Added
      2026-08-22, matching `latch-mobile`'s style rather than the verbatim Contributor Covenant
      template originally recommended here — a short, project-voiced version with a
      contracts-specific addition (no introducing vulnerabilities, no disclosing security issues
      publicly before they're fixed, no soliciting private keys), "Adapted from" the Covenant
      rather than a verbatim copy. Worth revisiting this checklist's original "use it verbatim"
      guidance for other repos too, now that there's a real precedent that reads better.
- [x] **`CONTRIBUTING.md`** — document your actual contribution workflow, not an aspirational one.
      Ours: fork + issue-first (no PR without a prior issue, except trivial fixes), a per-crate
      command checklist, and an explicit AI-assisted-contribution policy (see §4). Copy the shape,
      swap in your repo's actual build/test/lint commands.
- [x] **`SECURITY.md`** — vulnerability disclosure contact, explicit scope (what's covered / what
      isn't — e.g. "bugs in our upstream dependencies go to them, not us"), and an honest audit
      status. Don't invent legal/ToS boilerplate you don't actually have backing for.

## 2. GitHub Repo Configuration

- [x] **`.github/pull_request_template.md`** — short: what issue this fixes, a Tests/Docs
      checklist.
- [x] **`.github/ISSUE_TEMPLATE/`** — structured bug report + feature request forms
      (`bug_report.yml`, `feature_request.yml`).
- [ ] **`CODEOWNERS`** — skip while you're the only maintainer, add once there's a second person
      whose review should be auto-requested. Deliberately deferred here.
- [x] **Branch protection via Rulesets** (not the older "classic branch protection rules" —
      GitHub is investing in rulesets going forward, and they're the only option that can also
      cover tags later). Ours: no force-push, PR required, squash-merge only, both existing CI
      jobs required to pass, branch does *not* need to be up-to-date before merge (see the note in
      `UPGRADE_PATH.md`-adjacent discussion about why "strict up-to-date" fights you, not helps,
      once multiple contributors have PRs open at once).
      ⚠️ **Verify it actually enforces what you configured.** We found a PR here merge via a
      regular 2-parent merge commit despite the ruleset only allowing `["squash"]` — still
      unresolved, likely an implicit owner/admin bypass. Check this on your repo before trusting
      the ruleset is doing what it says.

## 3. CI/CD

- [x] **Build + test on every PR.**
- [x] 🔧 **Lint/format enforced in CI, as a required check** — not just configured, actually
      wired to block merge. Ours: `cargo +nightly fmt --check` + `cargo clippy -D warnings`. Swap
      for your stack's equivalent (`eslint`+`prettier`, `ruff`+`black`, etc.) — same idea.
      ⚠️ **Turning this on will immediately fail every existing PR/file if nothing has ever been
      run through the formatter.** We hit this directly — reformat the whole repo as its own
      dedicated, no-behavior-change commit *before* you turn on the check as required, not after.
- [x] **Typo-checking** — [`crate-ci/typos`](https://github.com/crate-ci/typos) isn't Rust-specific,
      it works across most text-based source. Cheap, catches embarrassing stuff before merge.
- [x] **Path-filtering so docs-only PRs report fast** instead of running your full build matrix.
      Important detail: the check must still *always run and report* (even if it skips the heavy
      steps) — if a required status check simply never fires for certain file changes, PRs
      touching only those files can get stuck forever at "waiting for status to be reported."
- [x] **CI actually covers every package/module in the repo, not just the original ones.**
      ⚠️ We found this gap on ourselves: two crates (`session-policy`, `spending-limit-policy`)
      existed and had their own test suites, but nothing in CI ran them independently — they were
      only exercised indirectly as another crate's dev-dependency. Audit your CI matrix against
      your actual package list, don't assume it grew automatically.
- [x] **New CI jobs added to the branch ruleset's required checks.** Easy to forget — we expanded
      our CI matrix from 2 to 8 jobs over the course of this session and initially left the
      ruleset only requiring the original 2. All 8 `build-and-test` jobs plus `check-for-typos`
      are now required to merge.

## 4. Contributor Experience

- [x] **A written conventions/style doc**, not just a linter config — the actual patterns specific
      to this codebase (file layout, naming, error handling, testing patterns), derived from
      reading the real code, not copy-pasted from a template. Ours:
      [`.claude/commands/code-quality.md`](.claude/commands/code-quality.md), wired up as a
      `/code-quality` slash command too.
- [x] **Explicit AI-assisted contribution policy** in `CONTRIBUTING.md` — welcome it, but make
      clear the human submitting the PR owns and must review it, and that low-effort unreviewed AI
      output gets closed without detailed review, not nitpicked line-by-line.
- [x] **README readable by a stranger with zero context** — does it explain what the project does,
      how to build/run it, and where a first-time contributor should start? Ours was missing two
      crates that existed in the repo but not the README (`session-policy`,
      `spending-limit-policy`), referenced a `contracts/` directory that doesn't actually exist
      (a stale leftover from the same broken root `Cargo.toml` tracked in `TODO.md`), and had no
      pointer to `CONTRIBUTING.md`/`SECURITY.md`/`LICENSE` at all. Fixed.

---

## Notes for adapting this to a non-Rust repo

- The *practice* in every 🔧 item is universal; only the specific tool changes. Don't skip the
  item just because your stack doesn't have a `rustfmt.toml` equivalent — it does, find it.
- "Reformat everything first, then turn on enforcement" applies regardless of language — any
  formatter/linter you're adding fresh to an existing codebase will fail on old code otherwise.
- If your repo has multiple independent packages/modules (workspaces, monorepo packages, etc.),
  explicitly re-audit "does CI actually test all of them" — this is the mistake we made, and it's
  an easy one to repeat since it doesn't announce itself; the build just silently doesn't cover
  the new thing.
