# Latch Account Factory

A deterministic, idempotent smart account factory for Soroban. Validates and canonicalizes signer inputs, derives account addresses before deployment, and deploys smart account instances via shared singleton verifier and policy contracts.

Built on [OpenZeppelin Stellar Contracts](https://github.com/OpenZeppelin/stellar-contracts).

---

## Architecture

<img width="2560" height="2095" alt="factory" src="https://github.com/user-attachments/assets/eccfd0e3-a397-4a05-a2a1-6ba7b6885088" />



</br>
The factory is the only deployment path for Latch smart accounts. Singletons are deployed independently before the factory and never re-deployed by it. The factory holds no user state — all user data lives in the deployed account contracts.


---


## Public Interface

**Constructor** — called once at factory deployment:
```rust
fn __constructor(
    env: Env,
    smart_account_wasm_hash: BytesN<32>,
    ed25519_verifier: Address,
    secp256k1_verifier: Address,
    webauthn_verifier: Address,
    threshold_policy: Address,
)
```
Stores config as immutable instance storage. All four singleton addresses must point to deployed contracts — validated at construction. Calling again panics with `AlreadyInitialized`.

**`get_account_address`** — pure computation, no deployment:
```rust
fn get_account_address(env: Env, params: AccountInitParams) -> Address
```

**`create_account`** — validates, derives address, deploys. Idempotent — returns existing address without re-deploying. Emits `AccountCreated` on first deployment only:
```rust
fn create_account(env: Env, params: AccountInitParams) -> Address
```

**`get_verifier`** / **`get_threshold_policy`** — read stored singleton addresses from config.

---

## Key Concepts

### Signer Inputs

| Family | Shape | Auth mechanism |
|---|---|---|
| `Delegated(Address)` | Stellar `G...` account | Native Stellar account auth |
| `External(ExternalSignerInit)` | Public key + kind | `verifier.verify(key_data, payload, sig)` |

External signer key shapes:

| Kind | `key_data` | Notes |
|---|---|---|
| `Ed25519` | 32 bytes | Standard Stellar/Phantom key format |
| `Secp256k1` | 65 bytes, first byte `0x04` | MetaMask, Rabby, EVM wallets |
| `WebAuthn` | > 65 bytes, first byte `0x04` | Face ID, Touch ID, Windows Hello, YubiKey (P-256 pubkey + credential ID) |

The factory does **shape validation only** — length and prefix checks. Cryptographic verification is the verifier's job.

### Address Derivation

Addresses are deterministic — derived from parameters, not caller identity. Signer order does not affect the address (list is sorted before hashing).

```
LatchAccountSaltV1 = SHA256(
  "latch.factory.account.v1"  ||
  account_salt                ||   // 32 bytes
  signer_count                ||   // 4 bytes big-endian
  for each canonical signer:
    signer_code               ||   // 1 byte: 0x00=Delegated, 0x01=Ed25519, 0x02=Secp256k1, 0x03=WebAuthn
    signer_data_length        ||   // 4 bytes big-endian
    signer_data               ||   // XDR address (Delegated) or key_data (External)
  effective_threshold              // 4 bytes big-endian
)

SmartAccountAddress = DeployAddress(deployer=factory, salt=LatchAccountSaltV1, wasm=smart_account_wasm_hash)
```

The same signer set can produce multiple independent accounts by varying `account_salt`. Generate a random 32-byte salt per account; store it if the user needs address recovery.

### Threshold Policy

Single-signer accounts get no policy. Multi-signer accounts get `SimpleThresholdPolicy` installed with the given threshold at creation. The threshold is stored in the policy contract keyed by `(account_address, context_rule_id)` — it is not updated automatically when signers change post-creation.

---

## Validation Rules

| Check | Error |
|---|---|
| Zero signers | `NoSigners` |
| Duplicate signer | `DuplicateSigner` |
| Multi-signer with no threshold | `MissingThreshold` |
| Threshold `= 0` or `> n` | `InvalidThreshold` |
| Single-signer with threshold `> 1` | `InvalidThreshold` |
| Ed25519 `key_data.len() != 32` | `InvalidEd25519Key` |
| Secp256k1 `key_data.len() != 65` or first byte `!= 0x04` | `InvalidSecp256k1Key` |
| WebAuthn `key_data.len() <= 65` or first byte `!= 0x04` | `InvalidWebAuthnKey` |
| Singleton address not deployed | `InvalidEd25519/Secp256k1/WebAuthn/ThresholdPolicy Verifier` |
| Constructor called twice | `AlreadyInitialized` |

---

## Storage & Events

**Storage:** Instance only. One key: `DataKey::Config` → `FactoryConfig`. TTL extended to 30 days (`518400` ledgers) on every config read and in the constructor. No account state is stored — addresses are recomputed on demand.

**Events:** `AccountCreated { account: Address }` — emitted on first deployment only.

---

## Security

- **Caller identity does not affect account address.** Salt is derived purely from canonical params.
- **All validation happens before deployment.** No partial-deployment state is possible.
- **Account creation is atomic.** `deploy_v2` runs the constructor in the same transaction — account exists fully configured or not at all.
- **Config is immutable.** No admin function exists to update it. A different set of contracts requires a new factory.
- **No private keys ever passed.** Only public key material appears in transaction args or storage.

**Risks:**
- **Threshold drift:** Signer changes post-creation don't update the threshold policy. Removing signers without updating threshold can permanently lock the account.
- **Singleton contract trust:** Verifier and policy addresses are trusted at deploy time. There is no on-chain enforcement of what code lives at those addresses after the fact.
- **No upgrade path:** Bugs in any singleton or the smart account WASM require a new factory. Existing accounts remain under the old one.

---

## Development

```bash
# Build factory
stellar contract build --package factory-contract

# Build + copy test stubs (only needed after changing dummy contracts)
stellar contract build --package dummy-account
stellar contract build --package dummy-singleton
cp target/wasm32v1-none/release/dummy_account.wasm contracts/factory-contract/testdata/
cp target/wasm32v1-none/release/dummy_singleton.wasm contracts/factory-contract/testdata/

# Test
cargo test
```

Tests split into two layers: validation tests (use `install_factory_stub`, no real WASM needed) and deployment tests (use `install_factory` with embedded dummy WASMs). The stubs in `testdata/` are committed so CI can run `cargo test` without a WASM build step.
