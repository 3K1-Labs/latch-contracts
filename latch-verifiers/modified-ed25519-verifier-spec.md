# Modified Ed25519 Verifier Spec

A stateless singleton Soroban contract that verifies Ed25519 signatures produced by Phantom wallet on behalf of Latch smart accounts. Deployed once, shared across all accounts on the network.

## Why This Verifier Exists

Stellar uses Ed25519 as its native signature curve and the Soroban host exposes `e.crypto().ed25519_verify()` as a builtin. In theory, a verifier that calls that builtin over the raw 32-byte auth payload hash is sufficient.

In practice, **Phantom blocks it.**

Phantom is a Solana wallet. Solana also uses Ed25519, and on Solana a transaction is identified by a 32-byte hash. Phantom's `signMessage` API rejects any payload that is exactly 32 bytes because it is indistinguishable from a Solana transaction hash — a user could unknowingly sign a real Solana transaction disguised as a message signing request. This is an intentional anti-phishing protection in the wallet.

The Soroban auth payload hash is exactly 32 bytes. Calling `provider.signMessage(auth_payload_hash)` from Phantom will be rejected at the wallet level before it ever reaches the chain.

### The Latch Signing Convention

The fix is a prefix. Instead of signing the raw 32-byte hash, the client constructs a human-readable string:

```
"Stellar Smart Account Auth:\n" + hex(auth_payload_hash)
```

This produces a 92-byte message (28-byte prefix + 64-byte lowercase hex encoding of the hash). Phantom accepts it because it no longer looks like a raw transaction hash.

This contract is the on-chain counterpart. It receives the 32-byte auth payload hash from the Soroban host, reconstructs the expected 92-byte signed message internally, and verifies the Ed25519 signature against it.

This prefix format is the **Latch Ed25519 signing convention for Phantom**. It is not a workaround — it is the defined protocol between the client and this verifier. Any client integrating with this verifier must produce signatures in this format.

### Constraint Scope

This constraint is Phantom-specific. It does not apply to:
- Any signer that can sign arbitrary byte payloads (hardware keys, custom signers)
- `Signer::Delegated` — the Soroban host validates native Stellar keypair signatures directly, no verifier call occurs
- WebAuthn — the challenge is base64url-encoded inside `clientDataJSON`, not sent as raw bytes

A separate pure Ed25519 verifier (no prefix, raw hash) exists for signers without this restriction — see `ed25519-verifier`, a thin wrapper around the `stellar_accounts::verifiers::ed25519` library. Use that one for any signer that can sign arbitrary bytes directly; use this one only when the client is going through a signing interface with Phantom's specific constraint.

---

## Trait Compliance

The contract fully implements the OZ `Verifier` trait — all three methods. The prefix logic is an internal detail of `verify`. From the trait's perspective, the contract receives a hash and returns whether the provided signature is valid for that hash. How it defines "valid" (i.e., over a prefixed form of the hash) is the implementation's business, not the trait's.

```rust
pub trait Verifier {
    type KeyData: FromVal<Env, Val>;
    type SigData: FromVal<Env, Val>;

    fn verify(e: &Env, hash: Bytes, key_data: Self::KeyData, sig_data: Self::SigData) -> bool;
    fn canonicalize_key(e: &Env, key_data: Self::KeyData) -> Bytes;
    fn batch_canonicalize_key(e: &Env, key_data: Vec<Self::KeyData>) -> Vec<Bytes>;
}
```

`canonicalize_key` and `batch_canonicalize_key` are unaffected by the prefix convention — they operate on the 32-byte public key only.

---

## Contract Interface

```rust
fn verify(e: &Env, hash: Bytes, key_data: BytesN<32>, sig_data: BytesN<64>) -> bool

fn canonicalize_key(e: &Env, key_data: BytesN<32>) -> Bytes

fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<32>>) -> Vec<Bytes>
```

### `verify`

Receives the 32-byte auth payload hash from the Soroban host during `__check_auth`.

Internally:
1. Validates `hash` is exactly 32 bytes
2. Hex-encodes `hash` to produce a 64-byte lowercase hex string
3. Prepends `"Stellar Smart Account Auth:\n"` to produce the 92-byte signed message
4. Calls `e.crypto().ed25519_verify(key_data, prefixed_message, sig_data)`
5. Returns `true` if it does not panic

The host builtin panics with `Error(Crypto, InvalidInput)` on any invalid signature, wrong key, or malformed input. The function surfaces that panic — it does not catch it.

**The client must sign the prefixed message, not the raw hash.** A signature over the raw hash will fail verification.

### `canonicalize_key`

Returns the canonical byte representation of the Ed25519 public key.

Ed25519 public keys are 32-byte compressed Edwards curve points with a single canonical encoding. `BytesN<32>` already enforces the correct length at deserialization. This function converts the fixed-size key to `Bytes` and returns it unchanged.

```
canonicalize_key(key: BytesN<32>) -> Bytes::from_slice(key.to_array())
```

### `batch_canonicalize_key`

Canonicalizes a list of keys, preserving input order. Calls `canonicalize_key` for each entry and returns the results as `Vec<Bytes>`.

---

## Types

```rust
// Key data — 32-byte Ed25519 public key
type KeyData = BytesN<32>;

// Signature data — 64-byte Ed25519 signature over the prefixed message
type SigData = BytesN<64>;
```

The signature is a standard Ed25519 signature. The only thing that changes relative to a raw-hash verifier is what was signed — the prefixed message, not the hash directly.

---

## Signing Convention (Client Side)

```
AUTH_PREFIX     = "Stellar Smart Account Auth:\n"   // 28 bytes
auth_hash       = <32-byte Soroban auth payload hash>
hex_hash        = lowercase_hex(auth_hash)          // 64 bytes
signed_message  = AUTH_PREFIX + hex_hash            // 92 bytes total

signature = ed25519_sign(private_key, signed_message)
```

The client passes:
- `key_data`: the 32-byte Ed25519 public key
- `sig_data`: the 64-byte signature over `signed_message`

The verifier reconstructs `signed_message` from the `hash` it receives from the host and verifies the signature against it.

---

## Key Shape

| Property | Value |
|---|---|
| Key length | 32 bytes exactly |
| Encoding | Raw Edwards curve point (no prefix byte) |
| Canonical representations | 1 — no compressed vs. uncompressed ambiguity |

---

## Statelessness

The verifier holds no storage. No constructor is needed. Every call is a pure function of its inputs. Multiple accounts share the same deployed instance without any state collision risk.

---

## Workspace Location

```
latch-contracts/
└── latch-verifiers/
    └── modified-ed25519-verifier/
        └── src/
            ├── lib.rs
            └── test.rs
```

The `verify` implementation does not delegate to `stellar_accounts::verifiers::ed25519::verify` directly — that function verifies over the raw payload. The contract implements the prefix logic itself and calls `e.crypto().ed25519_verify` directly. `canonicalize_key` and `batch_canonicalize_key` do delegate to the OZ library functions.

---

## Implementation Skeleton

```rust
const AUTH_PREFIX: &[u8] = b"Stellar Smart Account Auth:\n";
const PREFIX_LEN: usize = 28;
const PAYLOAD_LEN: usize = 32;
const HEX_LEN: usize = 64;
const TOTAL_LEN: usize = PREFIX_LEN + HEX_LEN; // 92

#[contract]
pub struct ModifiedEd25519Verifier;

#[contractimpl]
impl Verifier for ModifiedEd25519Verifier {
    type KeyData = BytesN<32>;
    type SigData = BytesN<64>;

    fn verify(e: &Env, hash: Bytes, key_data: BytesN<32>, sig_data: BytesN<64>) -> bool {
        // Reconstruct the prefixed message from the hash
        // hex-encode hash, prepend AUTH_PREFIX, verify signature over result
        ...
        e.crypto().ed25519_verify(&key_data, &prefixed_message, &sig_data);
        true
    }

    fn canonicalize_key(e: &Env, key_data: BytesN<32>) -> Bytes {
        stellar_accounts::verifiers::ed25519::canonicalize_key(e, &key_data)
    }

    fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<32>>) -> Vec<Bytes> {
        stellar_accounts::verifiers::ed25519::batch_canonicalize_key(e, &key_data)
    }
}
```

---

## Test Plan

### `verify` — valid Phantom-format signature
- Generate an Ed25519 keypair
- Produce a 32-byte payload hash
- Construct `"Stellar Smart Account Auth:\n" + hex(hash)` and sign it
- Call `verify(hash, pub_key, signature)` — must return `true`

### `verify` — raw hash signature rejected
- Sign the raw 32-byte hash directly (no prefix)
- Call `verify(hash, pub_key, signature)` — must panic with `Error(Crypto, InvalidInput)`
- This confirms the contract enforces the prefix convention

### `verify` — wrong prefix rejected
- Sign `"Wrong Prefix:\n" + hex(hash)`
- Call `verify` — must panic with `Error(Crypto, InvalidInput)`

### `verify` — invalid signature
- Valid keypair and correct prefix format
- Corrupt one byte of the signature
- Must panic with `Error(Crypto, InvalidInput)`

### `verify` — wrong key
- Sign with key A using correct prefix format
- Call `verify` with key B's public key
- Must panic with `Error(Crypto, InvalidInput)`

### `verify` — wrong payload
- Sign prefix + hex(hash_X) with correct key
- Call `verify(hash_Y, ...)` where hash_Y != hash_X
- Must panic with `Error(Crypto, InvalidInput)`

### `canonicalize_key` — identity
- Pass a 32-byte key
- Returned `Bytes` must equal the same 32 bytes
- Length must be 32

### `batch_canonicalize_key` — preserves order
- Pass three keys, verify output order matches input

---

## What This Is Not

- Not a general-purpose Ed25519 verifier. It only verifies signatures in the Latch/Phantom prefix format. A signature over a raw hash will always fail.
- Not coupled to Phantom beyond the signing convention. Any client that produces `sign(key, "Stellar Smart Account Auth:\n" + hex(hash))` will work, regardless of what wallet produced the signature.
- Not responsible for replay protection. The Soroban auth framework handles that at the host level.
- Not a replacement for `Signer::Delegated`. Users migrating from G-addresses can use the delegated path — no verifier needed.
