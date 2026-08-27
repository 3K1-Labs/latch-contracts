#![no_std]

mod conditions;

pub use conditions::{
    Condition, ExpectedValue, Operator, ParameterScopedAccountParams, ParameterScopedData,
    ParameterScopedError, ParameterScopedStorageKey,
};
use soroban_sdk::{auth::Context, contract, contractimpl, Address, Env, Map, Symbol, Vec};
use stellar_accounts::{
    policies::Policy,
    smart_account::{ContextRule, Signer},
};

#[contract]
pub struct ParameterScopedPolicy;

#[contractimpl]
impl Policy for ParameterScopedPolicy {
    type AccountParams = ParameterScopedAccountParams;

    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        conditions::enforce(e, &context, &authenticated_signers, &context_rule, &smart_account)
    }

    fn install(
        e: &Env,
        install_params: Self::AccountParams,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        conditions::install(e, &install_params, &context_rule, &smart_account)
    }

    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        conditions::uninstall(e, &context_rule, &smart_account)
    }
}

#[contractimpl]
impl ParameterScopedPolicy {
    pub fn get_conditions(
        e: &Env,
        context_rule_id: u32,
        smart_account: Address,
    ) -> Map<Symbol, Vec<Condition>> {
        conditions::get_conditions(e, context_rule_id, &smart_account)
    }
}

#[cfg(test)]
mod test;
