#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, Bytes, BytesN, Env, Vec,
};
use stellar_accounts::verifiers::Verifier;

#[contract]
pub struct P256Verifier;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum P256VerifierError {
    InvalidHashLength = 1,
    InvalidKeyEncoding = 2,
    InvalidSignatureEncoding = 3,
}

fn hash32_from_bytesn(bytes: &BytesN<32>) -> soroban_sdk::crypto::Hash<32> {
    unsafe { core::mem::transmute_copy(bytes) }
}

#[contractimpl]
impl Verifier for P256Verifier {
    type KeyData = BytesN<65>;
    type SigData = BytesN<64>;

    /// Verify a secp256r1 signature over the raw 32-byte Soroban auth payload
    /// hash.
    ///
    /// This is the host primitive for raw secp256r1 verification: the client
    /// passes the 65-byte uncompressed SEC1 public key, the 32-byte auth hash,
    /// and the 64-byte compact `r || s` signature. The host enforces the
    /// canonical low-S choice and rejects malformed keys and signatures.
    fn verify(e: &Env, hash: Bytes, key_data: BytesN<65>, sig_data: BytesN<64>) -> bool {
        if hash.len() != 32 {
            panic_with_error!(e, P256VerifierError::InvalidHashLength);
        }

        let key_prefix = key_data.to_array()[0];
        if key_prefix != 0x04 {
            panic_with_error!(e, P256VerifierError::InvalidKeyEncoding);
        }

        let hash_bytes = hash.to_buffer::<32>();
        let mut hash_array = [0u8; 32];
        hash_array.copy_from_slice(hash_bytes.as_slice());
        let hash_n = BytesN::<32>::from_array(e, &hash_array);
        let digest = hash32_from_bytesn(&hash_n);

        e.crypto().secp256r1_verify(&key_data, &digest, &sig_data);
        true
    }

    /// Returns the canonical 65-byte uncompressed secp256r1 public key.
    fn canonicalize_key(e: &Env, key_data: BytesN<65>) -> Bytes {
        let key_prefix = key_data.to_array()[0];
        if key_prefix != 0x04 {
            panic_with_error!(e, P256VerifierError::InvalidKeyEncoding);
        }

        Bytes::from_slice(e, &key_data.to_array())
    }

    /// Canonicalizes a batch of secp256r1 keys while preserving input order.
    fn batch_canonicalize_key(e: &Env, key_data: Vec<BytesN<65>>) -> Vec<Bytes> {
        Vec::from_iter(e, key_data.iter().map(|key| Self::canonicalize_key(e, key)))
    }
}

#[cfg(test)]
mod test;
