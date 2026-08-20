//! Thin `#[contract]` wrapper around OZ's `weighted_threshold` policy —
//! `stellar_accounts::policies::weighted_threshold`. All logic lives
//! upstream; this crate only supplies the deployable contract shell.
//!
//! # Security Warning: Signer Set Divergence
//!
//! Signer weights and the threshold are validated against each other only at
//! install time. Neither is **automatically updated** when signers are later
//! added to or removed from the account's `ContextRule`. Left unattended,
//! this causes:
//!
//! - **DoS**: removing a heavily-weighted signer can drop total available
//!   weight below the stored threshold, permanently blocking any action this
//!   policy governs until the threshold is lowered.
//! - **Silent security degradation**: adding signers without assigning them
//!   weight contributes 0 by default (harmless but confusing); adding weighted
//!   signers without raising the threshold quietly weakens a 150-of-250
//!   multisig into e.g. 150-of-350.
//!
//! Whoever administers signer changes on an account using this policy
//! **must** call `set_signer_weight` and/or `set_threshold` in the same
//! transaction — before removing signers, or after adding them. See OZ's
//! `weighted_threshold` module docs for the full writeup and worked
//! examples.
#![no_std]

use soroban_sdk::{auth::Context, contract, contractimpl, Address, Env, Map, Vec};
use stellar_accounts::{
    policies::{weighted_threshold, weighted_threshold::WeightedThresholdAccountParams, Policy},
    smart_account::{ContextRule, Signer},
};

#[contract]
pub struct WeightedThresholdPolicy;

#[contractimpl]
impl Policy for WeightedThresholdPolicy {
    type AccountParams = WeightedThresholdAccountParams;

    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        weighted_threshold::enforce(
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
        weighted_threshold::install(e, &install_params, &context_rule, &smart_account)
    }

    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        weighted_threshold::uninstall(e, &context_rule, &smart_account)
    }
}

#[contractimpl]
impl WeightedThresholdPolicy {
    pub fn get_threshold(e: &Env, context_rule_id: u32, smart_account: Address) -> u32 {
        weighted_threshold::get_threshold(e, context_rule_id, &smart_account)
    }

    pub fn get_signer_weights(
        e: &Env,
        context_rule: ContextRule,
        smart_account: Address,
    ) -> Map<Signer, u32> {
        weighted_threshold::get_signer_weights(e, &context_rule, &smart_account)
    }

    pub fn set_threshold(
        e: Env,
        threshold: u32,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        weighted_threshold::set_threshold(&e, threshold, &context_rule, &smart_account)
    }

    pub fn set_signer_weight(
        e: Env,
        signer: Signer,
        weight: u32,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        weighted_threshold::set_signer_weight(&e, &signer, weight, &context_rule, &smart_account)
    }
}
