#![no_std]

// STUB — secp256k1 (MetaMask/EVM) verifier, not yet implemented.
// Deployed as a placeholder so the factory can be configured.
// verify() will panic if called — do not create secp256k1 accounts on this
// deployment.

use soroban_sdk::{contract, contracterror, contractimpl, panic_with_error, Bytes, Env, Vec};
use stellar_accounts::verifiers::Verifier;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Secp256k1VerifierError {
    NotImplemented = 1,
}

#[contract]
pub struct Secp256k1Verifier;

#[contractimpl]
impl Verifier for Secp256k1Verifier {
    type KeyData = Bytes;
    type SigData = Bytes;

    fn verify(e: &Env, _hash: Bytes, _key_data: Bytes, _sig_data: Bytes) -> bool {
        panic_with_error!(e, Secp256k1VerifierError::NotImplemented)
    }

    fn canonicalize_key(_e: &Env, key_data: Bytes) -> Bytes {
        key_data
    }

    fn batch_canonicalize_key(e: &Env, key_data: Vec<Bytes>) -> Vec<Bytes> {
        Vec::from_iter(e, key_data.iter())
    }
}
