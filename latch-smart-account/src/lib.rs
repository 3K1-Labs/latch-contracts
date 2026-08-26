#![no_std]

use soroban_sdk::{
    auth::{Context, CustomAccountInterface},
    contract, contracterror, contractimpl,
    crypto::Hash,
    panic_with_error, vec, Address, BytesN, Env, IntoVal, Map, String, Symbol, Val, Vec,
};
use stellar_accounts::smart_account::{
    self as smart_account, AuthPayload, ContextRule, ContextRuleType, ExecutionEntryPoint, Signer,
    SmartAccount, SmartAccountError,
};
use stellar_contract_utils::upgradeable::{self as upgradeable, Upgradeable};

/// Error codes for Latch-specific smart account operations.
///
/// This is a standalone, independently-deployed contract, not a module of
/// the upstream `stellar-accounts` crate, so it is not part of that crate's
/// shared error-numbering convention. Numbering starts fresh at `1`.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum LatchSmartAccountError {
    /// Removing the signer would make a policy's threshold unreachable,
    /// permanently blocking authorization for this context rule.
    SignerRemovedWouldBreakPolicy = 1,
}

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

    /// Removes a signer from a context rule, but first probes every attached
    /// policy to verify the removal would not make any threshold unreachable.
    ///
    /// For each policy attached to the context rule (up to `MAX_POLICIES`),
    /// calls `would_remain_reachable(context_rule_id, smart_account,
    /// remaining_signer_count)` via `try_invoke_contract`. Policies that do
    /// not implement this function are silently skipped ("no opinion"). If
    /// any implementing policy returns `false`, the entire operation is
    /// reverted with [`LatchSmartAccountError::SignerRemovedWouldBreakPolicy`]
    /// before the signer is touched.
    ///
    /// On success, delegates to OZ's `storage::remove_signer` — the audited,
    /// unmodified path.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `context_rule_id` - The ID of the context rule to modify.
    /// * `signer_id` - The ID of the signer to remove.
    ///
    /// # Errors
    ///
    /// * [`LatchSmartAccountError::SignerRemovedWouldBreakPolicy`] - When an
    ///   attached policy reports that the removal would leave its threshold
    ///   unreachable.
    ///
    /// # Notes
    ///
    /// Requires authorization from the smart account itself
    /// (`e.current_contract_address().require_auth()`).
    pub fn remove_signer_checked(e: &Env, context_rule_id: u32, signer_id: u32) {
        e.current_contract_address().require_auth();

        let rule = smart_account::get_context_rule(e, context_rule_id);
        let remaining_count = rule.signers.len() - 1;

        for policy in rule.policies.iter() {
            let args: Vec<Val> = vec![
                e,
                context_rule_id.into_val(e),
                e.current_contract_address().into_val(e),
                remaining_count.into_val(e),
            ];
            if let Ok(Ok(reachable)) = e.try_invoke_contract::<bool, soroban_sdk::Error>(
                &policy,
                &Symbol::new(e, "would_remain_reachable"),
                args,
            ) {
                if !reachable {
                    panic_with_error!(e, LatchSmartAccountError::SignerRemovedWouldBreakPolicy);
                }
            }
        }

        smart_account::remove_signer(e, context_rule_id, signer_id);
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
