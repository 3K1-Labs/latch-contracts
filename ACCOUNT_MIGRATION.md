# G-Address to Latch Smart Account Migration

Defines how a user with an existing Stellar G-address migrates to a Latch smart account (C-address),
with explicit decisions on every open question from the initial spike.

See [`ACCOUNT_MIGRATION_SPIKE.md`](ACCOUNT_MIGRATION_SPIKE.md) for the earlier exploratory notes
that informed this document.

---

## Decision: The smart account is the new primary wallet

The C-address is where the user's assets will live going forward — not a controller layer on top
of a G-address. This matters for several downstream decisions:

- The G-address should eventually be closed (not kept live indefinitely).
- Trustlines belong on the C-address once migration is complete.
- The Ed25519 signer is a bootstrap, not a permanent architecture — passkey/recovery enrollment
  should be encouraged (but not required) as a post-migration step.

This also means Latch needs to fund the smart account before assets can arrive. See § Soroban
ledger entry reserve below.

---

## What Migration Covers (and Doesn't)

**In scope (this document):**

- Single-keypair G-address (one Ed25519 signer) to single-signer Latch smart account
- XLM migration
- SAC token migration (assets with a deployed Stellar Asset Contract)
- Trustline setup on the C-address
- G-address cleanup (optional but complete)
- Passkey/recovery signer enrollment after migration

**Out of scope:**

- G-addresses with multiple signers or threshold > 1
- Assets that have no deployed SAC (see § Assets with no SAC)
- CEX withdrawal routing / bridge proxy (separate concern)
- Recovery flows if the user loses device access (separate concern)
- Multisig-to-multisig migration

---

## Migration Paths

### Path A — Ed25519 external signer (recommended)

The smart account is created with the user's existing Ed25519 public key as an
`External(ed25519_verifier, pub_key)` signer. The keypair stays on the device — only the signing
payload changes (raw hash on G-address → `__check_auth` hash on C-address).

This is the clean path. After migration the G-address serves no purpose and can be merged.

### Path B — Delegated signer (bootstrap-then-rotate)

The smart account is created with `Signer::Delegated(g_address)` and assets are moved first,
then the user rotates to `External(ed25519_verifier, pub_key)` and removes the delegated signer.

Use this path only if Path A produces unacceptable UX friction during onboarding (e.g., the
signing payload change breaks a specific SDK integration). It requires an extra user action and
leaves a window where the G-address is still a live signer on the smart account.

Both paths produce the same end state: smart account controlled by the Ed25519 key, G-address
closed, no delegated signer remaining.

---

## Step-by-Step Migration Flow

### Step 1 — Detect migration state

Before showing any migration UI, read the current on-chain state. All reads are horizon/RPC
queries, no contract calls required.

```
detect(pub_key):
  g_address   = stellar_address_from_pub_key(pub_key)
  c_address   = factory.get_account_address(ed25519_params(pub_key))
  g_balance   = horizon.account(g_address).balances      // XLM + trustlines
  c_exists    = rpc.get_ledger_entry(c_address) != null
```

Derive the migration state entirely from on-chain reads (no backend needed):

| State | Condition |
|--|--|
| `not_started` | G-address has assets, C-address does not exist |
| `account_deployed` | C-address exists, G-address still has non-reserve assets |
| `assets_moved` | G-address has only the base reserve left (≤ 1 XLM, no trustlines) |
| `complete` | G-address does not exist (merged) or user explicitly chose to keep it |

### Step 2 — Deploy the smart account

If the C-address does not exist yet, call `factory.create_account` with the user's Ed25519 public
key. The factory is idempotent — safe to call again if the UI is interrupted and replayed.

**Canonical account_salt for migration (Path A):**

```
account_salt = sha256("latch.migration.v1" || ed25519_pub_key_bytes)
```

Fix this salt in the client for the migration flow. A user migrating their primary G-address
always gets the same C-address for the same key — there is no ambiguity about which smart account
is "the migration account." This is different from normal account creation, where the salt is
random. Document the salt derivation so it can be replicated across SDK versions.

### Step 3 — Fund the C-address (Soroban ledger entry reserve)

A Soroban contract account must hold enough XLM to cover its ledger entry reserve before it can
receive tokens or be used as an asset destination. The current minimum is approximately 0.5 XLM
(one base reserve, 0.5 XLM) for the contract entry itself; SAC trustlines on the contract add
further entries at 0.5 XLM each.

**Who funds it:** The user's G-address funds the smart account with a classic payment for the
reserve floor before any tokens are transferred.

**How much:** `num_assets_to_migrate * 0.5 XLM + 0.5 XLM base`. Round up and leave a small
fee buffer. The exact amount depends on the number of SAC trustlines to be opened.

The factory deployment itself may initialize the contract entry with some XLM to satisfy the
minimum; check whether the current factory version does this. If not, the first payment to the
C-address (step 4) must be at least the required minimum.

### Step 4 — Open trustlines on the C-address

For each asset the user wants to migrate, the smart account must open a trustline (SAC trust
entry) before it can receive that token.

This is a Soroban invocation on the SAC contract authorized by the smart account itself:

```
for each asset in assets_to_migrate:
    sac_contract(asset).set_trustline_authorized(smart_account, true)
    // or: smart_account calls sac.trustline_entry to opt in
```

The exact mechanism depends on how the SAC exposes trustline management for contract accounts.
In practice, a contract account can receive a token without the classic `change_trust` operation
as long as the asset issuer authorizes it (or the asset has no auth required flag). Audit this
against the assets the user actually holds — particularly USDC (Circle-issued, auth-required flag
varies by network).

**Action item:** Confirm trustline mechanics for SAC accounts for each asset class in the user
population (XLM, USDC, other issued assets). This is a required open question before shipping.

### Step 5 — Transfer assets

Build the asset sweep as a single transaction envelope where possible.

**XLM:**

```
Payment {
    source:      g_address,
    destination: c_address,
    asset:       native,
    amount:      g_xlm_balance - fee_buffer - base_reserve
}
```

Leave the G-address base reserve (1 XLM) intact at this stage. Recover it in step 7.

**SAC tokens (one per asset):**

```
InvokeContractOp {
    contract: sac_address(asset),
    function: "transfer",
    args:     [g_address, c_address, full_balance]
}
```

The G-address keypair authorizes all ops. They can go in one envelope as long as the total
operation count stays within Stellar's per-transaction limit (currently 100 ops).

For users with many assets (> ~90), split into multiple transactions. The migration state
machine handles partial completion across transactions.

### Step 6 — Verify

After the sweep transaction confirms, re-read balances from the ledger:

- G-address should have only the base reserve (≈ 1 XLM) and no trustlines.
- C-address should have the migrated XLM and all token balances.

If any balance is missing, surface a recoverable error and let the user retry the specific
asset(s) that failed.

### Step 7 — Close the G-address (optional but recommended)

Once all assets are on the C-address, offer the user the option to merge the G-address:

```
AccountMerge {
    source:      g_address,
    destination: c_address
}
```

This:
- Sends the remaining base reserve XLM to the C-address.
- Removes all remaining trustlines (must be zero-balance first — this should already be true).
- Deletes the G-address from the ledger permanently.

The merge must be signed by the G-address keypair. It cannot be reversed.

**Merge destination:** The smart account (C-address). Merging into the smart account keeps all
value under one identity, which is the right UX default. Advanced users who want to route the
reserve elsewhere can do so, but the default should be the C-address.

**Why recommend closing:** Every live G-address with no assets is a footgun — a user could
accidentally send funds to it later, believing it is their active wallet. Closing it makes the
migration irreversible and unambiguous. If the user declines, mark the state as `complete`
anyway — do not block them indefinitely.

### Step 8 — Enroll passkey/recovery signer (optional, post-migration)

After migration is complete, the user controls the smart account with the same Ed25519 key they
used before. The smart account's architecture makes adding more signers straightforward:

```
smart_account.add_signer(
    context_rule_id: 0,   // default rule
    signer: Signer::External(webauthn_verifier, passkey_pub_key)
)
```

Then, once the user has confirmed the passkey works, remove the Ed25519 signer:

```
smart_account.remove_signer(
    context_rule_id: 0,
    signer_key: ed25519_pub_key
)
```

Or keep both signers active for a period before removing the Ed25519 key — the user decides.

This step is not required to complete migration. The smart account is fully operational with only
the Ed25519 signer. Passkey enrollment is an upgrade path, not a migration gate.

---

## Soroban Ledger Entry Reserve

A Soroban contract instance on the Stellar ledger requires XLM staked as a reserve. Key numbers:

| Entry type | Reserve cost (approximate) |
|--|--|
| Contract instance (base) | 0.5 XLM |
| Per SAC trustline entry | 0.5 XLM |

A user migrating 4 assets (XLM + 3 tokens) needs approximately 0.5 + 3 × 0.5 = 2 XLM held in
the C-address just for ledger entries — before any actual token balances.

These numbers come from Stellar's base reserve definition (currently 0.5 XLM per ledger entry).
They may change with protocol upgrades. Check the current network value at migration time.

**Implication:** A user with exactly 1 XLM on their G-address cannot fully migrate if the
reserve requirement exceeds their balance minus fees. Surface this clearly in the UI ("You need
at least X XLM to migrate all your assets. Migrate without USDC, or fund your account first.").

---

## Assets with No SAC

Not every Stellar asset has a deployed Stellar Asset Contract. Without a SAC, there is no
`transfer` function to invoke — the only way to move the asset is via classic Stellar payment ops,
which operate on G-addresses, not C-addresses.

**Behavior:** Skip silently, surface a warning.

- Show the user which assets cannot be migrated and why.
- Do not block migration of other assets.
- Leave those assets on the G-address; do not close the G-address if non-migratable assets remain.
- When SAC support arrives for a given asset later, the user can do a partial migration of that asset.

**Note:** If the user has only non-SAC assets and XLM, migration still makes sense (the smart
account can hold XLM directly). In that case the migration is XLM-only.

---

## Do We Need a Migration Helper Contract?

Short answer: no, not for v1.

| Concern | Client-orchestrated | Helper contract |
|--|--|--|
| Atomicity | Single Stellar tx covers all ops | Same, but encapsulated |
| Batch support | Multi-op envelope | Same |
| Safety checks (balance, reserve) | RPC pre-read, UI enforcement | Could do on-chain |
| Gas / fee | User pays directly | Relayer can abstract |
| Auditability | Transparent ops | Another contract to audit |
| Upgrade surface | None | New contract to maintain |

A migration helper contract would mainly buy two things: relayer-fee abstraction (so the user
does not need XLM on their G-address just to pay fees) and on-chain atomic reserve+sweep in
one invocation. Neither is a blocking concern for v1.

Build client-orchestrated first. Add a helper contract later if fee abstraction for zero-XLM
users becomes a real need.

---

## Transaction Batching

The full migration sweep can be batched into a small number of transactions:

| Transaction | Contents | Signer |
|--|--|--|
| T1 | Factory `create_account` | G-address (or relayer) |
| T2 | XLM payment to fund reserve + all SAC transfers | G-address |
| T3 | `AccountMerge` (optional) | G-address |

T1 and T2 can be merged into one envelope if the factory `create_account` and the asset ops fit
within the Stellar operation limit. In practice they can be separate if the UX wants a
"deploy first, then sweep" two-step confirmation.

T3 is always separate — it is irreversible and deserves its own user confirmation.

---

## Offers, Sponsorships, and Other Ledger Entries

A G-address may hold entries beyond simple balances:

| Entry | Migration behavior |
|--|--|
| Open DEX offers | Must be cancelled before merge (AccountMerge will fail if offers exist) |
| Sponsored entries | Sponsorship must be revoked or transferred; sponsored entries cannot be merged |
| Signers (other than the primary keypair) | Must be removed before merge |
| Data entries | Must be deleted before merge |

The migration UI should detect and surface these blockers before offering the merge step. The
user must resolve them manually or the client must automate the cleanup (cancel offers, remove
extra signers/data entries) as part of the pre-merge step.

For v1, detect and surface blockers with clear error messages. Automated cleanup is a nice-to-have
for a later iteration.

---

## Security Considerations

- **Do not store the private key anywhere.** The migration flow never touches the raw private key.
  All signing happens through the device's existing signing mechanism (same as today for G-address
  transactions).
- **The delegated signer must be removed.** If Path B is used (delegated signer bootstrap), the
  G-address remains a valid signing key on the smart account until `remove_signer` is called. Make
  this step mandatory in the Path B UX — do not let users exit the flow with a live delegated signer.
- **Verify the C-address before merging.** Before merging the G-address, confirm the smart account
  is reachable and the user can authorize a transaction on it. This prevents accidentally merging
  into an address that turns out to be inaccessible.
- **Reserve floor enforcement.** After the asset sweep, ensure the C-address holds enough XLM to
  cover all its ledger entries. If the XLM balance falls below the reserve, the contract becomes
  inactive. Enforce this in the sweep amount calculation.

---

## Open Questions (Blocking)

These must be answered before implementation:

1. **Soroban contract ledger entry reserve exact amount.** Confirm the current protocol reserve
   per ledger entry on testnet/mainnet (expected: 0.5 XLM, but verify with current Stellar core
   release and check whether upcoming protocol changes affect this).

2. **Trustline mechanics for SAC contract accounts.** Confirm how a Soroban contract account
   opts into an asset — whether `set_trustline_authorized` or another mechanism is needed for
   assets with `AUTH_REQUIRED` flag (specifically USDC on mainnet).

3. **Factory-funded minimum balance.** Does the current factory deployment initialize the
   contract entry with any XLM? If not, what is the exact minimum needed for the first payment
   to land successfully?

4. **Account merge preconditions.** Confirm the full list of ledger entry types that block an
   `AccountMerge` in the current protocol version, and whether the Stellar SDK exposes an API
   to enumerate them before attempting the merge.

---

## Open Questions (Non-blocking)

These can be deferred to a later iteration:

5. **Fee abstraction for zero-XLM users.** Can a relayer pay the deployment and sweep fees on
   behalf of the user? This would remove the requirement that the G-address holds enough XLM
   for fees, improving the "I have 0.01 XLM and 50 USDC" case. Requires a fee-sponsor or
   relayer service.

6. **Partial migration UX.** If the user only wants to move some assets (e.g., USDC but not an
   obscure token), should the UI support that explicitly? Or always sweep all migratable assets
   and let the user leave the rest?

7. **Legacy account_salt versions.** If a user already has a smart account from an older factory
   version with a different salt derivation, how do we detect and handle it?

8. **Notification strategy.** How does the extension know when a user who has not yet migrated
   opens the app — i.e., when to prompt for migration vs. when to show the normal home screen?

---

## Success Criteria

Migration is complete when all of the following are true:

1. A smart account exists at the C-address derived from the user's Ed25519 public key.
2. All XLM has moved from the G-address to the smart account (minus fees and any non-migratable
   reserve if the user chose not to merge).
3. All SAC token balances have moved from the G-address to the smart account.
4. The user can authorize a transaction from the smart account using the same device key they
   used to control the G-address.
5. No delegated signer pointing at the G-address remains on the smart account.
6. (Optional) The G-address has been merged and removed from the ledger.
