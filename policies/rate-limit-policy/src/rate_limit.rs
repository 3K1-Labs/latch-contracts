//! # Rate-Limit Policy Module
//!
//! This policy implements a rolling-window call-count rate limit. Any
//! `Context::Contract` invocation on the rule's `CallContract` target
//! increments a shared counter; once the counter reaches `max_calls` within
//! the rolling `period_ledgers` window, further calls are rejected until old
//! entries age out.
//!
//! ## Rolling window semantics
//!
//! The rolling window keeps only the last `period_ledgers` worth of call
//! entries. Entries whose ledger sequence is **less than or equal to**
//! `current_ledger - period_ledgers` are evicted before a new call is
//! evaluated, matching the eviction logic in OZ's `spending_limit` module.
//!
//! Example where `P` = `period_ledgers`, `C` = `current_ledger`:
//!
//! ```text
//! ... C-P-2 C-P-1 C-P C-P+1 ... C-1 C
//!    [evicted] [evicted] |<------ kept ----->|
//!                        ^ cutoff (exclusive window start)
//!
//! ... 78    79    80    81   ... 99  100
//!    [<=80 evicted]     |<------- kept ------>|
//!                       ^ cutoff when `C = 100`, `P = 20`
//! ```
use soroban_sdk::{
    auth::{Context, ContractContext},
    contracterror, contractevent, contracttype, panic_with_error, Address, Env, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

// ################## EVENTS ##################

/// Event emitted when the rate limit policy is enforced (a call is allowed).
#[contractevent]
#[derive(Clone)]
pub struct RateLimitEnforced {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub calls_in_period: u32,
}

/// Event emitted when a rate limit policy is installed.
#[contractevent]
#[derive(Clone, Debug)]
pub struct RateLimitInstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub max_calls: u32,
    pub period_ledgers: u32,
}

/// Event emitted when the max_calls value is changed.
#[contractevent]
#[derive(Clone, Debug)]
pub struct RateLimitChanged {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub max_calls: u32,
}

/// Event emitted when a rate limit policy is uninstalled.
#[contractevent]
#[derive(Clone, Debug)]
pub struct RateLimitUninstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
}

// ################## TYPES ##################

/// Installation parameters for the rate-limit policy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RateLimitAccountParams {
    /// Maximum number of `Context::Contract` calls allowed within the rolling
    /// window.
    pub max_calls: u32,
    /// The rolling window size in ledgers.
    pub period_ledgers: u32,
}

/// Internal storage structure for rate-limit tracking.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RateLimitData {
    /// Maximum calls allowed within the rolling window.
    pub max_calls: u32,
    /// The rolling window size in ledgers.
    pub period_ledgers: u32,
    /// History of call ledger sequences within the current window.
    pub call_history: Vec<CallEntry>,
    /// Cached count of all entries in `call_history`. Kept consistent with
    /// `call_history.len()` to avoid recomputing the length on every enforce.
    pub cached_call_count: u32,
}

/// Individual call entry for rate-limit tracking.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CallEntry {
    /// The ledger sequence when this call occurred.
    pub ledger_sequence: u32,
}

/// Error codes for rate-limit policy operations.
///
/// This is a standalone, independently-deployed contract, not a module of the
/// upstream `stellar-accounts` crate, so it is not part of that crate's shared
/// error-numbering convention. Numbering starts fresh at `1`.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RateLimitError {
    /// The smart account does not have a rate-limit policy installed for this
    /// context rule.
    SmartAccountNotInstalled = 1,
    /// Only the `CallContract` context rule type is allowed.
    OnlyCallContractAllowed = 2,
    /// `max_calls` was zero or `period_ledgers` was zero.
    InvalidLimitOrPeriod = 3,
    /// The rate limit for the rolling window has been exceeded.
    RateLimitExceeded = 4,
    /// The call history has reached maximum capacity.
    HistoryCapacityExceeded = 5,
    /// The policy was already installed for this smart account and context
    /// rule.
    AlreadyInstalled = 6,
    /// The context is not a `Context::Contract` invocation, or there are no
    /// authenticated signers.
    NotAllowed = 7,
}

/// Storage keys for rate-limit policy data.
#[contracttype]
pub enum RateLimitStorageKey {
    /// Storage key for rate-limit data of a smart account context rule.
    AccountContext(Address, u32),
}

// ################## CONSTANTS ##################

const DAY_IN_LEDGERS: u32 = 17_280;
pub const RATE_LIMIT_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub const RATE_LIMIT_TTL_THRESHOLD: u32 = RATE_LIMIT_EXTEND_AMOUNT - DAY_IN_LEDGERS;

/// Maximum number of call entries to keep in history per
/// `(smart_account, context_rule)`. Prevents unbounded storage growth.
/// Matches `MAX_HISTORY_ENTRIES` in OZ's `spending_limit` module.
pub const MAX_HISTORY_ENTRIES: u32 = 1000;

// ################## QUERY STATE ##################

/// Retrieves the rate-limit data for a smart account's rate-limit policy.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `context_rule_id` - The context rule ID for this policy.
/// * `smart_account` - The address of the smart account.
///
/// # Errors
///
/// * [`RateLimitError::SmartAccountNotInstalled`] - When the smart account
///   does not have a rate-limit policy installed for this context rule.
pub fn get_rate_limit_data(e: &Env, context_rule_id: u32, smart_account: &Address) -> RateLimitData {
    let key = RateLimitStorageKey::AccountContext(smart_account.clone(), context_rule_id);
    e.storage()
        .persistent()
        .get::<_, RateLimitData>(&key)
        .inspect(|_| {
            e.storage().persistent().extend_ttl(
                &key,
                RATE_LIMIT_TTL_THRESHOLD,
                RATE_LIMIT_EXTEND_AMOUNT,
            );
        })
        .unwrap_or_else(|| panic_with_error!(e, RateLimitError::SmartAccountNotInstalled))
}

// ################## CHANGE STATE ##################

/// Enforces the rate-limit policy: evicts stale entries from the rolling
/// window, rejects the call if the count would exceed `max_calls`, otherwise
/// records the call and updates the cached count. Requires authorization from
/// the smart account.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `context` - The authorization context.
/// * `authenticated_signers` - The list of authenticated signers.
/// * `context_rule` - The context rule for this policy.
/// * `smart_account` - The address of the smart account.
///
/// # Errors
///
/// * [`RateLimitError::NotAllowed`] - When there are no authenticated signers
///   or the context is not a `Context::Contract` invocation.
/// * [`RateLimitError::RateLimitExceeded`] - When the call count within the
///   rolling window would exceed `max_calls`.
/// * [`RateLimitError::HistoryCapacityExceeded`] - When the call history has
///   reached `MAX_HISTORY_ENTRIES`.
/// * refer to [`get_rate_limit_data`] errors.
///
/// # Events
///
/// * topics - `["rate_limit_enforced", smart_account: Address]`
/// * data - `[context_rule_id: u32, calls_in_period: u32]`
pub fn enforce(
    e: &Env,
    context: &Context,
    authenticated_signers: &Vec<Signer>,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    smart_account.require_auth();

    if authenticated_signers.is_empty() {
        panic_with_error!(e, RateLimitError::NotAllowed)
    }

    // Only `Context::Contract` calls count — reject anything else.
    let Context::Contract(ContractContext { .. }) = context else {
        panic_with_error!(e, RateLimitError::NotAllowed)
    };

    let key = RateLimitStorageKey::AccountContext(smart_account.clone(), context_rule.id);
    let mut data = get_rate_limit_data(e, context_rule.id, smart_account);
    let current_ledger = e.ledger().sequence();

    // Evict stale entries outside the rolling window before counting.
    let removed = cleanup_old_entries(&mut data.call_history, current_ledger, data.period_ledgers);
    data.cached_call_count -= removed;

    // Reject if recording this call would exceed the limit.
    if data.cached_call_count >= data.max_calls {
        panic_with_error!(e, RateLimitError::RateLimitExceeded)
    }

    // Guard against unbounded storage growth.
    if data.call_history.len() >= MAX_HISTORY_ENTRIES {
        panic_with_error!(e, RateLimitError::HistoryCapacityExceeded)
    }

    // Record the call.
    data.call_history.push_back(CallEntry { ledger_sequence: current_ledger });
    data.cached_call_count += 1;

    e.storage().persistent().set(&key, &data);

    RateLimitEnforced {
        smart_account: smart_account.clone(),
        context_rule_id: context_rule.id,
        calls_in_period: data.cached_call_count,
    }
    .publish(e);
}

/// Installs the rate-limit policy on a smart account. Only `CallContract`
/// context type is allowed, since the call counter is only meaningful when
/// pinned to one target contract. Requires authorization from the smart
/// account.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `params` - Installation parameters containing `max_calls` and
///   `period_ledgers`.
/// * `context_rule` - The context rule for this policy.
/// * `smart_account` - The address of the smart account.
///
/// # Errors
///
/// * [`RateLimitError::OnlyCallContractAllowed`] - When the context rule type
///   is not `CallContract`.
/// * [`RateLimitError::InvalidLimitOrPeriod`] - When `max_calls` is zero or
///   `period_ledgers` is zero.
/// * [`RateLimitError::AlreadyInstalled`] - When the policy was already
///   installed for this smart account and context rule.
///
/// # Events
///
/// * topics - `["rate_limit_installed", smart_account: Address]`
/// * data - `[context_rule_id: u32, max_calls: u32, period_ledgers: u32]`
pub fn install(
    e: &Env,
    params: &RateLimitAccountParams,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    smart_account.require_auth();

    if !matches!(context_rule.context_type, ContextRuleType::CallContract(_)) {
        panic_with_error!(e, RateLimitError::OnlyCallContractAllowed)
    }

    if params.max_calls == 0 || params.period_ledgers == 0 {
        panic_with_error!(e, RateLimitError::InvalidLimitOrPeriod)
    }

    let key = RateLimitStorageKey::AccountContext(smart_account.clone(), context_rule.id);

    if e.storage().persistent().has(&key) {
        panic_with_error!(e, RateLimitError::AlreadyInstalled)
    }

    let data = RateLimitData {
        max_calls: params.max_calls,
        period_ledgers: params.period_ledgers,
        call_history: Vec::new(e),
        cached_call_count: 0,
    };

    e.storage().persistent().set(&key, &data);

    RateLimitInstalled {
        smart_account: smart_account.clone(),
        context_rule_id: context_rule.id,
        max_calls: params.max_calls,
        period_ledgers: params.period_ledgers,
    }
    .publish(e);
}

/// Updates the `max_calls` limit for an already-installed rate-limit policy.
/// Requires authorization from the smart account.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `max_calls` - The new maximum call count per period.
/// * `context_rule` - The context rule for this policy.
/// * `smart_account` - The address of the smart account.
///
/// # Errors
///
/// * [`RateLimitError::InvalidLimitOrPeriod`] - When `max_calls` is zero.
/// * refer to [`get_rate_limit_data`] errors.
///
/// # Events
///
/// * topics - `["rate_limit_changed", smart_account: Address]`
/// * data - `[context_rule_id: u32, max_calls: u32]`
pub fn set_max_calls(e: &Env, max_calls: u32, context_rule: &ContextRule, smart_account: &Address) {
    smart_account.require_auth();

    if max_calls == 0 {
        panic_with_error!(e, RateLimitError::InvalidLimitOrPeriod)
    }

    let key = RateLimitStorageKey::AccountContext(smart_account.clone(), context_rule.id);
    let mut data = get_rate_limit_data(e, context_rule.id, smart_account);
    data.max_calls = max_calls;

    e.storage().persistent().set(&key, &data);

    RateLimitChanged {
        smart_account: smart_account.clone(),
        context_rule_id: context_rule.id,
        max_calls,
    }
    .publish(e);
}

/// Uninstalls the rate-limit policy from a smart account, removing all stored
/// call history and the limit. Requires authorization from the smart account.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `context_rule` - The context rule for this policy.
/// * `smart_account` - The address of the smart account.
///
/// # Errors
///
/// * [`RateLimitError::SmartAccountNotInstalled`] - When the policy is not
///   installed for the given smart account and context rule.
///
/// # Events
///
/// * topics - `["rate_limit_uninstalled", smart_account: Address]`
/// * data - `[context_rule_id: u32]`
pub fn uninstall(e: &Env, context_rule: &ContextRule, smart_account: &Address) {
    smart_account.require_auth();

    let key = RateLimitStorageKey::AccountContext(smart_account.clone(), context_rule.id);

    if !e.storage().persistent().has(&key) {
        panic_with_error!(e, RateLimitError::SmartAccountNotInstalled)
    }

    e.storage().persistent().remove(&key);

    RateLimitUninstalled { smart_account: smart_account.clone(), context_rule_id: context_rule.id }
        .publish(e);
}

// ################## HELPER FUNCTIONS ##################

/// Removes call entries that are outside the rolling window period. Returns
/// the number of entries removed, which must be subtracted from
/// `cached_call_count`.
///
/// Entries are evicted from the front of the vector while their
/// `ledger_sequence <= current_ledger - period_ledgers`, exactly matching
/// the cutoff semantics of OZ's `spending_limit::cleanup_old_entries`.
///
/// # Arguments
///
/// * `call_history` - Mutable reference to the call history vector.
/// * `current_ledger` - The current ledger sequence.
/// * `period_ledgers` - The rolling window size in ledgers.
///
/// # Returns
///
/// The count of removed entries.
fn cleanup_old_entries(
    call_history: &mut Vec<CallEntry>,
    current_ledger: u32,
    period_ledgers: u32,
) -> u32 {
    let cutoff_ledger = current_ledger.saturating_sub(period_ledgers);
    let mut removed_count = 0u32;

    while let Some(entry) = call_history.get(0) {
        if entry.ledger_sequence <= cutoff_ledger {
            removed_count += 1;
            call_history.pop_front();
        } else {
            break;
        }
    }

    removed_count
}
