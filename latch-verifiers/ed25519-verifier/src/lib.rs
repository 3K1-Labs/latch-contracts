//! Contract for verifying Ed25519 digital signatures.
//!
//! Verifies a signature over the 32-byte Soroban auth payload hash. Thin
//! `#[contract]` wrapper around OZ's `ed25519` verifier module
//! (`stellar_accounts::verifiers::ed25519`) — all real logic lives
//! upstream, this crate only supplies the deployable contract shell.
#![no_std]

use soroban_sdk::{contract, contractimpl, Bytes, BytesN, Env, Vec};
use stellar_accounts::verifiers::{ed25519, Verifier};

#[contract]
pub struct Ed25519Verifier;

#[contractimpl]
impl Verifier for Ed25519Verifier {
    type KeyData = BytesN<32>;
    type SigData = BytesN<64>;

    /// Verify an Ed25519 signature over the raw 32-byte auth payload hash.
    ///
    /// Panics on any verification failure (invalid signature, wrong key,
    /// wrong payload).
    fn verify(e: &Env, hash: Bytes, key_data: BytesN<32>, sig_data: BytesN<64>) -> bool {
        ed25519::verify(e, &hash, &key_data, &sig_data)
    }

    /// Returns the canonical 32-byte representation of the Ed25519 public key.
    ///
    /// Ed25519 keys have exactly one canonical encoding — this is a
    /// pass-through.
    fn canonicalize_key(e: &Env, key_data: BytesN<32>) -> Bytes {
        ed25519::canonicalize_key(e, &key_data)
    }

    /// Canonicalizes a batch of Ed25519 keys, preserving input order.
    fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<32>>) -> Vec<Bytes> {
        ed25519::batch_canonicalize_key(e, &key_data)
    }
}

#[cfg(test)]
mod test;
