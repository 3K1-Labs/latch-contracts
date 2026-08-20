//! # Method Allowlist Policy Module
//!
//! This policy restricts a context rule's signers to a fixed set of
//! contract function names. It is the building block behind Latch session
//! keys: a `ContextRule` scoped to `CallContract(dapp_contract)` combines an
//! ephemeral signer with this policy so the session key may only invoke the
//! allowlisted functions on that one contract.
//!
//! Session expiry and revocation are intentionally not reimplemented here —
//! they are already provided by the smart account's own `ContextRule`
//! machinery (`valid_until` and `remove_context_rule`). This policy only
//! adds the one thing the account model doesn't already provide: narrowing
//! a `CallContract` rule down to specific function names.
use soroban_sdk::{
    auth::{Context, ContractContext},
    contracterror, contractevent, contracttype, panic_with_error, Address, Env, Symbol, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

/// Event emitted when a session policy is enforced.
#[contractevent]
#[derive(Clone)]
pub struct SessionEnforced {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub fn_name: Symbol,
}

/// Event emitted when a session policy is installed.
#[contractevent]
#[derive(Clone, Debug)]
pub struct SessionInstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub allowed_fns: Vec<Symbol>,
}

/// Event emitted when a session policy is uninstalled.
#[contractevent]
#[derive(Clone, Debug)]
pub struct SessionUninstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
}

/// Installation parameters for the session (method allowlist) policy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SessionAccountParams {
    /// The function names on the rule's `CallContract` target that this
    /// session key is permitted to invoke.
    pub allowed_fns: Vec<Symbol>,
}

/// Internal storage structure for the session allowlist.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SessionData {
    /// The function names this session key is permitted to invoke.
    pub allowed_fns: Vec<Symbol>,
}

/// Error codes for session policy operations.
///
/// This is a standalone, independently-deployed contract, not a module of
/// the upstream `stellar-accounts` crate, so it is not part of that crate's
/// shared error-numbering convention (see its `CLAUDE.md`). Numbering starts
/// fresh at `1`.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum SessionError {
    /// The smart account does not have a session policy installed for this
    /// context rule.
    SmartAccountNotInstalled = 1,
    /// Only the `CallContract` context rule type is allowed.
    OnlyCallContractAllowed = 2,
    /// `allowed_fns` was empty or exceeded `MAX_ALLOWED_FNS`.
    InvalidAllowedFns = 3,
    /// The context's function name is not in the allowlist, or the context
    /// is not a `Context::Contract` variant.
    MethodNotAllowed = 4,
    /// The policy was already installed for this smart account and context
    /// rule.
    AlreadyInstalled = 5,
}

/// Storage keys for session policy data.
#[contracttype]
pub enum SessionStorageKey {
    /// Storage key for the allowlist of a smart account context rule.
    AccountContext(Address, u32),
}

// ################## CONSTANTS ##################

const DAY_IN_LEDGERS: u32 = 17280;
pub const SESSION_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub const SESSION_TTL_THRESHOLD: u32 = SESSION_EXTEND_AMOUNT - DAY_IN_LEDGERS;

/// Maximum number of allowed function names per session policy.
/// Bounds the linear scan performed in `enforce` and the storage size.
pub const MAX_ALLOWED_FNS: u32 = 10;

// ################## QUERY STATE ##################

/// Retrieves the allowlist for a smart account's session policy.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `context_rule_id` - The context rule ID for this policy.
/// * `smart_account` - The address of the smart account.
///
/// # Errors
///
/// * [`SessionError::SmartAccountNotInstalled`] - When the smart account does
///   not have a session policy installed.
pub fn get_allowed_fns(e: &Env, context_rule_id: u32, smart_account: &Address) -> Vec<Symbol> {
    let key = SessionStorageKey::AccountContext(smart_account.clone(), context_rule_id);
    e.storage()
        .persistent()
        .get::<_, SessionData>(&key)
        .inspect(|_| {
            e.storage().persistent().extend_ttl(&key, SESSION_TTL_THRESHOLD, SESSION_EXTEND_AMOUNT);
        })
        .map(|data| data.allowed_fns)
        .unwrap_or_else(|| panic_with_error!(e, SessionError::SmartAccountNotInstalled))
}

// ################## CHANGE STATE ##################

/// Enforces the session policy: the context must be a `Context::Contract`
/// invocation whose `fn_name` is in the installed allowlist. Requires
/// authorization from the smart account.
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
/// * [`SessionError::MethodNotAllowed`] - When there are no authenticated
///   signers, the context is not a `Context::Contract` invocation, or the
///   function name is not in the allowlist.
/// * refer to [`get_allowed_fns`] errors.
///
/// # Events
///
/// * topics - `["session_enforced", smart_account: Address]`
/// * data - `[context_rule_id: u32, fn_name: Symbol]`
pub fn enforce(
    e: &Env,
    context: &Context,
    authenticated_signers: &Vec<Signer>,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    // Require authorization from the smart_account
    smart_account.require_auth();

    if authenticated_signers.is_empty() {
        panic_with_error!(e, SessionError::MethodNotAllowed)
    }

    let allowed_fns = get_allowed_fns(e, context_rule.id, smart_account);

    match context {
        Context::Contract(ContractContext { fn_name, .. }) => {
            if !allowed_fns.contains(fn_name) {
                panic_with_error!(e, SessionError::MethodNotAllowed)
            }

            SessionEnforced {
                smart_account: smart_account.clone(),
                context_rule_id: context_rule.id,
                fn_name: fn_name.clone(),
            }
            .publish(e);
        }
        _ => panic_with_error!(e, SessionError::MethodNotAllowed),
    }
}

/// Installs the session policy on a smart account. Only `CallContract`
/// context type is allowed, since the allowlisted function names are only
/// meaningful when pinned to one target contract. Requires authorization
/// from the smart account.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `params` - Installation parameters containing the allowed function names.
/// * `context_rule` - The context rule for this policy.
/// * `smart_account` - The address of the smart account.
///
/// # Errors
///
/// * [`SessionError::OnlyCallContractAllowed`] - When the context rule type is
///   not `CallContract`.
/// * [`SessionError::InvalidAllowedFns`] - When `allowed_fns` is empty or
///   exceeds `MAX_ALLOWED_FNS`.
/// * [`SessionError::AlreadyInstalled`] - When the policy was already installed
///   for this smart account and context rule.
///
/// # Events
///
/// * topics - `["session_installed", smart_account: Address]`
/// * data - `[context_rule_id: u32, allowed_fns: Vec<Symbol>]`
pub fn install(
    e: &Env,
    params: &SessionAccountParams,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    // Require authorization from the smart_account
    smart_account.require_auth();

    if !matches!(context_rule.context_type, ContextRuleType::CallContract(_)) {
        panic_with_error!(e, SessionError::OnlyCallContractAllowed)
    }

    if params.allowed_fns.is_empty() || params.allowed_fns.len() > MAX_ALLOWED_FNS {
        panic_with_error!(e, SessionError::InvalidAllowedFns)
    }

    let key = SessionStorageKey::AccountContext(smart_account.clone(), context_rule.id);

    if e.storage().persistent().has(&key) {
        panic_with_error!(e, SessionError::AlreadyInstalled)
    }

    let data = SessionData { allowed_fns: params.allowed_fns.clone() };

    e.storage().persistent().set(&key, &data);

    SessionInstalled {
        smart_account: smart_account.clone(),
        context_rule_id: context_rule.id,
        allowed_fns: params.allowed_fns.clone(),
    }
    .publish(e);
}

/// Uninstalls the session policy from a smart account, removing all stored
/// allowlist data for the account and context rule. Requires authorization
/// from the smart account.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `context_rule` - The context rule for this policy.
/// * `smart_account` - The address of the smart account.
///
/// # Errors
///
/// * [`SessionError::SmartAccountNotInstalled`] - When the policy is not
///   installed for the given smart account and context rule.
///
/// # Events
///
/// * topics - `["session_uninstalled", smart_account: Address]`
/// * data - `[context_rule_id: u32]`
pub fn uninstall(e: &Env, context_rule: &ContextRule, smart_account: &Address) {
    // Require authorization from the smart_account
    smart_account.require_auth();

    let key = SessionStorageKey::AccountContext(smart_account.clone(), context_rule.id);

    if !e.storage().persistent().has(&key) {
        panic_with_error!(e, SessionError::SmartAccountNotInstalled)
    }

    e.storage().persistent().remove(&key);

    SessionUninstalled { smart_account: smart_account.clone(), context_rule_id: context_rule.id }
        .publish(e);
}
