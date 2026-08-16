# Research Log

Running log of conclusions, insights, and decisions drawn from studying the reference projects and codebase.

---

## Workspace Structure

- `latch-contracts` — Rust/Soroban smart contracts (primary working directory)
- `latch` — Next.js frontend + API routes (the app)
- Both repos together form the full Latch system

---

## Project Purpose

Latch enables users to control **Stellar Soroban Smart Accounts (C-addresses)** using wallets they already own — Phantom, MetaMask, or passkeys — without ever needing a Stellar seed phrase.

Two adoption problems being solved:
1. **Funding gap** — CEXs and on-ramps only support G-addresses, not C-addresses
2. **Tooling gap** — no production wallet treats C-addresses as first-class citizens

---

## References Overview

The `references/` folder contains four key projects:

| Folder | What it is |
|---|---|
| `stellar-contracts` | OpenZeppelin Rust library — defines `SmartAccount` trait, `Verifier` trait, context rules, signers, policies. The foundational framework everything builds on. |
| `smart-account-kit` | TypeScript SDK by kalepail (not OZ core) — passkey-based wallet creation and management, built on top of OZ deployed contracts |
| `argent-contracts` | Cairo smart account implementation (Starknet) — reference for account architecture patterns |
| `braavos-account-cairo` | Another Cairo smart account reference |

---

## How the Three Stellar Projects Relate

```
stellar-contracts (OZ Rust library)
    ↓  defines traits SmartAccount, Verifier, policies
latch-smart-account + smart-account-kit's deployed contracts
    ↓  implement those traits, deployed on-chain
smart-account-kit-bindings (generated TypeScript)
    ↓  TypeScript bridge to the deployed contract interface
smart-account-kit/src (the passkey SDK)
    ↓  consumes bindings, orchestrates WebAuthn + Soroban flows
```

Latch's `latch-smart-account` is doing the exact same thing as smart-account-kit's contracts — both implement OZ's `stellar-contracts` traits. That's what ties all three repos together.

---

## smart-account-kit Was Built SDK + Demo Together

The initial git commit (Dec 18, 2025) dropped both `src/` (SDK) and `demo/` simultaneously. They were developed in tandem — the demo drove what the SDK needed to expose, the SDK enforced clean abstractions the demo couldn't shortcut around. This is probably the right model for Latch SDK too.

---

## No Factory Contract in smart-account-kit

The kit uses **direct contract deployment** — a server-side deployer keypair signs and pays for deployment:

```typescript
SmartAccountClient.deploy(
  { signers: [signer], policies: new Map() },
  {
    wasmHash: deps.accountWasmHash,
    publicKey: deps.deployerKeypair.publicKey(),  // deployer keypair
    salt: hash(credentialId),                      // credentialId as salt
  }
)
```

No factory contract in the middle.

---

## Direct Deploy vs Latch Factory — Critical Difference

| | smart-account-kit | Latch Factory |
|---|---|---|
| **Who deploys** | Server-side deployer keypair | The factory contract |
| **Salt** | `hash(credentialId)` | `hash(signers + threshold + account_salt)` |
| **Address depends on** | deployer's G-address + credentialId | signer parameters only |
| **Caller matters?** | Yes — different deployer = different C-address | No — caller irrelevant |
| **Multi-signer types** | WebAuthn only | Phantom + MetaMask + WebAuthn + delegated |

The kit ties the C-address to the deployer keypair — if that keypair rotates, users can't re-derive their address. Latch's factory derives the address from user's signer parameters only, enabling relayer patterns, account recovery, and true decentralization.

---

## What Is a Credential ID

When a user registers a passkey via WebAuthn, the authenticator (Face ID, Touch ID, YubiKey, etc.) generates:
- A **private key** — stays locked inside the authenticator, never leaves
- A **public key** — 65 bytes, secp256r1 uncompressed, handed to the app
- A **credential ID** — a random lookup handle assigned by the authenticator to find the keypair later

The credential ID is not a key — it's a token. Typically 16–64 bytes, base64url encoded. Format varies by authenticator.

In the kit: `keyData = publicKey (65 bytes) + credentialId (variable)` — both are stored together as the on-chain signer registration.

---

## Why Re-Deriving Account Addresses Matters

The kit's `hash(deployer + credentialId)` formula creates fragility in these scenarios:
1. **Device switch** — passkey syncs via iCloud/Google, but if deployer changes the formula breaks
2. **Deployer key rotation** — security breach forces key rotation, all existing users orphaned
3. **Account recovery** — user loses passkey, registers new one, new credentialId → different address → old account inaccessible
4. **Third-party integration** — an exchange can't compute your address without knowing your credentialId AND which deployer was used

Latch avoids all of this because address derivation uses only public signer parameters that anyone can know.

---

## AuthPayload — The Current Signing Standard (v0.7.0-rc.2)

The old `Signatures` tuple encoding is dead. The current standard is explicit `AuthPayload`:

```
AuthPayload {
  context_rule_ids: [u32],          // which rules authorize this operation
  signers: Map<Signer, bytes>       // signatures per signer
}
```

Context rule IDs are bound into the signed digest:
```
auth_digest = sha256(signature_payload || context_rule_ids.to_xdr())
```

This is a security improvement — a signature can't be replayed across different rules. You're signing "I authorize this action under rule #2", not just "I authorize this action."

**Latch implication:** `latch-smart-account` must implement this same `AuthPayload` pattern. The old tuple approach is dead upstream.

---

## Rule Discovery — No Bulk Method Anymore

`get_context_rules(type)` was removed from the OZ smart account contract. Rule discovery is now:
1. `get_context_rules_count()` — how many rules exist
2. `get_context_rule(id)` — fetch each one individually

The kit wraps this into `kit.rules.list()`. Without an indexer, listing all rules requires multiple RPC calls.

**Latch implication:** The wallet UI will need either an indexer or the enumerate-by-count pattern before building rule discovery features.

---

## Signer/Policy Removal Now Uses IDs

`add_signer` and `add_policy` now return a stable u32 ID on creation. Removal uses that ID rather than object equality. More reliable for on-chain operations.

---

## pnpm vs npm

pnpm is the right choice for a monorepo because:
- Single `pnpm install` across all workspaces
- Workspace linking — packages can import each other without publishing to npm
- One lockfile covers the entire repo
- Faster installs via global content-addressable store (one copy of each dep on disk)
- Stricter about phantom dependencies (you can only import what you declared)

The `latch` repo is currently a single Next.js app so it matters less there. But when Latch SDK is extracted as a proper package, pnpm monorepo is the right structure.

---

## Latch SDK — Future Shape

When the logic in `latch/app/api/` gets extracted into a proper SDK, the natural monorepo shape would be:

```
latch-sdk/
├── packages/
│   ├── core/        ← signing, auth payload, verifier logic
│   ├── react/       ← hooks, components
│   ├── bindings/    ← generated Soroban contract bindings
│   └── demo/        ← example app
├── pnpm-workspace.yaml
└── pnpm-lock.yaml
```

---

## WebAuthn Reference Lineage

The three WebAuthn reference implementations and how they relate:

```
kalepail/passkey-kit  →  OpenZeppelin/stellar-contracts  →  g2c + smart-account-kit
     (original)               (formalised library)           (both build on top)
```

stellar-contracts references kalepail in **attribution comments only** — not a code dependency. OZ adapted the `webauthn::verify()` logic and no-std `base64_url.rs` from kalepail's original passkey-kit.

### stellar-contracts — Framework Layer (Rust library, no deployed contract)
- `webauthn::verify()` — core secp256r1 verification logic
- `WebAuthnSigData` struct, `canonicalize_key()`, `base64_url.rs`
- Unit tests using real `p256` keypairs

### smart-account-kit — Client Layer (TypeScript SDK only)
No Rust verifier. WebAuthn work is entirely client-side:
- `webauthn-ops.ts` — registration + signing via `@simplewebauthn/browser`
- `auth-payload.ts` — digest construction and XDR encoding
- `utils.ts` — DER→compact, key extraction, challenge generation

### g2c — Complete End-to-End Reference (most directly relevant to Latch)
Located at `/Users/user/SuperFranky/latch/reference/g2c`:
- `contracts/webauthn-verifier/` — thin Rust wrapper over OZ `webauthn::verify()`
- `contracts/factory/` — simpler factory (funder keypair as deployer, single signer only)
- `crates/integration-tests/` — full Rust integration tests without a browser (synthetic P-256 keypairs, handcrafted authenticatorData + clientDataJSON)
- `packages/passkey-sdk/webauthn.ts` — key extraction with **CBOR fallback for mobile/Capacitor**
- `packages/passkey-sdk/signature.ts` — DER→compact with low-S normalization
- `packages/passkey-sdk/auth.ts` — auth hash building + signature injection into tx auth entries
- `packages/passkey-sdk/deploy.ts` — deterministic address computation + deployment
- `packages/contract-bindings/` — generated TS bindings per contract

**Primary references:**
- Rust contract + integration tests → **g2c** (has the standalone verifier contract + Rust tests without browser)
- TypeScript/client side → **smart-account-kit** (more robust — full managers, AuthPayload XDR, session management, relayer, multi-signer)
- They are complementary, not competing. Use both.

### What to copy vs adapt
| Source | What | How to use |
|---|---|---|
| `stellar-contracts/webauthn.rs` | `verify()`, `WebAuthnSigData`, `canonicalize_key()` | Copy directly — this IS the verifier logic |
| `stellar-contracts/base64_url.rs` | No-std base64url encoder | Copy directly |
| `stellar-contracts/test/webauthn.rs` | Rust test patterns, `sign()` helper, flag encoding | Blueprint for Latch verifier tests |
| `g2c/contracts/webauthn-verifier/contract.rs` | Thin verifier wrapper | Template for `latch-verifiers/webauthn-verifier` |
| `g2c/crates/integration-tests/` | Full cross-contract tests without browser | Blueprint for Latch integration tests |
| `g2c/packages/passkey-sdk/webauthn.ts` | CBOR fallback for mobile key extraction | Essential for Latch mobile support |
| `smart-account-kit/src/utils.ts` | `compactSignature()`, `extractPublicKeyFromAttestation()` | Client-side crypto utilities |
| `smart-account-kit/src/kit/auth-payload.ts` | `buildAuthDigest()`, `buildWebAuthnSignatureBytes()`, XDR encoding | Client-side AuthPayload construction |

---

## Current Latch Implementation Status

| Contract | Status |
|---|---|
| Factory (`latch-account-factory`) | Complete — fully specified, tested, deployed on testnet |
| Smart Account (`latch-smart-account`) | Basic version functional |
| Ed25519 Phantom verifier | Complete |
| Secp256k1 verifier (MetaMask) | Spec written, implementation pending |
| WebAuthn verifier (passkeys) | Spec written, implementation pending |
| Threshold policy | Planned — can reuse OZ SimpleThresholdPolicy |

---

## Upgrade Path & Factory Immutability

Decided: factory stays fully immutable (no admin key, no timelock — new smart-account version =
new factory deployment); smart account should gain a self-authorized `upgrade()` gated by its own
`require_auth()`, same tier as `add_signer`/`remove_policy`, not a new external admin.

Full reasoning, rejected alternatives (single admin key, multisig, timelock), and what's still
undecided (exact `upgrade()` signature, auth scoping, storage migration) → see `UPGRADE_PATH.md`.
