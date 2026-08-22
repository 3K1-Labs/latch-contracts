//! Demo/reference crate — **not part of the shipped verifier lineup.**
//!
//! This was built to prove, in a one-off demo, that a Phantom-held Ed25519
//! key could deploy and become the owning signer of a Latch smart account
//! on-chain. It isn't wired into any product surface: Latch and Phantom are
//! separate browser extensions, and nothing gives Latch's extension an
//! ongoing way to drive Phantom's signing popup after that initial demo
//! transaction — that would require real extension-to-extension
//! integration that was never built. The strategy going forward is Latch
//! as the wallet (its own extension + mobile app), with other wallets
//! integrating *via SDK*, not by Latch reaching into their signing UI.
//!
//! Kept here, out of `latch-verifiers/`, purely as a worked reference: the
//! wrapping pattern below (hex-encode the hash, prepend a human-readable
//! prefix, verify against the wrapped message) is the general shape needed
//! any time a *specific* wallet's signing popup refuses to sign a raw
//! 32-byte payload — the same problem `secp256k1-verifier-spec.md` flags
//! as a possible future need if a real MetaMask-popup integration is ever
//! attempted. Do not deploy this contract or treat it as a real verifier.
//!
//! # Why the underlying constraint exists (kept for reference)
//!
//! Phantom's browser-extension `signMessage` popup is a generic,
//! untrusted-dApp-facing API — it can't tell "a legitimate 32-byte auth
//! hash" from "a malicious site trying to get you to blind-sign an opaque
//! payload that's actually a transaction." As a defensive heuristic,
//! Phantom refuses to sign raw 32-byte payloads at all (they're
//! indistinguishable from Solana transaction hashes). The workaround: wrap
//! the hash in a human-readable message before asking Phantom to sign it —
//! `AUTH_PREFIX + lowercase_hex(auth_payload_hash)` — and this contract
//! reconstructs that exact 92-byte message and verifies the signature
//! against it.
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
