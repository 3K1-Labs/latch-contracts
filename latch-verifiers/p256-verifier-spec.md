# Raw P-256 Verifier Spec

> **Status: Not started, not blocked on a separate spec issue.** This spec was written directly
> by a maintainer rather than left as an open contributor task — canonical encoding/digest
> decisions for an authorization boundary are a maintainer call, same reasoning already applied to
> `secp256k1-verifier-spec.md`. Build from this spec, matching the structure of
> `ed25519-verifier`/`secp256k1-verifier`: thin `#[contract]` wrapper, `Verifier` trait impl,
> `src/test.rs` with adversarial negative tests.

## What this verifier is for

Part of the raw P-256 session-key initiative (issue #19): an ephemeral secp256r1 (P-256) signer
that a durable passkey authorizes once, via a scoped, expiring `CallContract` context rule plus the
existing session policy, so a session key can sign permitted actions **without another WebAuthn
ceremony**. This crate is the cryptographic primitive only — key generation, storage, and the
session-authorization UI are client-side work (`latch-web-extension`/`latch-mobile`), not this
crate's concern.

This is **not** a WebAuthn variant. `webauthn-verifier` already verifies secp256r1 signatures, but
wrapped in a full WebAuthn ceremony (`clientDataJSON`, `authenticatorData`, challenge binding, an
extra digest step — see below). This verifier signs and verifies the raw 32-byte Soroban auth
payload hash directly, the same convention `ed25519-verifier` and `secp256k1-verifier` use — no
ceremony, no browser API, suitable for a key that has to sign silently in the background.

## Architecture Decisions

### 1. Host function — `Env::crypto().secp256r1_verify`, a real verify, not recovery

Unlike secp256k1 (Soroban only exposes `secp256k1_recover`), Soroban exposes a direct verify
primitive for secp256r1:

```rust
pub fn secp256r1_verify(
    &self,
    public_key: &BytesN<65>,
    message_digest: &Hash<32>,
    signature: &BytesN<64>,
);
```

(`soroban-sdk` 26.1.0, `soroban_sdk::crypto::Crypto::secp256r1_verify` — confirmed by reading the
SDK source directly, `crypto.rs`.) It panics internally on an invalid signature, wrong key, or
malformed input — there is no recover-then-compare step to design, unlike
`secp256k1-verifier-spec.md`'s §2. `key_data` is a required, direct input, not something derived
from the signature.

### 2. Key data format — 65-byte uncompressed SEC1 point (decided, matches existing precedent)

`webauthn-verifier` already establishes this exact format in this repo for the same curve:
65 bytes, `0x04` prefix + 32-byte X + 32-byte Y — the same format `secp256r1_verify` requires as
input. This isn't a fresh design choice, it's what the host function and the existing verifier both
already use. No compressed-key (33-byte) support — reject anything else as malformed, same as
`webauthn-verifier` does.

### 3. Signature format — 64-byte compact `r || s`, low-S required (decided, matches existing precedent)

Also established by `webauthn-verifier` and directly required by `secp256r1_verify`'s `BytesN<64>`
signature parameter: raw compact form, not DER. **Low-S canonical form is required** — the host
function rejects high-S signatures (standard ECDSA malleability protection: `(r, s)` and
`(r, n - s)` both verify against the same key/message, so canonical form picks one). This is
already load-bearing in this repo: `webauthn-verifier/src/test.rs` explicitly calls
`normalize_s()` on its test signatures with the comment "ensures low-S form required by Stellar
`secp256r1_verify`." A client signing with a naive ECDSA library that doesn't normalize S will
produce signatures this verifier correctly rejects — document this plainly for client
implementers, it is the single most likely real-world integration bug.

### 4. What's signed — the raw 32-byte hash directly, no extra digest wrapping

`webauthn::verify` in OZ's own library calls
`e.crypto().secp256r1_verify(pub_key, &e.crypto().sha256(&message_digest), signature)` — it
re-hashes before verifying. That extra `sha256` is WebAuthn-specific (WebAuthn's own signed payload
is `authenticatorData || sha256(clientDataJSON)`, itself then digested), not a general
`secp256r1_verify` requirement. This verifier has no ceremony to construct a payload from — it
passes the 32-byte Soroban auth payload hash directly as `message_digest`, with no extra hashing
step, same "raw hash, no wrapping" decision already made for `ed25519-verifier` and
`secp256k1-verifier`.

### 5. Statelessness — stateless singleton, same as every other verifier here

No constructor, no storage, no upgrade entrypoint. `verify` is a pure function of its three inputs.
Deployed once, shared across every account that installs a raw P-256 signer, matching
`ed25519-verifier`/`webauthn-verifier`/`secp256k1-verifier`.

### 6. Malformed input vs. invalid signature

- `key_data` not exactly 65 bytes, or not `0x04`-prefixed → panic before calling the host function
  (malformed input, not a verification failure) — matches `webauthn-verifier`'s
  `extract_from_bytes` pattern.
- `sig_data` not exactly 64 bytes → panic before calling the host function.
- A well-formed 65-byte key + 64-byte signature that doesn't verify (wrong key, wrong digest,
  corrupted `r`/`s`, high-S) → `secp256r1_verify` panics internally; no separate error enum needed
  on this side, matching `ed25519-verifier`'s and `webauthn-verifier`'s convention of relying on the
  host's own panic rather than inventing a custom mismatch error (unlike
  `secp256k1-verifier-spec.md`'s recover-then-compare case, which genuinely needs one).

## Contract Interface (proposed)

```rust
fn verify(e: &Env, hash: Bytes, key_data: BytesN<65>, sig_data: BytesN<64>) -> bool

fn canonicalize_key(e: &Env, key_data: BytesN<65>) -> Bytes

fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<65>>) -> Vec<Bytes>
```

### `verify`

1. Validate `hash` is 32 bytes.
2. Call `e.crypto().secp256r1_verify(&key_data, &Hash::from_bytes(hash), &sig_data)` directly over
   the raw hash — no digest reconstruction (see §4).
3. Return `true` if the host call didn't panic. No manual comparison step (see §1) — the host
   function is the entire verification, unlike the recover-then-compare shape secp256k1 needs.

### `canonicalize_key`

Pass-through — the 65-byte uncompressed form is already canonical, matching
`webauthn-verifier`'s and `ed25519-verifier`'s approach. Unlike `webauthn-verifier`, there's no
credential-ID suffix to strip here — no WebAuthn ceremony means no credential ID exists in the
first place.

## Types

```rust
// Key data — 65-byte uncompressed secp256r1 public key (0x04 + 32-byte X + 32-byte Y)
type KeyData = BytesN<65>;

// Signature data — 64-byte compact ECDSA signature (r || s), low-S canonical form required
type SigData = BytesN<64>;
```

## Test Plan

Same adversarial shape as every other verifier's test suite here — don't ship with only a
happy-path test. Explicitly required by issue #21 as well:

- `verify` — valid signature, low-S form → succeeds
- `verify` — valid signature in high-S form (unnormalized) → rejected (confirms the host enforces
  canonical form; this is the case most likely to surprise a real client integration)
- `verify` — signed with key A, `key_data` is key B → rejected
- `verify` — signature over a different hash than the one passed to `verify` → rejected
- `verify` — corrupted `r`/`s` bytes → rejected
- `verify` — malformed `key_data` (wrong length, non-`0x04` prefix) → rejected
- `verify` — malformed `sig_data` (wrong length) → rejected
- `canonicalize_key` — identity for a well-formed key
- `batch_canonicalize_key` — preserves order, single-element case matches `canonicalize_key`
- Confirm no contract storage is written by any call (statelessness, see §5)

## What This Is Not

- Not a WebAuthn variant — no ceremony, no `clientDataJSON`/`authenticatorData`, no extra digest
  wrapping. See §4.
- Not wired into the factory — session keys are created post-account-creation via `add_context_rule`
  on an existing account, not at signup. See issue #19's target flow.
- Not responsible for replay protection, expiry, or revocation — those are the smart account's own
  `valid_until`/`remove_context_rule` machinery and the session policy, same separation of concerns
  `session-policy`'s own module doc already documents for the WebAuthn/Ed25519 case.
- Not responsible for client-side key generation, storage, or the session-authorization UI — see
  the client-side issues (`latch-web-extension`#41, `latch-mobile`#60).

## References

- `soroban-sdk` 26.1.0 `crypto.rs` — `Crypto::secp256r1_verify` exact signature.
- `latch-verifiers/webauthn-verifier/src/lib.rs` — existing 65-byte SEC1 key format and 64-byte
  compact signature format for this same curve in this repo.
- `latch-verifiers/webauthn-verifier/src/test.rs` — `normalize_s()` precedent for low-S signing.
- `references/stellar-contracts/packages/accounts/src/verifiers/webauthn.rs:356` — the extra
  `sha256` wrapping this verifier deliberately does *not* replicate (see §4).
- `latch-verifiers/secp256k1-verifier-spec.md` — sibling spec, same rigor, contrasting recover-based
  verification.
- Issue #19 — the initiative and target flow this verifier serves.
