#![no_std]

use soroban_sdk::{
    auth::{Context, CustomAccountInterface},
    contract, contractimpl,
    crypto::Hash,
    Address, BytesN, Env, Map, String, Symbol, Val, Vec,
};
use stellar_accounts::smart_account::{
    self as smart_account, AuthPayload, ContextRule, ContextRuleType, ExecutionEntryPoint, Signer,
    SmartAccount, SmartAccountError,
};
use stellar_contract_utils::upgradeable::{self as upgradeable, Upgradeable};

#[contract]
pub struct LatchSmartAccount;

#[contractimpl]
impl LatchSmartAccount {
    pub fn __constructor(e: &Env, signers: Vec<Signer>, policies: Map<Address, Val>) {
        smart_account::add_context_rule(
            e,
            &ContextRuleType::Default,
            &String::from_str(e, "default"),
            None,
            &signers,
            &policies,
        );
    }

    pub fn batch_add_signer(e: &Env, context_rule_id: u32, signers: Vec<Signer>) {
        e.current_contract_address().require_auth();
        smart_account::batch_add_signer(e, context_rule_id, &signers);
    }

    /// Proposes a call through a timelock-protected context rule.
    ///
    /// Identical to `execute()` in that it triggers `require_auth()` and the
    /// full policy enforcement pipeline, but does **not** call the target
    /// contract afterward. Policies whose `enforce()` stores a pending
    /// proposal (e.g. `timelock-policy`) rely on this: the proposal is
    /// recorded during the authorization phase, and the actual invocation
    /// happens later via the policy's own `execute_pending()` entrypoint.
    ///
    /// For context rules without a timelock policy, calling `propose()` is
    /// a no-op — auth succeeds but nothing is recorded and no call is made.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `target` - The address of the contract to propose calling.
    /// * `target_fn` - The function name to propose invoking.
    /// * `target_args` - Arguments to pass to the target function.
    pub fn propose(e: Env, _target: Address, _target_fn: Symbol, _target_args: Vec<Val>) {
        e.current_contract_address().require_auth();
        // No invoke_contract — policies record proposals during the auth
        // phase; delayed execution is handled by the policy's
        // execute_pending() entrypoint.
    }
}

#[contractimpl]
impl CustomAccountInterface for LatchSmartAccount {
    type Error = SmartAccountError;
    type Signature = AuthPayload;

    fn __check_auth(
        e: Env,
        signature_payload: Hash<32>,
        signatures: AuthPayload,
        auth_contexts: Vec<Context>,
    ) -> Result<(), Self::Error> {
        smart_account::do_check_auth(&e, &signature_payload, &signatures, &auth_contexts)
    }
}

#[contractimpl(contracttrait)]
impl SmartAccount for LatchSmartAccount {}

#[contractimpl(contracttrait)]
impl ExecutionEntryPoint for LatchSmartAccount {}

#[contractimpl]
impl Upgradeable for LatchSmartAccount {
    fn upgrade(e: &Env, new_wasm_hash: BytesN<32>, _operator: Address) {
        e.current_contract_address().require_auth();
        upgradeable::upgrade(e, &new_wasm_hash);
    }
}

#[cfg(test)]
mod test;
