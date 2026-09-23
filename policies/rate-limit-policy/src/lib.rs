//! Rolling-window call-count (rate-limit) policy for Latch smart accounts.
//!
//! Attach this policy to a `CallContract` context rule to cap how many
//! contract calls can be made through that rule within a rolling ledger window
//! — e.g. "at most 50 calls per rolling day" to a specific contract. Once the
//! limit is hit, further calls in that window are rejected until old entries
//! age out.
//!
//! This is an own-logic policy (not a thin wrapper around an OZ primitive) —
//! the rolling-window eviction pattern is adapted from OZ's `spending_limit`
//! module, but tracking call *counts* rather than transfer amounts. Any
//! `Context::Contract` call on the rule's `CallContract` target increments the
//! counter, regardless of function name.
//!
//! # How it works
//!
//! - `install` — called once per `(smart_account, context_rule)` pair, with a
//!   `max_calls` (max number of calls allowed in the window) and
//!   `period_ledgers` (the rolling window size). Rejects a non-`CallContract`
//!   context rule, or a zero max_calls/period.
//! - `enforce` — called on every authorized call against the rule; evicts
//!   call entries older than `period_ledgers`, checks whether the count would
//!   exceed `max_calls`, and records the call if not.
//! - `set_max_calls` / `get_rate_limit_data` — read or change the stored limit
//!   (and inspect call history) after installation.
//! - `uninstall` — removes all stored call history and the limit.
//!
//! # Important Constraints
//!
//! - **Counts any `Context::Contract` call** on the rule's `CallContract`
//!   target, regardless of function name. Per-function granularity is out of
//!   scope for v1.
//! - **One contract per installation.** Requiring a `CallContract` context
//!   rule pins each installed policy to a single target contract. Limiting
//!   calls across multiple contracts needs a separate context rule per
//!   contract.
//! - **Call history is capped** at `MAX_HISTORY_ENTRIES` (1000) entries per
//!   `(smart_account, context_rule)`; exceeding it panics with
//!   `HistoryCapacityExceeded` rather than silently dropping old data.
#![no_std]

mod rate_limit;

pub use rate_limit::{
    RateLimitAccountParams, RateLimitData, RateLimitError, RateLimitStorageKey,
};
use soroban_sdk::{auth::Context, contract, contractimpl, Address, Env, Vec};
use stellar_accounts::{
    policies::Policy,
    smart_account::{ContextRule, Signer},
};

#[contract]
pub struct RateLimitPolicy;

#[contractimpl]
impl Policy for RateLimitPolicy {
    type AccountParams = RateLimitAccountParams;

    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        rate_limit::enforce(e, &context, &authenticated_signers, &context_rule, &smart_account)
    }

    fn install(
        e: &Env,
        install_params: Self::AccountParams,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        rate_limit::install(e, &install_params, &context_rule, &smart_account)
    }

    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        rate_limit::uninstall(e, &context_rule, &smart_account)
    }
}

#[contractimpl]
impl RateLimitPolicy {
    pub fn get_rate_limit_data(
        e: &Env,
        context_rule_id: u32,
        smart_account: Address,
    ) -> rate_limit::RateLimitData {
        rate_limit::get_rate_limit_data(e, context_rule_id, &smart_account)
    }

    pub fn set_max_calls(
        e: Env,
        max_calls: u32,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        rate_limit::set_max_calls(&e, max_calls, &context_rule, &smart_account)
    }
}

#[cfg(test)]
mod test;
