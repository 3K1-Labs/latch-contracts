//! Stateless raw P-256 (secp256r1) signature verifier.
//!
//! Verifies a 64-byte compact ECDSA signature (`r || s`) over the already
//! produced 32-byte Soroban authorization payload hash using a 65-byte
//! uncompressed SEC1 public key (`0x04 || X || Y`). The digest is not hashed a
//! second time. WebAuthn assertions use a different verifier and protocol.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, Bytes, BytesN, Env, Vec,
};
use stellar_accounts::verifiers::Verifier;

/// Input-format errors detected before native P-256 verification.
///
/// Error numbering starts at 1 because this verifier is independently
/// deployed.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum P256VerifierError {
    /// The Soroban authorization payload hash is not exactly 32 bytes.
    InvalidHashLength = 1,
    /// The public key does not use the required uncompressed SEC1 prefix.
    InvalidKeyEncoding = 2,
}

/// Deployable singleton for raw P-256 signature verification.
///
/// The contract has no constructor, storage, admin, or upgrade entrypoint.
#[contract]
pub struct P256Verifier;

#[contractimpl]
impl Verifier for P256Verifier {
    type KeyData = BytesN<65>;
    type SigData = BytesN<64>;

    /// Verifies a raw P-256 signature over a Soroban authorization payload
    /// hash.
    ///
    /// `key_data` must be a 65-byte uncompressed SEC1 point (`0x04 || X || Y`),
    /// and `sig_data` must be a low-S 64-byte compact signature (`r || s`). The
    /// supplied 32-byte `hash` is passed directly to the Soroban host without
    /// another hashing step.
    ///
    /// # Errors
    ///
    /// - [`P256VerifierError::InvalidHashLength`] - `hash` is not 32 bytes.
    /// - [`P256VerifierError::InvalidKeyEncoding`] - `key_data` does not start
    ///   with the uncompressed SEC1 prefix `0x04`.
    /// - Native `Error(Crypto, InvalidInput)` - the point, signature
    ///   components, signature, key, or digest is cryptographically invalid.
    fn verify(e: &Env, hash: Bytes, key_data: BytesN<65>, sig_data: BytesN<64>) -> bool {
        if hash.len() != 32 {
            panic_with_error!(e, P256VerifierError::InvalidHashLength);
        }
        validate_key_encoding(e, &key_data);

        let hash_n = match BytesN::<32>::try_from(hash) {
            Ok(hash_n) => hash_n,
            Err(_) => panic_with_error!(e, P256VerifierError::InvalidHashLength),
        };

        e.crypto_hazmat().secp256r1_verify(&key_data, &hash_n, &sig_data);
        true
    }

    /// Returns the canonical 65-byte uncompressed SEC1 public key unchanged.
    ///
    /// # Errors
    ///
    /// - [`P256VerifierError::InvalidKeyEncoding`] - `key_data` does not start
    ///   with the uncompressed SEC1 prefix `0x04`.
    fn canonicalize_key(e: &Env, key_data: BytesN<65>) -> Bytes {
        validate_key_encoding(e, &key_data);
        key_data.into()
    }

    /// Canonicalizes every P-256 key while preserving input order.
    ///
    /// # Errors
    ///
    /// - [`P256VerifierError::InvalidKeyEncoding`] - any key does not start
    ///   with the uncompressed SEC1 prefix `0x04`.
    fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<65>>) -> Vec<Bytes> {
        Vec::from_iter(e, key_data.iter().map(|key| Self::canonicalize_key(e, key)))
    }
}

fn validate_key_encoding(e: &Env, key_data: &BytesN<65>) {
    if key_data.get(0) != Some(0x04) {
        panic_with_error!(e, P256VerifierError::InvalidKeyEncoding);
    }
}

#[cfg(test)]
mod test;
