//! Simple (unweighted) M-of-N threshold policy for Latch smart accounts.
//!
//! Attach this policy to a context rule to require that at least `threshold`
//! of its signers authenticate before an action is authorized — a standard
//! M-of-N multisig, with every signer counting equally (contrast with
//! `weighted-threshold-policy`, where some signers can outweigh others).
//!
//! This is a thin `#[contract]` wrapper around OZ's `simple_threshold` policy
//! (`stellar_accounts::policies::simple_threshold`) — all real logic lives
//! upstream, this crate only supplies the deployable contract shell the
//! factory and smart accounts can actually install and call.
//!
//! # How it works
//!
//! - `install` — called once per `(smart_account, context_rule)` pair, with a
//!   `threshold` (the number of signers required). Rejects `threshold == 0` or
//!   `threshold > ` the rule's current signer count.
//! - `enforce` — called during authorization; counts how many of the rule's
//!   signers actually authenticated for this call, and panics unless that count
//!   is `>= threshold`.
//! - `set_threshold` / `get_threshold` — read or change the stored threshold
//!   after installation.
//! - `uninstall` — removes the stored threshold entirely.
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
    /// Returns the stored threshold for a context rule on this smart account.
    pub fn get_threshold(e: &Env, context_rule_id: u32, smart_account: Address) -> u32 {
        simple_threshold::get_threshold(e, context_rule_id, &smart_account)
    }

    /// Updates the threshold for a context rule on this smart account.
    pub fn set_threshold(
        e: Env,
        threshold: u32,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        simple_threshold::set_threshold(&e, threshold, &context_rule, &smart_account)
    }

    /// Checks whether removing a signer from a context rule would leave the
    /// threshold reachable with the remaining signers.
    ///
    /// Returns `true` if `remaining_signer_count >= stored_threshold`, `false`
    /// otherwise. A `false` result means removing the proposed signer would
    /// make it impossible to satisfy the threshold, permanently blocking
    /// authorization for this context rule.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `context_rule_id` - The context rule ID for this policy.
    /// * `smart_account` - The address of the smart account.
    /// * `_signer_to_remove` - Unused; included for interface compatibility
    ///   with `weighted-threshold-policy`.
    /// * `remaining_signer_count` - The number of signers that would remain
    ///   after the proposed removal, computed by the calling smart account.
    pub fn would_remain_reachable(
        e: &Env,
        context_rule_id: u32,
        smart_account: Address,
        _signer_to_remove: Signer,
        remaining_signer_count: u32,
    ) -> bool {
        let threshold = simple_threshold::get_threshold(e, context_rule_id, &smart_account);
        remaining_signer_count >= threshold
    }
}

#[cfg(test)]
mod test;
