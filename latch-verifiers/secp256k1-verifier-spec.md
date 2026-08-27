# Secp256k1 Verifier Spec

> **Status: Not started, not blocked.** Build fresh from this spec, matching the structure of
> `ed25519-verifier` (see that crate's source for the pattern: a thin `#[contract]` wrapper,
> `Verifier` trait impl, `src/test.rs` with adversarial negative tests).

## What this verifier is for

This lets a secp256k1 keypair — the curve MetaMask, other EVM wallets, and EVM chains themselves
already use to derive their addresses — control a Stellar smart account. The verifier checks a
signature over the standard 32-byte Soroban auth payload hash, same as any other verifier in this
repo. It is **not** specific to MetaMask, EIP-191, or any particular wallet's signing popup — it's
a curve-level verifier, exactly the same relationship `ed25519-verifier` has to the Ed25519 curve.

---

## Architecture Decisions

### 1. Signing convention — raw hash, no wrapping (decided)

Sign the raw 32-byte auth payload hash directly, the same convention `ed25519-verifier` uses.
Nothing about the secp256k1 curve or ECDSA itself requires a message wrapper — that's not a
cryptographic property of the curve.

Don't build in an EIP-191 (`"\x19Ethereum Signed Message:\n" + len + message`) wrapper by default.
That convention only matters if a client is specifically going through a wallet's `personal_sign`
popup — which is a client-integration detail, not something this verifier should assume or bake
in. `demo/modified-ed25519-verifier` is a worked reference for what that kind of wrapping looks
like *if* it's ever needed — it exists because a one-off demo needed to work around Phantom's
signing popup refusing to sign bare 32-byte payloads, not because every wallet-popup integration
needs this by default. If a real MetaMask-popup integration is attempted later and hits the same
kind of constraint, that would justify a separate variant built the same way — don't design this
one around a problem that hasn't been confirmed to exist yet.

### 2. Verify vs recover — resolved: recovery is the only path

The Soroban host exposes `e.crypto().secp256k1_recover()` — there is no `secp256k1_verify()`. This
isn't a choice: recovery is the only mechanism available.

```
recovered_pub_key = e.crypto().secp256k1_recover(&message_hash, &signature, recovery_id)
```

The verifier must derive `recovered_pub_key` and compare it against the registered `key_data`. The
`recovery_id` has to be extracted from the signature's 65th byte, alongside the 64-byte `r || s`.
Confirm what encoding the client-side signing library actually produces for `recovery_id` (raw 0/1
is the ECDSA-native form; some libraries offset it) before locking the format.

**Why recover-and-compare is real verification, not a weaker substitute.** ECDSA's `(r, s)`
signature mathematically under-determines the public key on its own — there are exactly two curve
points that could have produced a given `r`, and `recovery_id` is the extra bit that picks which
one. Given `(hash, r, s, recovery_id)` alone, recovery reconstructs one specific candidate public
key with no pubkey input needed at all — a structural property of ECDSA, not a Soroban limitation.
Comparing that candidate against the registered `key_data` is mathematically equivalent to a direct
`verify(pubkey, hash, sig) -> bool`: an attacker who doesn't hold the private key for `key_data`
cannot construct any `(r, s, recovery_id)` that recovers to *that specific* key without solving the
discrete log problem, i.e. breaking ECDSA itself. This isn't a Soroban-specific workaround either —
it's exactly how Ethereum's `ecrecover` precompile works; EVM tooling has never had a separate
ECDSA verify function, everything goes through recover-and-compare.

Ed25519 needs a real `verify()` for the opposite structural reason: Ed25519 signatures don't
support public-key recovery at all — the scheme's construction doesn't leave enough information in
`(R, s)` alone to reconstruct the signer's key, so the pubkey has to be supplied and checked
directly. That's a difference between the two signature schemes, not an inconsistency between this
verifier and `ed25519-verifier`.

### 3. Key data format — 65-byte uncompressed pubkey (decided)

Considered and rejected: a 20-byte Ethereum-style address (`keccak256(pubkey)[12:]`), recovering
the pubkey then re-deriving and comparing the address instead. Rejected because every other
verifier in this repo (`ed25519-verifier`, `webauthn-verifier`) stores and canonicalizes the actual
public key material, not a derived hash/address — an address-based model for just this one verifier
breaks that symmetry for no real benefit, and throws away key material a future scheme (aggregate
signatures, cross-verifier dedup) might want. Deriving a familiar `0x...` address for display
purposes belongs in the client/SDK layer, not the on-chain `key_data` representation.

Decided: **65-byte uncompressed pubkey** (`0x04` + 32-byte X + 32-byte Y) as `key_data`.

### 4. Reference implementation — none exists

No secp256k1 verifier exists anywhere in the codebase to crib from, and OZ's `stellar-accounts`
library does not ship one (`stellar_accounts::verifiers` only has `ed25519` and `webauthn`). This
is genuinely from-scratch work, using `e.crypto().secp256k1_recover()` directly.

---

## Contract Interface (proposed)

```rust
fn verify(e: &Env, hash: Bytes, key_data: BytesN<65>, sig_data: BytesN<65>) -> bool

fn canonicalize_key(e: &Env, key_data: BytesN<65>) -> Bytes

fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<65>>) -> Vec<Bytes>
```

`sig_data` is 65 bytes, not 64 — `r || s || recovery_id`, since recovery (unlike plain verification)
needs the recovery id to know which of the (usually two) candidate public keys the signature
actually corresponds to.

### `verify`

1. Validate `hash` is 32 bytes and `key_data` is a well-formed 65-byte uncompressed point
   (`0x04` prefix).
2. Call `e.crypto().secp256k1_recover(&hash, &sig_r_s, recovery_id)` directly over the raw hash —
   no message reconstruction step (see §1).
3. Compare the recovered pubkey to `key_data`. Panic with a dedicated error variant
   (`Secp256k1VerifierError::KeyMismatch` or similar) on mismatch — not a bare `false` return, to
   stay consistent with how the other verifiers surface failure via panic.

### `canonicalize_key`

Pass-through, matching `ed25519-verifier`'s and `webauthn-verifier`'s approach — the 65-byte
uncompressed form is already canonical, no compressed-key normalization needed unless client
libraries are found to produce compressed (33-byte) keys in practice, in which case
`canonicalize_key` should decompress to the uncompressed form before returning it, so two
registrations of the same key in different encodings dedupe correctly.

---

## Types

```rust
// Key data — 65-byte uncompressed secp256k1 public key (0x04 + 32-byte X + 32-byte Y)
type KeyData = BytesN<65>;

// Signature data — 65-byte recoverable ECDSA signature (r || s || recovery_id)
type SigData = BytesN<65>;
```

---

## Test Plan

Same adversarial shape as `ed25519-verifier`'s and `webauthn-verifier`'s test suites — don't ship
with only a happy-path test:

- `verify` — valid signature, correct recovery id → succeeds
- `verify` — wrong recovery id (flips which candidate key gets recovered) → rejected
- `verify` — signed with key A, `key_data` is key B → rejected
- `verify` — signature over a different hash than the one passed to `verify` → rejected
- `verify` — corrupted `r`/`s` bytes → rejected
- `verify` — malformed `key_data` (wrong length, non-`0x04` prefix) → rejected
- `canonicalize_key` — identity for an already-uncompressed key
- `canonicalize_key` — compressed-key normalization, if implemented (see §`canonicalize_key` above)
- `batch_canonicalize_key` — preserves order, single-element case matches `canonicalize_key`

---

## Dependencies on Other Work

None remaining — the sequencing gate (Ed25519 + WebAuthn verifiers shipping first) has passed.
Nothing in this spec depends on confirming any particular wallet's popup behavior; it's buildable
now.

## What This Is Not

- Not wired into the factory.
- Not a MetaMask-specific verifier. It verifies secp256k1 signatures over the raw hash — the same
  curve MetaMask and other EVM wallets happen to use, not an integration with any particular
  wallet's signing flow. A wallet-popup-constrained variant, if one turns out to be needed, is a
  separate future concern (see §1) — don't conflate the two.
- Not responsible for replay protection — the Soroban auth framework handles that at the host level.
