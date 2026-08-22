//! Ed25519 verifier for a modified, prefixed signing convention — **not**
//! the verifier for a plain Ed25519 signer. See `ed25519-verifier` for that.
//!
//! # Why this exists
//!
//! This crate exists solely because of a Phantom wallet UX constraint, not
//! any cryptographic or protocol requirement. Phantom's browser-extension
//! `signMessage` popup is a generic, untrusted-dApp-facing API — it can't
//! tell "Latch requesting a legitimate 32-byte auth hash" from "a malicious
//! site trying to get you to blind-sign an opaque payload that's actually a
//! transaction." As a defensive heuristic, Phantom refuses to sign raw
//! 32-byte payloads at all (they're indistinguishable from Solana
//! transaction hashes).
//!
//! To work around that, the client wraps the hash in a human-readable
//! message before asking Phantom to sign it: `AUTH_PREFIX +
//! lowercase_hex(auth_payload_hash)`. This contract reconstructs that exact
//! 92-byte message and verifies the signature against it — real
//! cryptographic verification, just over a wrapped payload instead of the
//! raw hash.
//!
//! This is an artifact of going through Phantom's *external* signing
//! popup — the interface any third-party dApp uses. A wallet with native
//! Latch/SDK integration (its own trusted code constructing and signing the
//! request internally, not routed through the public "sign this opaque
//! thing" popup) would have no untrusted third party to defend against, and
//! could sign the raw hash directly like any other SDK-integrated signer —
//! i.e. it would use `ed25519-verifier`, not this one.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, Bytes, BytesN, Env, Vec,
};
use stellar_accounts::verifiers::{ed25519 as oz_ed25519, Verifier};

/// The prefix Phantom wallet prepends before signing.
/// Phantom rejects raw 32-byte payloads (indistinguishable from Solana tx
/// hashes), so the client constructs: AUTH_PREFIX + hex(auth_payload_hash) and
/// signs that.
const AUTH_PREFIX: &[u8] = b"Stellar Smart Account Auth:\n";
const PREFIX_LEN: usize = 28;
const PAYLOAD_LEN: usize = 32;
const HEX_LEN: usize = 64; // 32 bytes * 2 hex chars each
const SIGNED_MSG_LEN: usize = PREFIX_LEN + HEX_LEN; // 92 bytes total

/// Error codes for the modified Ed25519 verifier.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ModifiedEd25519VerifierError {
    /// `hash` was not exactly 32 bytes.
    InvalidHashLength = 1,
}

#[contract]
pub struct ModifiedEd25519Verifier;

#[contractimpl]
impl Verifier for ModifiedEd25519Verifier {
    type KeyData = BytesN<32>;
    type SigData = BytesN<64>;

    /// Verify a Phantom-produced Ed25519 signature over the Latch signing
    /// convention.
    ///
    /// The client signs: `"Stellar Smart Account Auth:\n" +
    /// lowercase_hex(auth_payload_hash)` This contract reconstructs that
    /// message from `hash` and verifies `sig_data` against it.
    ///
    /// # Errors
    ///
    /// * [`ModifiedEd25519VerifierError::InvalidHashLength`] - When `hash` is
    ///   not exactly 32 bytes.
    ///
    /// Panics with `Error(Crypto, InvalidInput)` if the signature is invalid.
    fn verify(e: &Env, hash: Bytes, key_data: BytesN<32>, sig_data: BytesN<64>) -> bool {
        if hash.len() != PAYLOAD_LEN as u32 {
            panic_with_error!(e, ModifiedEd25519VerifierError::InvalidHashLength);
        }

        // Build the 92-byte signed message: PREFIX + hex(hash)
        let mut signed_msg = [0u8; SIGNED_MSG_LEN];
        signed_msg[..PREFIX_LEN].copy_from_slice(AUTH_PREFIX);

        let hash_arr = hash.to_buffer::<PAYLOAD_LEN>();
        hex_encode_lower(&mut signed_msg[PREFIX_LEN..], hash_arr.as_slice());

        let signed_msg_bytes = Bytes::from_slice(e, &signed_msg);

        // Delegate to the Soroban host builtin. Panics on invalid signature.
        e.crypto().ed25519_verify(&key_data, &signed_msg_bytes, &sig_data);

        true
    }

    /// Returns the canonical 32-byte representation of the Ed25519 public key.
    ///
    /// Ed25519 keys have exactly one canonical encoding — this is a
    /// pass-through.
    fn canonicalize_key(e: &Env, key_data: BytesN<32>) -> Bytes {
        oz_ed25519::canonicalize_key(e, &key_data)
    }

    /// Canonicalizes a batch of Ed25519 keys, preserving input order.
    fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<32>>) -> Vec<Bytes> {
        oz_ed25519::batch_canonicalize_key(e, &key_data)
    }
}

/// Encodes `src` as lowercase hex into `dst`.
/// `dst` must be exactly `src.len() * 2` bytes.
fn hex_encode_lower(dst: &mut [u8], src: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut i = 0;
    for &byte in src {
        dst[i] = HEX[(byte >> 4) as usize];
        dst[i + 1] = HEX[(byte & 0x0f) as usize];
        i += 2;
    }
}

#[cfg(test)]
mod test;
