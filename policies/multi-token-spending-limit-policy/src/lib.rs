//! Oracle-denominated multi-token spending limit policy for Latch smart
//! accounts.
//!
//! Attach this policy to a `CallContract` context rule to cap, in USD terms,
//! how much can be transferred across a set of allowed token contracts
//! within a rolling ledger window. Each `transfer` call is priced through an
//! oracle at enforcement time, so unlike a single-token spending limit, the
//! cap is meaningful even when the account spends from several different
//! tokens.
//!
//! # Oracle trust model
//!
//! - The oracle address is fixed at `install` time and cannot be changed
//!   afterwards.
//! - A price is only trusted for `MAX_STALENESS_LEDGERS` worth of ledgers
//!   past its `updated_at` timestamp; anything older is rejected.
//! - The policy fails closed: an oracle call that reverts, returns a
//!   malformed `PriceData`, or returns a stale price blocks the transfer
//!   rather than letting it through.
#![no_std]

use soroban_sdk::{
    auth::{Context, ContractContext},
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, Symbol,
    TryFromVal, Vec,
};
use stellar_accounts::{
    policies::Policy,
    smart_account::{ContextRule, ContextRuleType, Signer},
};

/// Error codes for the multi-token spending limit policy.
///
/// This is a standalone, independently-deployed contract, not a module of
/// the upstream `stellar-accounts` crate, so it is not part of that crate's
/// shared error-numbering convention. Numbering starts fresh at `1`.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// Only the `CallContract` context rule type is allowed.
    InvalidContextRule = 1,
    /// `spending_limit_usd` was not positive, `period_ledgers` was zero, or
    /// `allowed_tokens` was empty.
    InvalidInstallParams = 2,
    /// The policy was already installed for this smart account and context
    /// rule.
    AlreadyInstalled = 3,
    /// The smart account does not have this policy installed for this
    /// context rule.
    NotInstalled = 4,
    /// The target contract of the call is not in the installed allowlist.
    TokenNotAllowed = 5,
    /// The context was not a `transfer` call, or its arguments could not be
    /// parsed.
    InvalidTransferArgs = 6,
    /// The oracle's price data is older than `MAX_STALENESS_LEDGERS`.
    StaleOraclePrice = 7,
    /// Adding this transfer would exceed the rolling spending limit.
    SpendingLimitExceeded = 8,
    /// The spending history reached `MAX_HISTORY_ENTRIES`.
    HistoryCapacityExceeded = 9,
}

/// Price feed shape returned by the configured oracle's `get_price` function.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceData {
    pub price_usd: i128,
    pub updated_at: u64,
}

/// Installation parameters for the multi-token spending limit policy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MultiTokenSpendingLimitAccountParams {
    /// The maximum amount, in USD (7 decimals, matching the oracle), that
    /// can be spent across `allowed_tokens` within `period_ledgers`.
    pub spending_limit_usd: i128,
    /// The rolling window size, in ledgers, over which the limit applies.
    pub period_ledgers: u32,
    /// The oracle contract queried for each token's USD price. Fixed at
    /// install time.
    pub oracle_address: Address,
    /// The set of token contracts this policy tracks spend across.
    pub allowed_tokens: Vec<Address>,
}

/// Individual spending entry for rolling-window tracking.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SpendingEntry {
    pub amount_usd: i128,
    pub ledger: u32,
}

/// Internal storage structure for a single `(smart_account, context_rule)`
/// installation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyData {
    pub spending_limit_usd: i128,
    pub period_ledgers: u32,
    pub oracle_address: Address,
    pub allowed_tokens: Vec<Address>,
    pub spending_history: Vec<SpendingEntry>,
    pub cached_total_spent_usd: i128,
}

#[contracttype]
pub enum DataKey {
    AccountContext(Address, u32),
}

// ################## CONSTANTS ##################

const DAY_IN_LEDGERS: u32 = 17280;
const POLICY_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
const POLICY_TTL_THRESHOLD: u32 = POLICY_EXTEND_AMOUNT - DAY_IN_LEDGERS;

/// Maximum number of spending entries to keep in history. Bounds the storage
/// size and the linear eviction scan performed on every `enforce` call.
const MAX_HISTORY_ENTRIES: u32 = 1000;

/// Maximum age, in ledgers, that an oracle price is trusted for before a
/// transfer is rejected. Fails closed: a stale price blocks spending rather
/// than letting it through at a possibly-outdated valuation.
const MAX_STALENESS_LEDGERS: u32 = 100;

/// Approximate ledger close time, in seconds, used to convert
/// `MAX_STALENESS_LEDGERS` into a timestamp-based staleness bound, since the
/// oracle reports `updated_at` as a ledger timestamp rather than a ledger
/// sequence.
const LEDGER_CLOSE_TIME_SECS: u64 = 5;

const ORACLE_FN: Symbol = Symbol::short("get_price");

// ################## HELPERS ##################

/// Loads the stored policy data for a `(smart_account, context_rule)` pair,
/// refreshing its TTL, or fails closed with [`Error::NotInstalled`].
fn get_policy_data(e: &Env, context_rule_id: u32, smart_account: &Address) -> PolicyData {
    let key = DataKey::AccountContext(smart_account.clone(), context_rule_id);
    e.storage()
        .persistent()
        .get(&key)
        .inspect(|_| {
            e.storage().persistent().extend_ttl(&key, POLICY_TTL_THRESHOLD, POLICY_EXTEND_AMOUNT);
        })
        .unwrap_or_else(|| panic_with_error!(e, Error::NotInstalled))
}

/// Evicts spending entries older than the rolling window and returns the
/// total USD amount removed, so the caller can keep `cached_total_spent_usd`
/// in sync without re-summing the whole history.
fn cleanup_old_entries(
    spending_history: &mut Vec<SpendingEntry>,
    current_ledger: u32,
    period_ledgers: u32,
) -> i128 {
    let cutoff_ledger = current_ledger.saturating_sub(period_ledgers);
    let mut removed_total: i128 = 0;

    // Entries are appended in increasing ledger order, so the oldest ones
    // are always at the front.
    while let Some(entry) = spending_history.get(0) {
        if entry.ledger <= cutoff_ledger {
            removed_total = removed_total.saturating_add(entry.amount_usd);
            spending_history.pop_front();
        } else {
            break;
        }
    }

    removed_total
}

#[contract]
pub struct MultiTokenSpendingLimitPolicy;

#[contractimpl]
impl Policy for MultiTokenSpendingLimitPolicy {
    type AccountParams = MultiTokenSpendingLimitAccountParams;

    /// Installs the policy on a smart account. Only `CallContract` context
    /// rules are allowed. Requires authorization from the smart account.
    ///
    /// # Errors
    ///
    /// * [`Error::InvalidContextRule`] - When the context rule type is not
    ///   `CallContract`.
    /// * [`Error::InvalidInstallParams`] - When `spending_limit_usd` is not
    ///   positive, `period_ledgers` is zero, or `allowed_tokens` is empty.
    /// * [`Error::AlreadyInstalled`] - When the policy was already installed
    ///   for this smart account and context rule.
    fn install(
        e: &Env,
        install_params: Self::AccountParams,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        smart_account.require_auth();

        if !matches!(context_rule.context_type, ContextRuleType::CallContract(_)) {
            panic_with_error!(e, Error::InvalidContextRule)
        }

        if install_params.spending_limit_usd <= 0
            || install_params.period_ledgers == 0
            || install_params.allowed_tokens.is_empty()
        {
            panic_with_error!(e, Error::InvalidInstallParams)
        }

        let key = DataKey::AccountContext(smart_account.clone(), context_rule.id);

        if e.storage().persistent().has(&key) {
            panic_with_error!(e, Error::AlreadyInstalled)
        }

        let data = PolicyData {
            spending_limit_usd: install_params.spending_limit_usd,
            period_ledgers: install_params.period_ledgers,
            oracle_address: install_params.oracle_address,
            allowed_tokens: install_params.allowed_tokens,
            spending_history: Vec::new(e),
            cached_total_spent_usd: 0,
        };

        e.storage().persistent().set(&key, &data);
    }

    /// Removes all stored policy data for this smart account and context
    /// rule. Requires authorization from the smart account.
    ///
    /// # Errors
    ///
    /// * [`Error::NotInstalled`] - When the policy is not installed for the
    ///   given smart account and context rule.
    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        smart_account.require_auth();

        let key = DataKey::AccountContext(smart_account, context_rule.id);

        if !e.storage().persistent().has(&key) {
            panic_with_error!(e, Error::NotInstalled)
        }

        e.storage().persistent().remove(&key);
    }

    /// Enforces the rolling USD spending limit across the allowed tokens.
    /// Requires authorization from the smart account.
    ///
    /// # Errors
    ///
    /// * [`Error::InvalidTransferArgs`] - When the context is not a
    ///   `Context::Contract` invocation of `transfer`, the amount argument
    ///   is missing or malformed, or the amount is negative.
    /// * [`Error::TokenNotAllowed`] - When the target contract is not in the
    ///   installed allowlist.
    /// * [`Error::StaleOraclePrice`] - When the oracle's price is older than
    ///   `MAX_STALENESS_LEDGERS`.
    /// * [`Error::SpendingLimitExceeded`] - When adding this transfer would
    ///   exceed the rolling spending limit.
    /// * [`Error::HistoryCapacityExceeded`] - When the spending history has
    ///   reached `MAX_HISTORY_ENTRIES`.
    /// * refer to [`get_policy_data`] errors.
    fn enforce(
        e: &Env,
        context: Context,
        _authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        smart_account.require_auth();

        let Context::Contract(ContractContext { contract: target, fn_name, args }) = context else {
            panic_with_error!(e, Error::InvalidTransferArgs)
        };

        if fn_name != Symbol::short("transfer") {
            panic_with_error!(e, Error::InvalidTransferArgs)
        }

        // A standard SEP-41 `transfer` is `transfer(from, to, amount)`; the
        // amount is the third argument (index 2).
        let amount_val =
            args.get(2).unwrap_or_else(|| panic_with_error!(e, Error::InvalidTransferArgs));
        let Ok(amount) = i128::try_from_val(e, &amount_val) else {
            panic_with_error!(e, Error::InvalidTransferArgs)
        };
        if amount < 0 {
            panic_with_error!(e, Error::InvalidTransferArgs)
        }

        let mut data = get_policy_data(e, context_rule.id, &smart_account);

        if !data.allowed_tokens.contains(&target) {
            panic_with_error!(e, Error::TokenNotAllowed)
        }

        // Fails closed: if the oracle call reverts or the return value
        // can't be decoded as `PriceData`, `invoke_contract` panics and the
        // transfer is rejected rather than allowed through.
        let price_data: PriceData = e.invoke_contract(
            &data.oracle_address,
            &ORACLE_FN,
            soroban_sdk::vec![e, target.to_val()],
        );

        let current_timestamp = e.ledger().timestamp();
        let current_ledger = e.ledger().sequence();

        let max_staleness_secs = MAX_STALENESS_LEDGERS as u64 * LEDGER_CLOSE_TIME_SECS;
        if current_timestamp.saturating_sub(price_data.updated_at) > max_staleness_secs {
            panic_with_error!(e, Error::StaleOraclePrice)
        }

        let amount_usd = amount.saturating_mul(price_data.price_usd).saturating_div(100_000_000);

        // Clean up old entries outside the rolling window before checking
        // the limit, so the cached total matches the live window.
        let removed_amount =
            cleanup_old_entries(&mut data.spending_history, current_ledger, data.period_ledgers);
        data.cached_total_spent_usd = data.cached_total_spent_usd.saturating_sub(removed_amount);

        if data.cached_total_spent_usd.saturating_add(amount_usd) > data.spending_limit_usd {
            panic_with_error!(e, Error::SpendingLimitExceeded)
        }

        if data.spending_history.len() >= MAX_HISTORY_ENTRIES {
            panic_with_error!(e, Error::HistoryCapacityExceeded)
        }

        data.spending_history.push_back(SpendingEntry { amount_usd, ledger: current_ledger });
        data.cached_total_spent_usd = data.cached_total_spent_usd.saturating_add(amount_usd);

        let key = DataKey::AccountContext(smart_account, context_rule.id);
        e.storage().persistent().set(&key, &data);
    }
}

#[contractimpl]
impl MultiTokenSpendingLimitPolicy {
    /// Retrieves the stored policy data for a smart account's context rule.
    ///
    /// # Errors
    ///
    /// * [`Error::NotInstalled`] - When the policy is not installed for the
    ///   given smart account and context rule.
    pub fn get_policy_data(e: &Env, context_rule_id: u32, smart_account: Address) -> PolicyData {
        get_policy_data(e, context_rule_id, &smart_account)
    }
}

#[cfg(test)]
mod test;
