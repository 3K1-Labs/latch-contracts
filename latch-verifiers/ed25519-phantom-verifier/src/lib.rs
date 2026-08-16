#![no_std]

use soroban_sdk::{contract, contractimpl, Bytes, BytesN, Env, Vec};
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

#[contract]
pub struct Ed25519PhantomVerifier;

#[contractimpl]
impl Verifier for Ed25519PhantomVerifier {
    type KeyData = BytesN<32>;
    type SigData = BytesN<64>;

    /// Verify a Phantom-produced Ed25519 signature over the Latch signing
    /// convention.
    ///
    /// The client signs: `"Stellar Smart Account Auth:\n" +
    /// lowercase_hex(auth_payload_hash)` This contract reconstructs that
    /// message from `hash` and verifies `sig_data` against it.
    ///
    /// Panics with `Error(Crypto, InvalidInput)` if the signature is invalid.
    fn verify(e: &Env, hash: Bytes, key_data: BytesN<32>, sig_data: BytesN<64>) -> bool {
        assert!(hash.len() == PAYLOAD_LEN as u32, "hash must be 32 bytes");

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
