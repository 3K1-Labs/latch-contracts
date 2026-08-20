# Latch Factory Contract Spec v1

## 1. Purpose

The `factory` contract is the canonical entrypoint for creating Latch smart accounts on Soroban.

Its responsibilities are to:

- accept user-facing account initialization parameters
- validate and canonicalize signer inputs
- derive the deterministic smart-account address before deployment
- lazily deploy shared verifier contracts as needed
- lazily deploy the threshold policy contract when needed
- deploy the smart account atomically with its initial signers and policies
- return the already-deployed account if the same configuration was created earlier

The factory must be deterministic, idempotent, and stateless with respect to user keys.

## 2. Non-Goals

The factory does not handle:

- private key custody
- recovery flows
- bridge/funding logic
- session keys
- spending limits
- account upgrades/admin mutation
- signature verification itself

Those belong to other contracts.

## 3. Public Types

```rust
enum SignerKind {
    Ed25519,
    WebAuthn,
}
```

```rust
struct ExternalSignerInit {
    signer_kind: SignerKind,
    key_data: Bytes,
}
```

```rust
enum AccountSignerInit {
    Delegated(Address),
    External(ExternalSignerInit),
}
```

```rust
struct AccountInitParams {
    signers: Vec<AccountSignerInit>,
    threshold: Option<u32>,
    account_salt: BytesN<32>,
}
```

## 4. Public Interface

Required methods:

```rust
fn get_account_address(e: Env, params: AccountInitParams) -> Address
fn create_account(e: Env, params: AccountInitParams) -> Address
```

Recommended read helpers:

```rust
fn get_verifier(e: Env, signer_kind: SignerKind) -> Address
fn get_threshold_policy(e: Env) -> Address
```

## 4.1 Account Multiplicity

Latch v1 supports multiple smart accounts for the same signer set.

This is achieved with an explicit `account_salt` field in `AccountInitParams`.

Rules:

- the same signer set may create many accounts by choosing different `account_salt` values
- the same signer set plus the same `account_salt` must always resolve to the same account address
- account multiplicity must not depend on caller identity, relayer identity, or fee payer identity

This means Latch does not follow a strict one-key-one-account model like Braavos standard account deployment. Instead, it follows a deterministic many-accounts-per-signer model, closer to the role CREATE2 salts play in Argent-style deployment flows.

## 5. Configuration

The factory stores immutable wasm hashes for:

- `smart-account`
- `ed25519-verifier`
- `webauthn-verifier`
- `threshold-policy`

Example config shape:

```rust
struct FactoryConfig {
    smart_account_wasm_hash: BytesN<32>,
    ed25519_verifier_wasm_hash: BytesN<32>,
    webauthn_verifier_wasm_hash: BytesN<32>,
    threshold_policy_wasm_hash: BytesN<32>,
}
```

Rules:

- config is set once at construction
- no admin update methods in v1
- new code versions require deploying a new factory

## 6. Validation Rules

### 6.1 Signer Count

- `signers.len() >= 1`
- zero signers must be rejected

### 6.2 Duplicate Signers

Two signers are duplicates if:

- both are delegated signers with the same `Address`
- or both are external signers with the same:
  - `signer_kind`
  - `key_data`

Duplicates must be rejected.

### 6.3 Threshold Rules

Let `n = signers.len()`.

- if `n == 1`:
  - `threshold` may be `None` or `Some(1)`
  - any other value must be rejected
  - no threshold policy is installed

- if `n > 1`:
  - `threshold` is required explicitly
  - `threshold >= 1`
  - `threshold <= n`
  - otherwise reject

Important decision:

- multisig must require explicit threshold
- factory must not default multisig to `1-of-N`

### 6.4 Account Salt Rules

- `account_salt` is required
- `account_salt` is the explicit multiplicity input for account creation
- reusing the same `account_salt` with the same logical account configuration must produce the same address
- using a different `account_salt` with the same logical account configuration must produce a different address

Important implication:

- `account_salt` must not be treated as a UI label
- if a product wants nicknames, emojis, or themes, those should be metadata layered on top of the account, not the deterministic deployment salt

## 7. Structural Validation

The factory performs shape validation only.
It does not perform cryptographic verification.

### 7.1 Delegated

Expected input:

- a valid Soroban `Address`

Factory checks:

- no additional shape validation is required beyond `Address` decoding

### 7.2 Ed25519

Expected `key_data`:

- exactly 32 bytes public key

Factory checks:

- `key_data.len() == 32`

### 7.3 WebAuthn

Expected `key_data` format:

- first 65 bytes: uncompressed P-256 public key
- remaining bytes: credential ID

Factory checks:

- `key_data.len() > 65`
- first byte is `0x04`

The verifier is responsible for actual signature and assertion verification.

## 8. Canonicalization Rules

Canonicalization is required before:

- duplicate detection
- salt derivation
- address derivation
- deployment

### 8.1 Canonical Signer Identity

Each signer is identified by:

- delegated signer:
  - `Address`
- external signer:
  - `signer_kind`
  - `key_data`

### 8.2 Canonical Ordering

Signers must be sorted by:

1. signer-family code
2. for delegated signers: `Address` ordering
3. for external signers:
   - `signer_kind` code
   - lexicographic order of `key_data`

This ensures input order does not affect account address.

Example:

- `[External(WebAuthn(B)), External(Ed25519(A))]`
- `[External(Ed25519(A)), External(WebAuthn(B))]`

must resolve to the same canonical signer set.

## 9. Address Derivation

The smart-account address must be derived from canonical account parameters, not caller identity.

### 9.1 Salt Input

Salt preimage must include:

- version tag
- canonical signer count
- each canonical signer's:
  - signer family code
  - signer data length
  - signer data
- effective threshold
- `account_salt`

Recommended version tag:

```text
latch.factory.account.v1
```

### 9.1.1 Deterministic Encoding Formula

The factory must derive a deterministic deployment salt from normalized account parameters.

Conceptually:

```text
LatchAccountSaltV1 =
H(
  version_tag ||
  account_salt ||
  signer_count ||
  signer_1_code || signer_1_data_len || signer_1_data ||
  signer_2_code || signer_2_data_len || signer_2_data ||
  ...
  signer_n_code || signer_n_data_len || signer_n_data ||
  effective_threshold
)
```

Where:

- `H` is the contract's chosen 32-byte hashing function for deployment salt derivation
- `version_tag` is the fixed string `latch.factory.account.v1`
- `account_salt` is the explicit multiplicity input provided by the caller
- `signer_count` is the number of canonical signers
- `signer_i_code` is the canonical encoded signer-family code
- `signer_i_data_len` is the byte length of `signer_i_data`
- `signer_i_data` is:
  - for delegated signers: `Address::to_xdr(env)`
  - for external signers: raw `key_data`
- `effective_threshold` is the normalized threshold value used by the account

The exact serialization implementation must preserve unambiguous boundaries between fields.
This can be achieved by fixed-width fields where appropriate and explicit length-prefixing for variable-length fields.

### 9.1.2 Signer Kind Encoding

The signer kind encoding used in the salt preimage must be stable and versioned.

Recommended v1 mapping:

- `Delegated(Address) = 0x00`
- `Ed25519 = 0x01`
- `WebAuthn = 0x02`

These encoded values are part of the deterministic address-derivation surface and must not be changed within v1.

### 9.1.3 Deployment Address Model

The smart-account deployment address is derived from:

- the factory contract as deployer
- the deterministic account deployment salt
- the smart-account wasm hash

Conceptually:

```text
SmartAccountAddressV1 =
DeployAddress(
  deployer = factory_address,
  salt = LatchAccountSaltV1,
  wasm_hash = smart_account_wasm_hash
)
```

Important implications:

- caller identity must not change the derived address
- relayer identity must not change the derived address
- fee payer identity must not change the derived address
- only canonical account parameters and `account_salt` may change the derived address

### 9.2 Effective Threshold Encoding

For address derivation:

- single signer with omitted threshold and single signer with explicit `1` must produce the same address
- therefore the salt must use the normalized effective threshold, not the raw optional field

### 9.3 Determinism Guarantee

The following must always hold:

- same logical signer set + same threshold + same `account_salt` => same account address
- different signer set or different threshold or different `account_salt` => different account address

## 10. Shared Singleton Contracts

The factory may lazily deploy these shared contracts:

- `ed25519-verifier`
- `webauthn-verifier`
- `threshold-policy`

These are singleton contracts scoped to the factory version.

### 10.1 Singleton Salt Names

Recommended deterministic salts:

- `latch.factory.verifier.ed25519.v1`
- `latch.factory.verifier.webauthn.v1`
- `latch.factory.policy.threshold.v1`

### 10.2 Singleton Behavior

For each singleton:

- compute deterministic address
- if already deployed, reuse it
- if not deployed, deploy it
- never rely on mutable registry state for correctness

## 11. Smart-Account Constructor Translation

The factory translates `AccountInitParams` into smart-account constructor inputs:

```rust
__constructor(signers: Vec<Signer>, policies: Map<Address, Val>)
```

### 11.1 Signer Translation

Each signer input becomes:

- delegated signer:

```rust
Signer::Delegated(address)
```

- external signer:

```rust
Signer::External(verifier_address, key_data)
```

Where `verifier_address` is the shared singleton for that signer kind.

### 11.2 Policy Translation

- single-signer account:
  - policies map is empty

- multisig account:
  - policies map includes threshold policy address
  - install params encode explicit threshold

## 12. Deployment Algorithm

### 12.1 `get_account_address`

1. validate `params`
2. structurally validate signer inputs
3. canonicalize signer list
4. normalize effective threshold
5. compute account salt
6. return deterministic future address
7. no state changes

### 12.2 `create_account`

1. validate `params`
2. structurally validate signer inputs
3. canonicalize signer list
4. normalize effective threshold
5. derive target account address
6. if account already exists, return it
7. ensure required verifier singleton(s) exist
8. if multisig, ensure threshold policy exists
9. build smart-account constructor args
10. deploy smart account atomically
11. return deployed address

## 13. Idempotency Guarantees

The factory must be idempotent.

For the same logical `AccountInitParams`:

- repeated `get_account_address` calls return the same address
- repeated `create_account` calls return the same address
- if the account already exists, `create_account` returns it without redeploying

The same rule applies to singleton verifier and policy contracts.

More concretely:

- same signer set + same threshold + same `account_salt` => same address
- same signer set + same threshold + different `account_salt` => different address

## 14. Failure Cases

The factory must reject:

- zero signers
- duplicate signers
- invalid threshold
- missing explicit threshold for multisig
- malformed `key_data`
- malformed delegated signer input
- unknown signer kind
- missing config wasm hash
- contract deployment failure

## 15. Security Invariants

The implementation must preserve these invariants:

- caller identity must not influence account address derivation
- canonical signer ordering must eliminate input-order variance
- malformed signer input must fail before deployment
- account multiplicity must be controlled only by explicit `account_salt`
- verifier contracts must be shared and stateless
- threshold policy must only be installed for multisig
- account creation must be atomic
- no private keys are ever stored in factory state
- no private keys are ever passed as account constructor state

## 16. Worked Examples

### 16.1 Single Ed25519 Account

Input:

```rust
signers = [
  External({ signer_kind: Ed25519, key_data: <32-byte pubkey> })
]
threshold = None
account_salt = <32-byte salt A>
```

Behavior:

- valid
- no threshold policy
- deploy/reuse `ed25519-verifier`
- deploy smart account with one external signer

### 16.2 Single Delegated Account

Input:

```rust
signers = [
  Delegated(<G-address-backed Address>)
]
threshold = None
account_salt = <32-byte salt B>
```

Behavior:

- valid
- no threshold policy
- deploy smart account with one delegated signer

### 16.3 2-of-3 Mixed Multisig

Input:

```rust
signers = [
  Delegated(<G-address-backed Address A>),
  External({ signer_kind: Ed25519, key_data: <32-byte pubkey B> }),
  External({ signer_kind: WebAuthn, key_data: <65-byte p256 pubkey + credential id> }),
]
threshold = Some(2)
account_salt = <32-byte salt C>
```

Behavior:

- valid
- canonicalize signer order
- deploy/reuse required verifier singletons
- deploy/reuse threshold policy singleton
- deploy smart account with mixed delegated and external signers
- install threshold policy with `threshold = 2`

### 16.4 Invalid Multisig Without Threshold

Input:

```rust
signers = [
  Delegated(<G-address-backed Address A>),
  External({ signer_kind: Ed25519, key_data: <pubkey B> }),
]
threshold = None
account_salt = <32-byte salt D>
```

Behavior:

- reject
- multisig requires explicit threshold

## 17. Product Guidance for Salts

The deterministic `account_salt` should be treated as protocol input, not presentation metadata.

Recommended product behavior:

- generate a high-entropy random `account_salt` for normal account creation
- persist the salt client-side or server-side if users need to recreate or precompute the address
- allow advanced users or SDK consumers to provide a custom salt explicitly
- keep nicknames, emojis, themes, and labels separate from the salt

If a user reuses the same `account_salt` with the same canonical signer set and threshold:

- `get_account_address` must return the same address
- `create_account` must return the same existing account

This is idempotency, not an error.

If a product shows the same nickname or emoji but different account addresses, that strongly suggests the nickname or emoji is not the real deployment salt. It is more likely:

- display-only metadata
- or a human-friendly alias over a hidden random/internal salt

That is an inference from deterministic deployment behavior and the reference factory patterns, not a claim about Argent app internals.
