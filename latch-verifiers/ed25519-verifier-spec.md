# Ed25519 Verifier Spec

A stateless singleton Soroban contract that verifies Ed25519 signatures over the raw 32-byte Soroban auth payload hash, on behalf of Latch smart accounts. Deployed once, shared across all accounts on the network.

## Why This Verifier Exists

Stellar uses Ed25519 as its native signature curve and the Soroban host exposes `e.crypto().ed25519_verify()` as a builtin. This verifier calls that builtin directly over the raw 32-byte auth payload hash — the client signs the hash as-is, no wrapping or additional encoding.

## Trait Compliance

Implements the OZ `Verifier` trait, delegating every method directly to `stellar_accounts::verifiers::ed25519`:

```rust
pub trait Verifier {
    type KeyData: FromVal<Env, Val>;
    type SigData: FromVal<Env, Val>;

    fn verify(e: &Env, hash: Bytes, key_data: Self::KeyData, sig_data: Self::SigData) -> bool;
    fn canonicalize_key(e: &Env, key_data: Self::KeyData) -> Bytes;
    fn batch_canonicalize_key(e: &Env, key_data: Vec<Self::KeyData>) -> Vec<Bytes>;
}
```

## Contract Interface

```rust
fn verify(e: &Env, hash: Bytes, key_data: BytesN<32>, sig_data: BytesN<64>) -> bool

fn canonicalize_key(e: &Env, key_data: BytesN<32>) -> Bytes

fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<32>>) -> Vec<Bytes>
```

### `verify`

Receives the 32-byte auth payload hash from the Soroban host during `__check_auth` and calls `stellar_accounts::verifiers::ed25519::verify(e, &hash, &key_data, &sig_data)`, which delegates straight to `e.crypto().ed25519_verify()`. Panics with `Error(Crypto, InvalidInput)` on any invalid signature, wrong key, or wrong payload — this function does not catch that panic.

**The client signs the raw 32-byte hash directly.** No prefix, no wrapping.

### `canonicalize_key`

Ed25519 public keys have exactly one canonical 32-byte encoding — this is a pass-through to `stellar_accounts::verifiers::ed25519::canonicalize_key`.

### `batch_canonicalize_key`

Canonicalizes a list of keys, preserving input order.

## Types

```rust
// Key data — 32-byte Ed25519 public key
type KeyData = BytesN<32>;

// Signature data — 64-byte Ed25519 signature over the raw hash
type SigData = BytesN<64>;
```

## Key Shape

| Property | Value |
|---|---|
| Key length | 32 bytes exactly |
| Encoding | Raw Edwards curve point (no prefix byte) |
| Canonical representations | 1 — no compressed vs. uncompressed ambiguity |

## Statelessness

The verifier holds no storage. No constructor is needed. Every call is a pure function of its inputs. Multiple accounts share the same deployed instance without any state collision risk.

## What This Is Not

- Not responsible for replay protection. The Soroban auth framework handles that at the host level.
- Not a replacement for `Signer::Delegated`. Users migrating from G-addresses can use the delegated path — no verifier needed.
