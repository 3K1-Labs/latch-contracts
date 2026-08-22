//! Plain Ed25519 verifier — signs the raw 32-byte auth payload hash
//! directly, no message wrapping.
//!
//! This is the verifier for any signer that can sign arbitrary bytes
//! directly: a native Stellar/Ed25519 key, an SDK-integrated wallet, or any
//! client that isn't routed through a defensive third-party signing popup
//! that refuses raw payloads. If a wallet needs its signature wrapped in a
//! human-readable message before it'll sign (e.g. Phantom's browser
//! extension, which rejects bare 32-byte payloads as an anti-blind-signing
//! heuristic), use `modified-ed25519-verifier` instead — this crate does
//! not, and should not, work around client-specific signing-popup
//! constraints.
//!
//! Thin `#[contract]` wrapper around OZ's `ed25519` verifier module
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
