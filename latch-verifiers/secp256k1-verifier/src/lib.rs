//! Contract for verifying secp256k1 (ECDSA) digital signatures.
//!
//! Verifies a recoverable ECDSA signature over the 32-byte Soroban auth
//! payload hash. Unlike Ed25519, the Soroban host exposes only
//! `secp256k1_recover()` — there is no separate `secp256k1_verify()`. The
//! verifier calls recover-and-compare: it recovers the public key from
//! `(hash, r‖s, recovery_id)` and asserts the result equals the registered
//! `key_data`. This is mathematically equivalent to direct verification and
//! is the same mechanism Ethereum's `ecrecover` precompile uses.
//!
//! ## Key format
//!
//! `key_data` is a 65-byte uncompressed SEC-1 public key: `0x04 ‖ X ‖ Y`.
//!
//! ## Signature format
//!
//! `sig_data` is 65 bytes: `r (32) ‖ s (32) ‖ recovery_id (1)`.
//! `recovery_id` is the raw ECDSA recovery bit — `0` or `1`.
//!
//! ## Signing convention
//!
//! Sign the raw 32-byte Soroban auth payload hash directly — no EIP-191 or
//! any other message wrapper. See `demo/modified-ed25519-verifier` for the
//! wrapping reference if a wallet-popup-constrained variant is ever needed.
//!
//! ## No upstream OZ module
//!
//! `stellar_accounts::verifiers` does not ship a secp256k1 module. All
//! verification logic is implemented here directly, using
//! [`e.crypto_hazmat().secp256k1_recover()`][soroban_sdk::Env::crypto_hazmat]
//! from the Soroban SDK.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, Bytes, BytesN, Env, Vec,
};
use stellar_accounts::verifiers::Verifier;

// ── error type ───────────────────────────────────────────────────────────────

/// Error codes for the secp256k1 verifier contract.
#[contracterror]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Secp256k1VerifierError {
    /// The `hash` argument is not exactly 32 bytes.
    InvalidHashLength = 1,
    /// `key_data` does not start with the `0x04` uncompressed-point prefix.
    InvalidKeyPrefix = 2,
    /// The recovered public key does not match `key_data`.
    KeyMismatch = 3,
    /// `recovery_id` byte in `sig_data` is not 0 or 1.
    InvalidRecoveryId = 4,
}

// ── contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct Secp256k1Verifier;

#[contractimpl]
impl Verifier for Secp256k1Verifier {
    /// 65-byte uncompressed secp256k1 public key (`0x04` ‖ X ‖ Y).
    type KeyData = BytesN<65>;
    /// 65-byte recoverable ECDSA signature (`r` ‖ `s` ‖ `recovery_id`).
    type SigData = BytesN<65>;

    /// Verify a secp256k1 ECDSA signature over the raw 32-byte auth payload
    /// hash.
    ///
    /// # Arguments
    ///
    /// * `hash` — The 32-byte Soroban auth payload hash. Must be exactly 32
    ///   bytes; panics with [`Secp256k1VerifierError::InvalidHashLength`]
    ///   otherwise.
    /// * `key_data` — 65-byte uncompressed public key (`0x04` prefix + 32-byte
    ///   X + 32-byte Y). Panics with [`Secp256k1VerifierError::InvalidKeyPrefix`]
    ///   if the first byte is not `0x04`.
    /// * `sig_data` — 65-byte recoverable ECDSA signature: `r (32) ‖ s (32)
    ///   ‖ recovery_id (1)`. `recovery_id` must be `0` or `1`; panics with
    ///   [`Secp256k1VerifierError::InvalidRecoveryId`] otherwise.
    ///
    /// # Returns
    ///
    /// `true` if recovery succeeds and the recovered key matches `key_data`.
    /// Panics with [`Secp256k1VerifierError::KeyMismatch`] on mismatch.
    fn verify(e: &Env, hash: Bytes, key_data: BytesN<65>, sig_data: BytesN<65>) -> bool {
        // 1. Validate hash length.
        if hash.len() != 32 {
            panic_with_error!(e, Secp256k1VerifierError::InvalidHashLength);
        }

        // 2. Validate key prefix (`0x04` = uncompressed).
        if key_data.get(0).unwrap() != 0x04 {
            panic_with_error!(e, Secp256k1VerifierError::InvalidKeyPrefix);
        }

        // 3. Extract r‖s (first 64 bytes) and recovery_id (65th byte).
        //    `BytesN` has no `.slice()` — go through `Bytes` for the slice,
        //    then convert back via `TryInto`.
        let sig_as_bytes: Bytes = sig_data.into();
        let r_s: BytesN<64> = sig_as_bytes
            .slice(0..64)
            .try_into()
            .unwrap();
        let recovery_id = sig_as_bytes.get(64).unwrap() as u32;
        if recovery_id > 1 {
            panic_with_error!(e, Secp256k1VerifierError::InvalidRecoveryId);
        }

        // 4. Convert the variable-length `hash: Bytes` to `BytesN<32>` for
        //    `CryptoHazmat`. Length was already validated in step 1.
        let hash_fixed: BytesN<32> = hash.try_into().unwrap();

        // 5. Recover the public key via `e.crypto_hazmat()` (requires the
        //    `hazmat-crypto` feature on soroban-sdk, declared in Cargo.toml).
        let recovered = e
            .crypto_hazmat()
            .secp256k1_recover(&hash_fixed, &r_s, recovery_id);

        // 6. Compare recovered key to registered key_data.
        if recovered != key_data {
            panic_with_error!(e, Secp256k1VerifierError::KeyMismatch);
        }

        true
    }

    /// Returns the canonical 65-byte uncompressed public key.
    ///
    /// The 65-byte uncompressed SEC-1 form is already canonical — this is a
    /// pass-through, exactly as in `ed25519-verifier`. Compressed (33-byte)
    /// key normalization is not implemented because no client library in this
    /// project is known to produce compressed keys; add it here if one is
    /// found to do so.
    fn canonicalize_key(_e: &Env, key_data: BytesN<65>) -> Bytes {
        key_data.into()
    }

    /// Canonicalizes a batch of secp256k1 public keys, preserving input order.
    fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<65>>) -> Vec<Bytes> {
        let mut out: Vec<Bytes> = Vec::new(e);
        for k in key_data.iter() {
            out.push_back(Self::canonicalize_key(e, k));
        }
        out
    }
}

#[cfg(test)]
mod test;
