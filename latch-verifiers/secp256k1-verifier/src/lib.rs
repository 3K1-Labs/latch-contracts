//! Stateless raw secp256k1 signature verifier.
//!
//! Verifies a 65-byte recoverable ECDSA signature (`r || s || recovery_id`)
//! over the already produced 32-byte Latch authorization payload hash using a
//! 65-byte uncompressed SEC1 public key (`0x04 || X || Y`). The digest is not
//! hashed a second time. The Soroban host enforces low-S normalization.
//!
//! The recovery ID is the raw ECDSA value `0` or `1`. Clients that receive an
//! Ethereum-style `v` value such as `27` or `28` must normalize it off-chain.
//! This is a curve-level verifier, not an EIP-191 or wallet-specific flow.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, Bytes, BytesN, Env, Vec,
};
use stellar_accounts::verifiers::Verifier;

/// Input and authorization errors detected by the verifier.
///
/// Error numbering starts at 1 because this verifier is independently
/// deployed.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Secp256k1VerifierError {
    /// The Latch authorization payload hash is not exactly 32 bytes.
    InvalidHashLength = 1,
    /// The public key does not use the required uncompressed SEC1 prefix.
    InvalidKeyEncoding = 2,
    /// The signature recovered a public key other than the registered key.
    KeyMismatch = 3,
    /// The recovery ID is not the raw ECDSA value 0 or 1.
    InvalidRecoveryId = 4,
}

/// Deployable singleton for raw secp256k1 signature verification.
///
/// The contract has no constructor, storage, admin, or upgrade entrypoint.
#[contract]
pub struct Secp256k1Verifier;

#[contractimpl]
impl Verifier for Secp256k1Verifier {
    type KeyData = BytesN<65>;
    type SigData = BytesN<65>;

    /// Recovers the signer of a raw secp256k1 signature and compares it with
    /// the registered key.
    ///
    /// `key_data` must be a 65-byte uncompressed SEC1 point (`0x04 || X || Y`)
    /// and `sig_data` must be `r[32] || s[32] || recovery_id[1]`, with a raw
    /// recovery ID of `0` or `1`. The Soroban host requires a normalized low-S
    /// signature. The supplied 32-byte `hash` is passed directly to the Soroban
    /// host without another hashing step.
    ///
    /// The hazmat API is necessary here because the generic verifier boundary
    /// represents the already-SHA-256-hashed Latch auth digest as `Bytes`. The
    /// SDK has no supported conversion from those raw bytes back to `Hash<32>`;
    /// hashing again to obtain that type would verify a different message.
    ///
    /// # Errors
    ///
    /// - [`Secp256k1VerifierError::InvalidHashLength`] - `hash` is not 32
    ///   bytes.
    /// - [`Secp256k1VerifierError::InvalidKeyEncoding`] - `key_data` does not
    ///   start with the uncompressed SEC1 prefix `0x04`.
    /// - [`Secp256k1VerifierError::InvalidRecoveryId`] - the signature recovery
    ///   ID is not `0` or `1`; Ethereum-style `27`/`28` values are not
    ///   accepted.
    /// - [`Secp256k1VerifierError::KeyMismatch`] - recovery succeeds but yields
    ///   a public key other than `key_data`.
    /// - Native `Error(Crypto, InvalidInput)` - `r` or `s` is invalid or
    ///   non-normalized, or public key recovery otherwise fails.
    fn verify(e: &Env, hash: Bytes, key_data: BytesN<65>, sig_data: BytesN<65>) -> bool {
        if hash.len() != 32 {
            panic_with_error!(e, Secp256k1VerifierError::InvalidHashLength);
        }
        validate_key_encoding(e, &key_data);

        let hash_n = match BytesN::<32>::try_from(hash) {
            Ok(hash_n) => hash_n,
            Err(_) => panic_with_error!(e, Secp256k1VerifierError::InvalidHashLength),
        };

        let sig_bytes = sig_data.to_array();
        let recovery_id = u32::from(sig_bytes[64]);
        if recovery_id > 1 {
            panic_with_error!(e, Secp256k1VerifierError::InvalidRecoveryId);
        }

        let mut compact_signature = [0_u8; 64];
        compact_signature.copy_from_slice(&sig_bytes[..64]);
        let signature = BytesN::from_array(e, &compact_signature);

        let recovered = e.crypto_hazmat().secp256k1_recover(&hash_n, &signature, recovery_id);
        if recovered != key_data {
            panic_with_error!(e, Secp256k1VerifierError::KeyMismatch);
        }

        true
    }

    /// Returns the canonical 65-byte uncompressed SEC1 public key unchanged.
    ///
    /// # Errors
    ///
    /// - [`Secp256k1VerifierError::InvalidKeyEncoding`] - `key_data` does not
    ///   start with the uncompressed SEC1 prefix `0x04`.
    fn canonicalize_key(e: &Env, key_data: BytesN<65>) -> Bytes {
        validate_key_encoding(e, &key_data);
        key_data.into()
    }

    /// Canonicalizes every secp256k1 key while preserving input order.
    ///
    /// # Errors
    ///
    /// - [`Secp256k1VerifierError::InvalidKeyEncoding`] - any key does not
    ///   start with the uncompressed SEC1 prefix `0x04`.
    fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<65>>) -> Vec<Bytes> {
        Vec::from_iter(e, key_data.iter().map(|key| Self::canonicalize_key(e, key)))
    }
}

fn validate_key_encoding(e: &Env, key_data: &BytesN<65>) {
    if key_data.get(0) != Some(0x04) {
        panic_with_error!(e, Secp256k1VerifierError::InvalidKeyEncoding);
    }
}

#[cfg(test)]
mod test;
