//! Weighted M-of-N threshold policy for Latch smart accounts.
//!
//! Like `threshold-policy`, but signers aren't equal: each signer in a
//! context rule is assigned an individual weight, and authorization
//! requires the *sum* of authenticated signers' weights to reach a minimum
//! threshold — not just a headcount. This lets some signers outweigh
//! others, e.g. CEO=100, CTO=75, CFO=75, threshold=150 requires either the
//! CEO plus anyone else, or both the CTO and CFO together.
//!
//! This is a thin `#[contract]` wrapper around OZ's `weighted_threshold`
//! policy (`stellar_accounts::policies::weighted_threshold`) — all real
//! logic lives upstream, this crate only supplies the deployable contract
//! shell the factory and smart accounts can actually install and call.
//!
//! # How it works
//!
//! - `install` — called once per `(smart_account, context_rule)` pair, with a
//!   signer→weight map and a `threshold`. Rejects `threshold == 0` or a
//!   threshold that exceeds the sum of all configured weights.
//! - `enforce` — called during authorization; sums the weights of the signers
//!   who actually authenticated for this call (unweighted/unknown signers
//!   contribute 0), and panics unless that sum is `>= threshold`.
//! - `set_signer_weight` / `get_signer_weights` — read or change one signer's
//!   weight after installation.
//! - `set_threshold` / `get_threshold` — read or change the stored threshold
//!   after installation.
//! - `uninstall` — removes all stored weights and the threshold.
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

#[cfg(test)]
mod test;
use stellar_accounts::{
    policies::{weighted_threshold, weighted_threshold::WeightedThresholdAccountParams, Policy},
    smart_account::{ContextRule, ContextRuleType, Signer},
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
    /// Returns the stored threshold for a context rule on this smart account.
    pub fn get_threshold(e: &Env, context_rule_id: u32, smart_account: Address) -> u32 {
        weighted_threshold::get_threshold(e, context_rule_id, &smart_account)
    }

    /// Returns the signer-to-weight mapping for a context rule on this smart
    /// account.
    pub fn get_signer_weights(
        e: &Env,
        context_rule: ContextRule,
        smart_account: Address,
    ) -> Map<Signer, u32> {
        weighted_threshold::get_signer_weights(e, &context_rule, &smart_account)
    }

    /// Updates the threshold for a context rule on this smart account.
    pub fn set_threshold(
        e: Env,
        threshold: u32,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        weighted_threshold::set_threshold(&e, threshold, &context_rule, &smart_account)
    }

    /// Updates a signer's weight for a context rule on this smart account.
    pub fn set_signer_weight(
        e: Env,
        signer: Signer,
        weight: u32,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        weighted_threshold::set_signer_weight(&e, &signer, weight, &context_rule, &smart_account)
    }

    /// Checks whether removing a signer from a context rule would leave the
    /// weight threshold reachable with the remaining signers' total weight.
    ///
    /// Computes the total weight of all signers in the context rule *except*
    /// the proposed removal target, then compares against the stored threshold.
    /// Returns `true` if the remaining weight is `>= threshold`, `false`
    /// otherwise.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `context_rule_id` - The context rule ID for this policy.
    /// * `smart_account` - The address of the smart account.
    /// * `signer_to_remove` - The signer whose weight would be subtracted.
    pub fn would_remain_reachable(
        e: &Env,
        context_rule_id: u32,
        smart_account: Address,
        signer_to_remove: Signer,
    ) -> bool {
        let threshold = weighted_threshold::get_threshold(e, context_rule_id, &smart_account);
        let signer_weights = weighted_threshold::get_signer_weights(
            e,
            &ContextRule {
                id: context_rule_id,
                context_type: ContextRuleType::Default,
                name: soroban_sdk::String::from_str(e, ""),
                signers: Vec::new(e),
                signer_ids: Vec::new(e),
                policies: Vec::new(e),
                policy_ids: Vec::new(e),
                valid_until: None,
            },
            &smart_account,
        );

        let mut remaining_weight: u32 = 0;
        for (signer, weight) in signer_weights.iter() {
            if signer != signer_to_remove {
                remaining_weight = remaining_weight.saturating_add(weight);
            }
        }

        remaining_weight >= threshold
    }
}
