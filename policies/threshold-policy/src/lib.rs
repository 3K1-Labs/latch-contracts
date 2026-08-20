//! Thin `#[contract]` wrapper around OZ's `simple_threshold` policy —
//! `stellar_accounts::policies::simple_threshold`. All logic lives upstream;
//! this crate only supplies the deployable contract shell.
//!
//! # Security Warning: Signer Set Divergence
//!
//! The threshold is validated against the signer count only at install time.
//! It is **not automatically updated** when signers are later added to or
//! removed from the account's `ContextRule`. Left unattended, this causes:
//!
//! - **DoS**: removing signers can drop the count below the stored threshold,
//!   permanently blocking any action this policy governs until the threshold is
//!   lowered.
//! - **Silent security degradation**: adding signers without raising the
//!   threshold quietly turns a strict N-of-N into a weaker N-of-(N+M).
//!
//! Whoever administers signer changes on an account using this policy
//! **must** call `set_threshold` in the same transaction — before removing
//! signers, or after adding them. See OZ's `simple_threshold` module docs
//! for the full writeup and worked examples.
#![no_std]

use soroban_sdk::{auth::Context, contract, contractimpl, Address, Env, Vec};
use stellar_accounts::{
    policies::{simple_threshold, simple_threshold::SimpleThresholdAccountParams, Policy},
    smart_account::{ContextRule, Signer},
};

#[contract]
pub struct ThresholdPolicy;

#[contractimpl]
impl Policy for ThresholdPolicy {
    type AccountParams = SimpleThresholdAccountParams;

    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        simple_threshold::enforce(
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
        simple_threshold::install(e, &install_params, &context_rule, &smart_account)
    }

    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        simple_threshold::uninstall(e, &context_rule, &smart_account)
    }
}

#[contractimpl]
impl ThresholdPolicy {
    pub fn get_threshold(e: &Env, context_rule_id: u32, smart_account: Address) -> u32 {
        simple_threshold::get_threshold(e, context_rule_id, &smart_account)
    }

    pub fn set_threshold(
        e: Env,
        threshold: u32,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        simple_threshold::set_threshold(&e, threshold, &context_rule, &smart_account)
    }
}
