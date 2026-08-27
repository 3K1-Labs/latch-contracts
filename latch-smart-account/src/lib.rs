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
    /// Adding the signer would weaken the policy's threshold ratio (e.g.
    /// 3-of-3 becomes 3-of-5) and the caller did not explicitly
    /// acknowledge this via `confirm_threshold_unchanged`.
    SignerAddedWouldWeakenPolicy = 2,
    /// The single-signer `add_signer` entry point (inherited from
    /// `SmartAccount`'s default) is disabled on this contract. Its fixed
    /// trait signature has no room for a `confirm_threshold_unchanged`-style
    /// acknowledgment, so it can't carry the same addition-side protection
    /// `batch_add_signer` does. Use `batch_add_signer` instead, even for a
    /// single signer.
    SingleSignerAdditionDisabled = 3,
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

    /// Adds signers to a context rule.
    ///
    /// Requires the caller to explicitly acknowledge that adding signers
    /// may weaken the existing threshold ratio (e.g. a 3-of-3 becoming
    /// 3-of-5). Setting `confirm_threshold_unchanged = false` always
    /// reverts — this forces the caller to consciously accept the ratio
    /// change.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `context_rule_id` - The ID of the context rule to modify.
    /// * `signers` - The signers to add.
    /// * `confirm_threshold_unchanged` - Must be `true` for the addition to
    ///   proceed. If `false`, the operation reverts with
    ///   [`LatchSmartAccountError::SignerAddedWouldWeakenPolicy`].
    pub fn batch_add_signer(
        e: &Env,
        context_rule_id: u32,
        signers: Vec<Signer>,
        confirm_threshold_unchanged: bool,
    ) {
        e.current_contract_address().require_auth();

        if !confirm_threshold_unchanged {
            panic_with_error!(e, LatchSmartAccountError::SignerAddedWouldWeakenPolicy);
        }

        smart_account::batch_add_signer(e, context_rule_id, &signers);
    }
}

/// Resolves the actual [`Signer`] corresponding to a global `signer_id` within
/// a context rule by scanning the rule's `signer_ids` / `signers` arrays
/// (positionally aligned).
fn resolve_signer(e: &Env, rule: &ContextRule, signer_id: u32) -> Signer {
    for (i, id) in rule.signer_ids.iter().enumerate() {
        if id == signer_id {
            return rule.signers.get_unchecked(i as u32);
        }
    }
    panic_with_error!(e, SmartAccountError::SignerNotFound);
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
impl SmartAccount for LatchSmartAccount {
    /// Disabled — the inherited default has no way to require the same
    /// `confirm_threshold_unchanged` acknowledgment `batch_add_signer`
    /// enforces, so leaving it live would let anyone bypass that check by
    /// adding signers one at a time. Always panics with
    /// [`LatchSmartAccountError::SingleSignerAdditionDisabled`]; use
    /// `batch_add_signer` instead, even for a single signer.
    fn add_signer(e: &Env, _context_rule_id: u32, _signer: Signer) -> u32 {
        panic_with_error!(e, LatchSmartAccountError::SingleSignerAdditionDisabled);
    }

    fn remove_signer(e: &Env, context_rule_id: u32, signer_id: u32) {
        e.current_contract_address().require_auth();

        let rule = smart_account::get_context_rule(e, context_rule_id);
        let signer_to_remove = resolve_signer(e, &rule, signer_id);
        let remaining_count = rule.signers.len().saturating_sub(1);

        for policy in rule.policies.iter() {
            let args: Vec<Val> = vec![
                e,
                context_rule_id.into_val(e),
                e.current_contract_address().into_val(e),
                signer_to_remove.clone().into_val(e),
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
