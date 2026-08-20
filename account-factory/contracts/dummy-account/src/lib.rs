//! Minimal smart account stub for factory integration tests.
//! Accepts the same constructor signature the factory passes but does nothing.
#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, Map, Val, Vec};
use stellar_accounts::smart_account::Signer;

#[contract]
pub struct DummyAccount;

#[contractimpl]
impl DummyAccount {
    pub fn __constructor(_env: Env, _signers: Vec<Signer>, _policies: Map<Address, Val>) {}
}
