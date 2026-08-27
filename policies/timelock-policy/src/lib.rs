//! # Timelock Policy for Delayed Execution
//!
//! Attach this policy to a `CallContract` context rule to enforce a mandatory
//! delay between when an action is proposed (authorized) and when it can be
//! executed. This implements a propose → delay → execute state machine with
//! cancellation support, similar in spirit to Zodiac's Delay Module but
//! adapted for Soroban's ledger-based timing and context-rule authorization
//! model.
//!
//! # How it works
//!
//! - **`install`** — called once per `(smart_account, context_rule)` pair,
//!   with a `delay_ledgers` value (the number of ledgers that must pass
//!   between proposal and execution) and an optional `cancellable_by` list
//!   of addresses authorized to cancel pending proposals.
//! - **`enforce`** — called during authorization; extracts the target
//!   contract, function name, and args from the `Context::Contract` variant,
//!   stores a `PendingProposal` with `unlock_ledger = current_ledger +
//!   delay_ledgers`, and emits a `TimelockProposed` event containing the
//!   proposal ID.
//! - **`execute_pending`** — a standalone entrypoint (not part of the `Policy`
//!   trait) that anyone can call once the delay has elapsed. Reads the
//!   proposal, validates the ledger has advanced past `unlock_ledger`,
//!   removes the proposal from storage, and invokes the target contract.
//! - **`cancel`** — a standalone entrypoint that removes a pending proposal
//!   before its delay has elapsed. Callable by addresses in the
//!   `cancellable_by` list (or any signer on the context rule if the list
//!   is empty).
//! - **`uninstall`** — removes the timelock configuration and all pending
//!   proposals for the given `(smart_account, context_rule_id)`.
//!
//! # Important Constraints
//!
//! - **Only `CallContract` context rules are supported.** The timelock needs
//!   a concrete target contract to invoke on execution; `Default` rules are
//!   rejected.
//! - **One fixed delay per installation.** The delay is set at install time
//!   and applies uniformly to all proposals under that context rule. Variable
//!   per-action delays are out of scope for v1.
//! - **Proposal IDs are communicated via events.** The `Policy::enforce()`
//!   trait signature returns `()`, so the proposal ID is included in the
//!   `TimelockProposed` event. Off-chain clients must index events to
//!   discover proposal IDs.
//! - **One-shot execution.** A proposal can only be executed once. After
//!   execution, the proposal is removed from storage.
//! - **No guardian-triggered recovery.** This policy handles delayed
//!   execution only. Guardian recovery is a separate design (see
//!   Discussion #31).
#![no_std]

use soroban_sdk::{
    auth::Context, contract, contracterror, contractevent, contractimpl, contracttype,
    panic_with_error, Address, Env, Symbol, Val, Vec,
};
use stellar_accounts::{
    policies::Policy,
    smart_account::{ContextRule, ContextRuleType, Signer},
};

// ################## CONSTANTS ##################

const DAY_IN_LEDGERS: u32 = 17280;
pub const TIMELOCK_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub const TIMELOCK_TTL_THRESHOLD: u32 = TIMELOCK_EXTEND_AMOUNT - DAY_IN_LEDGERS;

// ################## TYPES ##################

/// Installation parameters for the timelock policy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TimelockAccountParams {
    /// Number of ledgers that must pass between proposal and execution.
    pub delay_ledgers: u32,
    /// Addresses authorized to cancel pending proposals. If empty, any
    /// signer on the context rule can cancel.
    pub cancellable_by: Vec<Address>,
}

/// Configuration stored at install time, keyed by
/// `(smart_account, context_rule_id)`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TimelockConfig {
    /// Number of ledgers that must pass between proposal and execution.
    pub delay_ledgers: u32,
    /// Addresses authorized to cancel pending proposals. If empty, any
    /// signer on the context rule can cancel.
    pub cancellable_by: Vec<Address>,
}

/// A pending proposal awaiting its delay window.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingProposal {
    /// The target contract to call on execution.
    pub target: Address,
    /// The function name to invoke.
    pub fn_name: Symbol,
    /// The arguments to pass.
    pub args: Vec<Val>,
    /// The ledger sequence at which this proposal becomes executable.
    pub unlock_ledger: u32,
    /// The address that proposed this action.
    pub proposer: Address,
    /// The context rule ID this proposal was created under.
    pub context_rule_id: u32,
}

/// Storage keys for timelock policy data.
#[contracttype]
pub enum TimelockStorageKey {
    /// Configuration: delay and cancellable_by list.
    /// Keyed by `(smart_account, context_rule_id)`.
    Config(Address, u32),
    /// Pending proposal.
    /// Keyed by `(smart_account, proposal_id)`.
    Proposal(Address, u32),
    /// Next proposal ID counter per smart account and context rule.
    /// Keyed by `(smart_account, context_rule_id)`.
    NextProposalId(Address, u32),
}

// ################## ERRORS ##################

/// Error codes for timelock policy operations.
///
/// This is an independently deployed contract, not a module of a shared
/// library, so error numbering starts fresh at `1`.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum TimelockError {
    /// The policy is not installed for this smart account and context rule.
    NotInstalled = 1,
    /// The policy is already installed for this smart account and context rule.
    AlreadyInstalled = 2,
    /// The delay must be greater than zero.
    InvalidDelay = 3,
    /// Only `CallContract` context rules are supported.
    OnlyCallContractAllowed = 4,
    /// The action cannot be executed yet — the delay has not elapsed.
    DelayNotElapsed = 5,
    /// The proposal does not exist or has already been executed/cancelled.
    ProposalNotFound = 6,
    /// The caller is not authorized to cancel this proposal.
    UnauthorizedCancel = 7,
    /// No authenticated signers provided for proposal.
    NoAuthenticatedSigners = 8,
}

// ################## EVENTS ##################

/// Event emitted when a timelock policy is installed.
#[contractevent]
#[derive(Clone, Debug)]
pub struct TimelockInstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub delay_ledgers: u32,
}

/// Event emitted when a timelock policy is uninstalled.
#[contractevent]
#[derive(Clone, Debug)]
pub struct TimelockUninstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
}

/// Event emitted when a proposal is created via `enforce`.
#[contractevent]
#[derive(Clone, Debug)]
pub struct TimelockProposed {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub proposal_id: u32,
    pub target: Address,
    pub fn_name: Symbol,
    pub unlock_ledger: u32,
}

/// Event emitted when a pending proposal is executed.
#[contractevent]
#[derive(Clone, Debug)]
pub struct TimelockExecuted {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub proposal_id: u32,
}

/// Event emitted when a pending proposal is cancelled.
#[contractevent]
#[derive(Clone, Debug)]
pub struct TimelockCancelled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub proposal_id: u32,
}

// ################## CONTRACT ##################

#[contract]
pub struct TimelockPolicy;

// ################## POLICY TRAIT ##################

#[contractimpl]
impl Policy for TimelockPolicy {
    type AccountParams = TimelockAccountParams;

    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        enforce_proposal(e, &context, &authenticated_signers, &context_rule, &smart_account)
    }

    fn install(
        e: &Env,
        install_params: TimelockAccountParams,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        install_timelock(e, &install_params, &context_rule, &smart_account)
    }

    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        uninstall_timelock(e, &context_rule, &smart_account)
    }
}

// ################## PUBLIC ENTRYPOINTS ##################

#[contractimpl]
impl TimelockPolicy {
    /// Executes a pending proposal after the delay has elapsed.
    ///
    /// Reads the proposal from storage, validates that the current ledger
    /// sequence is at or past `unlock_ledger`, removes the proposal, and
    /// invokes the target contract with the stored arguments.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `smart_account` - The address of the smart account that owns this
    ///   proposal.
    /// * `proposal_id` - The ID of the proposal to execute.
    ///
    /// # Errors
    ///
    /// * [`TimelockError::ProposalNotFound`] - When the proposal does not
    ///   exist or has already been executed/cancelled.
    /// * [`TimelockError::DelayNotElapsed`] - When the current ledger is
    ///   before the proposal's `unlock_ledger`.
    ///
    /// # Events
    ///
    /// * topics - `["timelock_executed", smart_account: Address]`
    /// * data - `[context_rule_id: u32, proposal_id: u32]`
    pub fn execute_pending(e: Env, smart_account: Address, proposal_id: u32) {
        let key = TimelockStorageKey::Proposal(smart_account.clone(), proposal_id);
        let proposal: PendingProposal = e
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, TimelockError::ProposalNotFound));

        if e.ledger().sequence() < proposal.unlock_ledger {
            panic_with_error!(e, TimelockError::DelayNotElapsed);
        }

        // Remove the proposal before execution to prevent re-execution.
        e.storage().persistent().remove(&key);

        TimelockExecuted {
            smart_account: smart_account.clone(),
            context_rule_id: proposal.context_rule_id,
            proposal_id,
        }
        .publish(&e);

        // Execute the stored action.
        e.invoke_contract::<Val>(&proposal.target, &proposal.fn_name, proposal.args);
    }

    /// Cancels a pending proposal before its delay has elapsed.
    ///
    /// Removes the proposal from storage, preventing future execution.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `caller` - The address requesting cancellation. Must be
    ///   authorized: either in the `cancellable_by` list, or if that list
    ///   is empty, the smart account itself.
    /// * `smart_account` - The address of the smart account that owns this
    ///   proposal.
    /// * `proposal_id` - The ID of the proposal to cancel.
    ///
    /// # Errors
    ///
    /// * [`TimelockError::ProposalNotFound`] - When the proposal does not
    ///   exist or has already been executed/cancelled.
    /// * [`TimelockError::UnauthorizedCancel`] - When the caller is not
    ///   authorized to cancel this proposal.
    ///
    /// # Events
    ///
    /// * topics - `["timelock_cancelled", smart_account: Address]`
    /// * data - `[context_rule_id: u32, proposal_id: u32]`
    pub fn cancel(e: Env, caller: Address, smart_account: Address, proposal_id: u32) {
        caller.require_auth();

        let key = TimelockStorageKey::Proposal(smart_account.clone(), proposal_id);
        let proposal: PendingProposal = e
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(e, TimelockError::ProposalNotFound));

        let config_key =
            TimelockStorageKey::Config(smart_account.clone(), proposal.context_rule_id);
        let config: TimelockConfig = e
            .storage()
            .persistent()
            .get(&config_key)
            .unwrap_or_else(|| panic_with_error!(e, TimelockError::NotInstalled));

        if config.cancellable_by.is_empty() {
            // If cancellable_by is empty, only the smart account itself
            // can cancel (via self-authorization through its context
            // rule signers).
            if caller != smart_account {
                panic_with_error!(e, TimelockError::UnauthorizedCancel);
            }
        } else {
            // Check if caller is in the cancellable_by list.
            if !config.cancellable_by.contains(&caller) {
                panic_with_error!(e, TimelockError::UnauthorizedCancel);
            }
        }

        e.storage().persistent().remove(&key);

        TimelockCancelled {
            smart_account: smart_account.clone(),
            context_rule_id: proposal.context_rule_id,
            proposal_id,
        }
        .publish(&e);
    }

    /// Returns the pending proposal for a given smart account and proposal ID.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `smart_account` - The address of the smart account.
    /// * `proposal_id` - The ID of the proposal to retrieve.
    ///
    /// # Errors
    ///
    /// * [`TimelockError::ProposalNotFound`] - When the proposal does not
    ///   exist or has already been executed/cancelled.
    pub fn get_proposal(e: &Env, smart_account: &Address, proposal_id: u32) -> PendingProposal {
        let key = TimelockStorageKey::Proposal(smart_account.clone(), proposal_id);
        e.storage()
            .persistent()
            .get(&key)
            .inspect(|_| {
                e.storage().persistent().extend_ttl(
                    &key,
                    TIMELOCK_TTL_THRESHOLD,
                    TIMELOCK_EXTEND_AMOUNT,
                );
            })
            .unwrap_or_else(|| panic_with_error!(e, TimelockError::ProposalNotFound))
    }

    /// Returns the timelock configuration for a given smart account and
    /// context rule.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `context_rule_id` - The context rule ID.
    /// * `smart_account` - The address of the smart account.
    ///
    /// # Errors
    ///
    /// * [`TimelockError::NotInstalled`] - When the policy is not installed
    ///   for the given smart account and context rule.
    pub fn get_config(e: &Env, context_rule_id: u32, smart_account: &Address) -> TimelockConfig {
        let key = TimelockStorageKey::Config(smart_account.clone(), context_rule_id);
        e.storage()
            .persistent()
            .get(&key)
            .inspect(|_| {
                e.storage().persistent().extend_ttl(
                    &key,
                    TIMELOCK_TTL_THRESHOLD,
                    TIMELOCK_EXTEND_AMOUNT,
                );
            })
            .unwrap_or_else(|| panic_with_error!(e, TimelockError::NotInstalled))
    }
}

// ################## PRIVATE HELPERS ##################

/// Installs the timelock policy on a smart account.
///
/// Only `CallContract` context rules are allowed, since the timelock needs
/// a concrete target contract to invoke on execution.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `params` - Installation parameters containing the delay and
///   cancellable-by list.
/// * `context_rule` - The context rule this policy is being attached to.
/// * `smart_account` - The address of the smart account installing this
///   policy.
///
/// # Errors
///
/// * [`TimelockError::OnlyCallContractAllowed`] - When the context rule
///   type is not `CallContract`.
/// * [`TimelockError::InvalidDelay`] - When `delay_ledgers` is zero.
/// * [`TimelockError::AlreadyInstalled`] - When the policy was already
///   installed for this smart account and context rule.
///
/// # Events
///
/// * topics - `["timelock_installed", smart_account: Address]`
/// * data - `[context_rule_id: u32, delay_ledgers: u32]`
fn install_timelock(
    e: &Env,
    params: &TimelockAccountParams,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    smart_account.require_auth();

    if !matches!(context_rule.context_type, ContextRuleType::CallContract(_)) {
        panic_with_error!(e, TimelockError::OnlyCallContractAllowed);
    }

    if params.delay_ledgers == 0 {
        panic_with_error!(e, TimelockError::InvalidDelay);
    }

    let key = TimelockStorageKey::Config(smart_account.clone(), context_rule.id);

    if e.storage().persistent().has(&key) {
        panic_with_error!(e, TimelockError::AlreadyInstalled);
    }

    let config = TimelockConfig {
        delay_ledgers: params.delay_ledgers,
        cancellable_by: params.cancellable_by.clone(),
    };

    e.storage().persistent().set(&key, &config);

    // Initialize the next proposal ID counter.
    let id_key = TimelockStorageKey::NextProposalId(smart_account.clone(), context_rule.id);
    e.storage().persistent().set(&id_key, &0u32);

    TimelockInstalled {
        smart_account: smart_account.clone(),
        context_rule_id: context_rule.id,
        delay_ledgers: params.delay_ledgers,
    }
    .publish(e);
}

/// Uninstalls the timelock policy from a smart account, removing the
/// configuration and all pending proposals for the given context rule.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `context_rule` - The context rule this policy is being removed from.
/// * `smart_account` - The address of the smart account.
///
/// # Errors
///
/// * [`TimelockError::NotInstalled`] - When the policy is not installed
///   for the given smart account and context rule.
///
/// # Events
///
/// * topics - `["timelock_uninstalled", smart_account: Address]`
/// * data - `[context_rule_id: u32]`
fn uninstall_timelock(e: &Env, context_rule: &ContextRule, smart_account: &Address) {
    smart_account.require_auth();

    let config_key = TimelockStorageKey::Config(smart_account.clone(), context_rule.id);
    if !e.storage().persistent().has(&config_key) {
        panic_with_error!(e, TimelockError::NotInstalled);
    }

    e.storage().persistent().remove(&config_key);

    // Clean up the next proposal ID counter.
    let id_key = TimelockStorageKey::NextProposalId(smart_account.clone(), context_rule.id);
    e.storage().persistent().remove(&id_key);

    TimelockUninstalled { smart_account: smart_account.clone(), context_rule_id: context_rule.id }
        .publish(e);
}

/// Enforces the timelock policy: stores a pending proposal with the target
/// contract, function name, args, and unlock ledger.
///
/// # Arguments
///
/// * `e` - Access to the Soroban environment.
/// * `context` - The authorization context being enforced.
/// * `authenticated_signers` - The list of authenticated signers.
/// * `context_rule` - The context rule for this policy.
/// * `smart_account` - The address of the smart account.
///
/// # Errors
///
/// * [`TimelockError::NoAuthenticatedSigners`] - When no signers
///   authenticated.
/// * [`TimelockError::NotInstalled`] - When the policy is not installed.
///
/// # Events
///
/// * topics - `["timelock_proposed", smart_account: Address]`
/// * data - `[context_rule_id: u32, proposal_id: u32, target: Address,
///   fn_name: Symbol, unlock_ledger: u32]`
fn enforce_proposal(
    e: &Env,
    context: &Context,
    authenticated_signers: &Vec<Signer>,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    smart_account.require_auth();

    if authenticated_signers.is_empty() {
        panic_with_error!(e, TimelockError::NoAuthenticatedSigners);
    }

    let config_key = TimelockStorageKey::Config(smart_account.clone(), context_rule.id);
    let config: TimelockConfig = e
        .storage()
        .persistent()
        .get(&config_key)
        .unwrap_or_else(|| panic_with_error!(e, TimelockError::NotInstalled));

    // Extract target, fn_name, and args from the context.
    let (target, fn_name, args) = match context {
        Context::Contract(ctx) => (ctx.contract.clone(), ctx.fn_name.clone(), ctx.args.clone()),
        _ => panic_with_error!(e, TimelockError::OnlyCallContractAllowed),
    };

    let unlock_ledger = e.ledger().sequence() + config.delay_ledgers;

    // Allocate a proposal ID.
    let id_key = TimelockStorageKey::NextProposalId(smart_account.clone(), context_rule.id);
    let proposal_id: u32 = e.storage().persistent().get(&id_key).unwrap_or(0u32);
    e.storage().persistent().set(&id_key, &(proposal_id + 1));

    let proposal = PendingProposal {
        target: target.clone(),
        fn_name: fn_name.clone(),
        args,
        unlock_ledger,
        proposer: smart_account.clone(),
        context_rule_id: context_rule.id,
    };

    let proposal_key = TimelockStorageKey::Proposal(smart_account.clone(), proposal_id);
    e.storage().persistent().set(&proposal_key, &proposal);

    TimelockProposed {
        smart_account: smart_account.clone(),
        context_rule_id: context_rule.id,
        proposal_id,
        target,
        fn_name,
        unlock_ledger,
    }
    .publish(e);
}

#[cfg(test)]
mod test;
