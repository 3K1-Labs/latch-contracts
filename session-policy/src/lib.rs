#![no_std]

mod allowlist;

use soroban_sdk::{auth::Context, contract, contractimpl, Address, Env, Symbol, Vec};
use stellar_accounts::{
    policies::Policy,
    smart_account::{ContextRule, Signer},
};

pub use allowlist::{SessionAccountParams, SessionData, SessionError, SessionStorageKey};

#[contract]
pub struct SessionPolicy;

#[contractimpl]
impl Policy for SessionPolicy {
    type AccountParams = SessionAccountParams;

    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        allowlist::enforce(
            e,
            &context,
            &authenticated_signers,
            &context_rule,
            &smart_account,
        )
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
impl SessionPolicy {
    pub fn get_allowed_fns(e: &Env, context_rule_id: u32, smart_account: Address) -> Vec<Symbol> {
        allowlist::get_allowed_fns(e, context_rule_id, &smart_account)
    }
}

#[cfg(test)]
mod test;
