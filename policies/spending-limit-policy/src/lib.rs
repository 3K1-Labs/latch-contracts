//! Rolling-window spending limit policy for Latch smart accounts.
//!
//! Attach this policy to a `CallContract` context rule to cap how much can
//! be transferred through that contract within a rolling ledger window —
//! e.g. "at most 10 XLM per rolling day" out of a specific token contract.
//! Once the limit is hit, further transfers in that window are rejected
//! until old entries age out.
//!
//! This is a thin `#[contract]` wrapper around OZ's `spending_limit` policy
//! (`stellar_accounts::policies::spending_limit`) — all real logic lives
//! upstream, this crate only supplies the deployable contract shell the
//! factory and smart accounts can actually install and call.
//!
//! # How it works
//!
//! - `install` — called once per `(smart_account, context_rule)` pair, with a
//!   `spending_limit` (max total, in stroops) and `period_ledgers` (the rolling
//!   window size). Rejects a non-`CallContract` context rule, or a non-positive
//!   limit/period.
//! - `enforce` — called on every authorized call against the rule; evicts
//!   spending entries older than `period_ledgers`, sums what remains, and
//!   panics if adding this call's amount would exceed `spending_limit`.
//!   Otherwise records the entry and updates the running total.
//! - `set_spending_limit` / `get_spending_limit_data` — read or change the
//!   stored limit (and inspect spend history) after installation.
//! - `uninstall` — removes all stored spending history and the limit.
//!
//! # Important Constraints
//!
//! - **Only intercepts calls literally named `transfer`**, with the amount read
//!   as the *third* positional argument — the standard SEP-41 token
//!   `transfer(from, to, amount)` shape. Any other function call on the rule is
//!   unconditionally rejected, not silently allowed through.
//! - **One contract per installation.** Requiring a `CallContract` context rule
//!   pins each installed policy to a single target contract, so all tracked
//!   transfers share one denomination. Limiting spend across several different
//!   token contracts needs a separate context rule (and policy install) per
//!   contract.
//! - **Spend history is capped** at `MAX_HISTORY_ENTRIES` (1000) entries per
//!   `(smart_account, context_rule)`; exceeding it panics with
//!   `HistoryCapacityExceeded` rather than silently dropping old data.
#![no_std]

use soroban_sdk::{auth::Context, contract, contractimpl, Address, Env, Vec};
use stellar_accounts::{
    policies::{spending_limit, spending_limit::SpendingLimitAccountParams, Policy},
    smart_account::{ContextRule, Signer},
};

#[contract]
pub struct SpendingLimitPolicy;

#[contractimpl]
impl Policy for SpendingLimitPolicy {
    type AccountParams = SpendingLimitAccountParams;

    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        spending_limit::enforce(e, &context, &authenticated_signers, &context_rule, &smart_account)
    }

    fn install(
        e: &Env,
        install_params: Self::AccountParams,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        spending_limit::install(e, &install_params, &context_rule, &smart_account)
    }

    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        spending_limit::uninstall(e, &context_rule, &smart_account)
    }
}

#[contractimpl]
impl SpendingLimitPolicy {
    pub fn get_spending_limit_data(
        e: &Env,
        context_rule_id: u32,
        smart_account: Address,
    ) -> spending_limit::SpendingLimitData {
        spending_limit::get_spending_limit_data(e, context_rule_id, &smart_account)
    }

    pub fn set_spending_limit(
        e: Env,
        spending_limit: i128,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        spending_limit::set_spending_limit(&e, spending_limit, &context_rule, &smart_account)
    }
}
