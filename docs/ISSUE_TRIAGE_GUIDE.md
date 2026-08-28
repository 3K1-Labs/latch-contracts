# Issue Triage Guide

How we got every open issue in `latch-contracts` ready for outside contributors ahead of the
Stellar OSS wave — apply the same process to your own repo's open issues
(`latch-web-extension`, `latch-mobile`, `latch-api`, `latch-relayer`, `latch-contracts-dapp`).
This isn't a one-time cleanup, it's a process — the drift that made this pass necessary will
happen again, so re-run it periodically.

## The core idea

An issue that made sense when it was filed can silently go stale: the design it describes gets
superseded, the feature it references ships differently, the person who understands the open
question moves on. A contributor who picks up a stale issue wastes real time — or worse, builds
the wrong thing confidently. Triage means verifying every open issue still describes real,
buildable work, not just skimming titles.

## Process

**Work oldest-first, one issue at a time.** Don't batch — each issue can reveal repo-specific
context (a superseded design, a private doc, a resolved dependency) that changes how the *next*
issue should be handled. Batching means you miss that.

For each issue:

### 1. Read the full body, not just the title

Titles drift from what an issue actually asks for. We found a "dApp signing" issue whose real
scope, once read fully, was contract-level auth-entry validation — nothing to do with the actual
dApp-connection UI.

### 2. Verify every technical claim against the real, current codebase

Don't trust an issue's claims at face value — grep/read the actual code it references. We
verified a "recipient is argument index 1" claim against `spending-limit-policy`'s real source
before trusting it into a rewritten issue. An issue can be well-written and still wrong about the
codebase it's describing.

### 3. Check whether the work is already done, just differently

Search sibling repos for ADRs, checklists, "proof of completion" docs, and shipped code before
assuming an issue's design is still the plan. We closed six issues this way — a whole bridge
subsystem the issues described had been tried, hit real problems, and been replaced by a
simpler shipped design, documented in a sibling repo's own ADR that the issues never referenced.
Two more closed because the *proof* the issue asked for already existed: a real testnet
transaction hash and an existing adversarial test suite, not just a plausible claim.

### 4. Distinguish a genuinely open question from one already answered by the code

Don't take "this is blocked" at face value either — re-derive it. We had an issue blocked on a
UI design question; re-tracing the actual authorization mechanics showed the contract-level part
of that question was already answered by how the existing type system scopes permissions — the
only genuinely open parts were downstream UI questions that didn't need to gate the contract
work. We unblocked it once we'd actually verified that, not on a guess.

### 5. Treat a private-only reference as a real defect, not a formality

An issue whose only context lives behind a private Notion page (or similar) is unusable by an
external contributor. Confirm access before trusting the link's presence to mean "context
exists."

### 6. Security-sensitive design decisions are a maintainer's job, not an open contributor task

If an issue asks a contributor to make an irreversible cryptographic or authorization-boundary
decision, that's not a "help wanted" task — a maintainer should decide it directly (grounded in
whatever precedent already exists in the codebase) and hand contributors an implementation task
against a settled spec instead. We did this for a verifier's canonical encoding: closed the
"write the spec" issue, wrote the spec ourselves in an hour using a sibling verifier's existing
code as ground truth, and repointed the implementation issue at it.

### 7. Turn open-ended design questions into Discussions, not issues

An issue needs a closeable deliverable. If what's actually open is a question with no concrete
"done" yet, it belongs in Discussions, not the issue tracker — file it there, then reference it
from a `blocked` issue (see below) or just close the issue with a pointer if the issue itself was
premature.

### 8. Use `blocked` deliberately, and revisit it

Add a `blocked` label plus a one-line notice at the *top* of the issue body linking to whatever
it's blocked on. But don't let it go stale — when the blocker resolves (or turns out to already
be resolved), remove the label and update the notice. We found a `blocked` label had silently
disappeared from an issue outside of any of our own edits, leaving the label and the issue body
inconsistent — periodically diff the two against each other, don't assume a label is still
accurate just because it's there.

### 9. When a contract/backend primitive only matters once a client uses it, file the client issue too

A primitive with no consumer isn't done. If an issue's own motivation is "so the client apps can
do X" but its scope stops at the primitive, that's a real gap — file the companion issue in the
client repo(s), and link both directions. We did this three times and initially *missed* it once
on an issue whose own motivation section said the client gap was the actual point — catch this
by asking "does this issue's own justification point at work I haven't filed anywhere?"

### 10. Use GitHub's real relationship features, not prose

- **Sub-issues** (including across repos in the same org) for parent/child breakdowns — gives a
  live progress bar instead of an informal "see child issues" mention.
- **Issue dependencies** (`blocked_by`/`blocking`) for pure sequencing ("can't start until that
  one merges") — distinct from the `blocked` label, which we reserved for "there's an open
  question," not ordinary ordering.

Neither has full `gh` CLI support yet; both are reachable via `gh api`:

```bash
# Sub-issues (sub_issue_id is the issue's internal id, not its number —
# get it with: gh api repos/OWNER/REPO/issues/N --jq '.id')
gh api repos/OWNER/REPO/issues/PARENT_N/sub_issues -X POST -F sub_issue_id=<id>
gh api repos/OWNER/REPO/issues/PARENT_N/sub_issues --jq '.[].number'   # list

# Issue dependencies
gh api repos/OWNER/REPO/issues/BLOCKED_N/dependencies/blocked_by -X POST -F issue_id=<id>
```

### 11. Fix dangling references

"See X above" only works if X still exists. We found (and fixed) more than one case where a
section got edited or removed and a reference to it was left pointing at nothing — treat these as
real defects, not typos to shrug off.

### 12. Cite real, verifiable sources — pinned version, not `main`

When referencing an upstream dependency, link the exact pinned version your lockfile actually
resolves to (check the lockfile, not just the version range in your manifest — they can differ),
not the upstream repo's default branch, which will drift out from under the link. Confirm the
line number you're citing actually exists at that pinned tag before publishing the link.

### 13. Keep labels honest

- Apply your wave-readiness labels consistently — and sweep back over issues you handled *before*
  you settled on the label set, since it's easy to establish a convention partway through and
  forget the earlier issues never got it. We missed eight issues this way in one pass.
- If an issue is labeled `good first issue` but its own scope describes a full trait
  implementation with real design surface, that's a mismatch — downgrade it or say why it's
  still beginner-friendly.

## Definition of "ready" for a wave-program issue

- [ ] Body is self-contained — a contributor doesn't need access to anything private to understand it.
- [ ] Every technical claim has been checked against current code, not assumed from when it was written.
- [ ] Not already done elsewhere under a different name — checked sibling repos and shipped code, not just this repo's history.
- [ ] If blocked, the blocker is real, current, and linked — not stale, not vague.
- [ ] If it depends on other work, that's expressed as a real link (sub-issue / dependency), not just prose.
- [ ] If its own motivation implies work in another repo, that companion issue exists and is linked both ways.
- [ ] No dangling "see above" references.
- [ ] External references point at pinned versions, not a moving branch.
- [ ] Labels match actual scope and your program's current taxonomy.
