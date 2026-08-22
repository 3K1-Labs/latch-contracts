# Secp256k1 Verifier Spec

> **Status: Not started, but no longer blocked.** The Ed25519 verifiers (`ed25519-verifier`,
> `modified-ed25519-verifier`) and `webauthn-verifier` are all shipped — the sequencing that
> deferred this one no longer applies. This spec now includes concrete recommendations, not just
> open questions, for whoever picks it up. The stub crate that previously existed
> (`secp256k1-verifier`) has been deleted from the repo — start fresh from this spec, matching the
> structure of `ed25519-verifier`/`modified-ed25519-verifier` (see those crates' source for the
> pattern: a thin `#[contract]` wrapper, `Verifier` trait impl, `src/test.rs` with adversarial
> negative tests).

The secp256k1 verifier is the MetaMask / EVM-wallet signing path.

## Not currently wired to the factory

Earlier revisions of this spec assumed the factory would install this verifier automatically
(`SignerKind::Secp256k1`). That's no longer true — `Secp256k1` was removed from the factory's
`SignerKind` entirely (it was unused: confirmed via both `latch-web-extension` and `latch-mobile`
audits). Building this verifier is now purely standalone infrastructure, usable by any smart
account via `Signer::External(verifier_address, key_data)` regardless of what the factory's
convenience enum supports. Re-adding a factory-level `SignerKind` for it, if ever wanted, is a
separate, later decision — don't couple this implementation to it.

---

## Open Architecture Questions — with recommendations

These still need a maintainer to confirm before implementation starts, but each now has a default
recommendation instead of being left fully open.

### 1. Signing convention — recommend hex-encoded hash, mirroring the Ed25519 Phantom pattern

MetaMask's `personal_sign` uses EIP-191:

```
"\x19Ethereum Signed Message:\n" + len(message) + message
```

Similar problem to Phantom — MetaMask wraps the message before signing, and the on-chain verifier
must reconstruct the identical wrapper to check the signature. **Recommendation**: reuse the exact
convention `modified-ed25519-verifier` already established for the same reason (Phantom rejects raw
32-byte payloads; MetaMask's UI would render raw bytes as unreadable garbage, which is bad UX and
looks suspicious to a user deciding whether to sign):

```
message   = "Stellar Smart Account Auth:\n" + lowercase_hex(auth_payload_hash)   // 92 bytes
eip191_msg = "\x19Ethereum Signed Message:\n" + len(message) + message
signature  = personal_sign(eip191_msg)  // MetaMask wraps this automatically — the *dApp* only
                                          // constructs `message`, not the outer EIP-191 wrapper
```

This keeps one shared client-side signing convention across every Latch verifier that has to defend
against a wallet-popup constraint, instead of inventing a second bespoke format. Confirm against
MetaMask's actual `personal_sign` behavior before finalizing — the EIP-191 wrapper itself is
usually applied by the wallet, not the dApp, so the contract only needs to reconstruct `message`
and the wrapper around it, matching whatever `eth_sign`/`personal_sign` actually produces on the
target client.

### 2. Verify vs recover — resolved: recovery is the only path

The Soroban host exposes `e.crypto().secp256k1_recover()` — there is no `secp256k1_verify()`. This
isn't a choice: recovery is the only mechanism available.

```
recovered_pub_key = e.crypto().secp256k1_recover(&message_hash, &signature, recovery_id)
```

The verifier must derive `recovered_pub_key` and compare it against the registered `key_data`. The
`recovery_id` (0/1, sometimes encoded as 27/28 or with an EIP-155 chain-id offset — MetaMask's
exact encoding needs confirming) has to be extracted from the signature's 65th byte, alongside the
64-byte `r || s`.

### 3. Key data format — recommend keeping the 65-byte uncompressed pubkey

Two options:

- **65-byte uncompressed pubkey** (`0x04` + 32-byte X + 32-byte Y) as `key_data` — what the
  original factory-era spec assumed, before the factory decoupling above.
- **20-byte Ethereum address** (`keccak256(pubkey)[12:]`), recovering the pubkey then re-deriving
  and comparing the address instead.

**Recommendation: keep the 65-byte uncompressed pubkey.** Every other verifier in this repo
(`ed25519-verifier`, `modified-ed25519-verifier`, `webauthn-verifier`) stores and canonicalizes the
actual public key material, not a derived hash/address — switching this one verifier to an
address-based model breaks that symmetry for no real benefit, and throws away key material a future
scheme (aggregate signatures, cross-verifier dedup) might want. The extra `keccak256` + truncate
step to render a familiar `0x...` address for display purposes belongs in the client/SDK layer, not
the on-chain `key_data` representation.

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
2. Reconstruct the EIP-191-wrapped message exactly as the client signed it (see §1).
3. Hash the wrapped message with keccak256 (Ethereum's convention, not SHA-256 — confirm the
   Soroban host exposes this, or whether it needs to be computed manually).
4. Call `e.crypto().secp256k1_recover(&message_hash, &sig_r_s, recovery_id)`.
5. Compare the recovered pubkey to `key_data`. Panic with a dedicated error variant
   (`Secp256k1VerifierError::KeyMismatch` or similar) on mismatch — not a bare `false` return, to
   stay consistent with how the other verifiers surface failure via panic.

### `canonicalize_key`

Pass-through, matching `ed25519-verifier`'s and `webauthn-verifier`'s approach — the 65-byte
uncompressed form is already canonical, no compressed-key normalization needed unless client
libraries are found to produce compressed (33-byte) keys in practice, in which case
`canonicalize_key` should decompress to the uncompressed form before returning it, so two
registrations of the same key in different encodings dedupe correctly (mirrors why
`webauthn-verifier::canonicalize_key` strips the credential-ID suffix).

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

Same adversarial shape as `modified-ed25519-verifier`'s and `webauthn-verifier`'s test suites —
don't ship with only a happy-path test:

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

None remaining — the sequencing gate (Ed25519 + WebAuthn verifiers shipping first) has passed. The
one real dependency left is confirming §1's signing convention against MetaMask's actual
`personal_sign` output on a real testnet transaction before locking the format — everything else in
this spec is buildable now.

## What This Is Not

- Not wired into the factory. See "Not currently wired to the factory" above.
- Not a general ECDSA verifier — specific to the secp256k1 curve and Ethereum's `personal_sign`/
  EIP-191 convention. A raw, unwrapped secp256k1 signature (no EIP-191 envelope) would need its own
  variant, same relationship `ed25519-verifier` has to `modified-ed25519-verifier`.
- Not responsible for replay protection — the Soroban auth framework handles that at the host level.
