//! Minimal no-op contract stub for factory integration tests.
//! Used as a stand-in for ed25519/secp256k1/webauthn verifiers and the
//! threshold policy — all deployed by the factory with no constructor args.
#![no_std]

use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct DummySingleton;

#[contractimpl]
impl DummySingleton {
    pub fn __constructor(_env: Env) {}
}
