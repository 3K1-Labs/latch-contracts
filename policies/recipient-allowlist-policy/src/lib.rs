#![no_std]

mod allowlist;

pub use allowlist::{
    RecipientAllowlistAccountParams, RecipientAllowlistData, RecipientAllowlistError,
    RecipientAllowlistStorageKey,
};
use soroban_sdk::{auth::Context, contract, contractimpl, Address, Env, Vec};
use stellar_accounts::{
    policies::Policy,
    smart_account::{ContextRule, Signer},
};

#[contract]
pub struct RecipientAllowlistPolicy;

#[contractimpl]
impl Policy for RecipientAllowlistPolicy {
    type AccountParams = RecipientAllowlistAccountParams;

    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        allowlist::enforce(e, &context, &authenticated_signers, &context_rule, &smart_account)
    }

    fn install(
        e: &Env,
        install_params: Self::AccountParams,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        allowlist::install(e, &install_params, &context_rule, &smart_account)
    }

    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        allowlist::uninstall(e, &context_rule, &smart_account)
    }
}

#[contractimpl]
impl RecipientAllowlistPolicy {
    pub fn get_allowed_recipients(
        e: &Env,
        context_rule_id: u32,
        smart_account: Address,
    ) -> Vec<Address> {
        allowlist::get_allowed_recipients(e, context_rule_id, &smart_account)
    }

    pub fn set_allowed_recipients(
        e: Env,
        allowed_recipients: Vec<Address>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        allowlist::set_allowed_recipients(&e, &allowed_recipients, &context_rule, &smart_account)
    }
}

#[cfg(test)]
mod test;
