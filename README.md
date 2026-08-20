<img width="4096" height="1536" alt="Latch 7" src="https://github.com/user-attachments/assets/e7042fc8-6b72-4ceb-933f-bd8a0a55c883" />


## Overview

Soroban smart contracts for the Latch auth layer. Provides deterministic smart account creation with support for Ed25519 and WebAuthn signers.

Latch accounts are Soroban smart accounts — programmable wallets that replace private-key-only authorization with flexible multi-signer, multi-policy authorization. Users can sign transactions with a Phantom wallet, a MetaMask wallet, a passkey (Face ID, Touch ID, fingerprint), or any combination of the three.

The system is built on the [OpenZeppelin Stellar Contracts](https://github.com/OpenZeppelin/stellar-contracts) smart account framework.

## Repository Structure

This repository is a **single Cargo workspace** — every contract is a member crate, sharing one
`Cargo.lock` and one pinned `stellar-accounts` version, built/tested independently via
`--package`.

```
latch-contracts/
├── account-factory/
│   └── contracts/
│       ├── factory-contract/    # ✅ Complete — the factory itself
│       ├── dummy-account/       # Test-only stub used by factory-contract's tests
│       └── dummy-singleton/     # Test-only stub used by factory-contract's tests
├── latch-smart-account/         # ✅ Smart account contract
├── latch-verifiers/             # ⚠️ Verifier contracts
│   ├── ed25519-phantom-verifier/
│   ├── secp256k1-verifier/      # Stub — not wired into the factory, unused in v1
│   └── webauthn-verifier/
├── latch-threshold-policy/      # ✅ Threshold policy
├── session-policy/              # ✅ Method-allowlist (session key) policy
├── spending-limit-policy/       # ✅ Spending-limit policy
├── factory-spec.md              # Behavioral spec for the factory
├── UPGRADE_PATH.md              # Account & factory upgrade path decision
└── PLAN.md                      # v1 architecture plan
```

## Contracts

### Factory — `account-factory/` ✅

The canonical entrypoint for creating Latch smart accounts. Validates and canonicalizes signer inputs, derives deterministic account addresses, and deploys new smart account instances.

**Key properties:**
- Address derivation is deterministic — same params always produce the same address
- Signer input order does not affect the derived address (canonical sort applied)
- Idempotent — calling `create_account` twice with the same params returns the existing account
- The same signer set can own multiple accounts via an explicit `account_salt`
- Verifier and policy contracts are pre-deployed and passed in at factory construction — the factory only ever deploys smart account instances

See [`account-factory/README.md`](account-factory/README.md) for full documentation.

### Smart Account — `latch-smart-account/` ✅

OZ-based programmable wallet contract. Implements `CustomAccountInterface`, `SmartAccount`, `ExecutionEntryPoint`, and `Upgradeable`. Initialized with a set of signers and optional policies by the factory. `upgrade()` is self-authorized — gated by the account's own signers via `require_auth()`, the same as every other mutation, not an external admin. See [`UPGRADE_PATH.md`](UPGRADE_PATH.md) for the reasoning.

### Verifiers — `latch-verifiers/` ⚠️

Stateless singleton contracts that verify signatures on behalf of smart accounts. One contract per signer kind, shared across all accounts on the network.

| Contract | Signer type | Key format | Status |
|---|---|---|---|
| `ed25519-phantom-verifier` | Phantom, Stellar wallets | 32-byte Ed25519 public key | ✅ Implemented |
| `webauthn-verifier` | Passkeys, Face ID, Touch ID, YubiKey | 65-byte P-256 key + credential ID | ✅ Implemented |
| `secp256k1-verifier` | MetaMask, EVM wallets | 65-byte uncompressed secp256k1 key | 🔜 Stub, not wired into the factory |

### Threshold Policy — `latch-threshold-policy/` ✅

OZ simple threshold policy. Enforces M-of-N authorization for multisig accounts. Deployed as a singleton shared across all multisig accounts.

### Session Policy — `session-policy/` ✅

Restricts a context rule's signers to an allow-listed set of contract function names — the building block behind Latch session keys. Own logic, not a wrapper around an OZ primitive.

### Spending Limit Policy — `spending-limit-policy/` ✅

Thin wrapper around OZ's `stellar-accounts` spending-limit policy. Enforces a rolling spend cap per context rule.

## Deployment Order

Before a factory can be deployed, all singleton contracts must already exist on the network. The required order is:

```
1. stellar contract install   # upload smart account wasm, capture hash
2. stellar contract deploy    ed25519-verifier
3. stellar contract deploy    webauthn-verifier
4. stellar contract deploy    threshold-policy
5. stellar contract deploy    factory  (pass smart_account_wasm_hash + 3 addresses)
```

## Development

### Prerequisites

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Stellar CLI (v25.2.0+)
cargo install --locked stellar-cli
```

### Build and test

`cargo +nightly fmt --all -- --check` formats/checks the whole workspace regardless of where you
run it from. Everything else scopes to one crate at a time — either `cd` into the crate directory
or pass `--package <name>` from the repo root (package names don't always match directory names,
e.g. `latch-smart-account`'s package is `smart-account` — see each crate's own `Cargo.toml`):

```bash
cargo +nightly fmt --all -- --check                          # whole workspace

cd latch-smart-account   # or any other crate listed above
cargo clippy --all-targets --all-features -- -D warnings     # lint, this crate only
cargo test                                                   # unit + integration tests
stellar contract build                                       # WASM build
```

## Spec and Planning

- [`factory-spec.md`](factory-spec.md) — Detailed behavioral specification for the factory contract (validation rules, address derivation formula, canonicalization, worked examples)
- [`UPGRADE_PATH.md`](UPGRADE_PATH.md) — How the factory and smart account handle upgrades and versioning
- [`PLAN.md`](PLAN.md) — v1 architecture plan covering all contracts in scope

## Contributing

Contributions are welcome — see [`CONTRIBUTING.md`](CONTRIBUTING.md) for the workflow (start with
an issue, not a PR) and the code conventions checklist. Security issues should go to
[`SECURITY.md`](SECURITY.md)'s contact instead of a public issue. Licensed under
[MIT](LICENSE).
