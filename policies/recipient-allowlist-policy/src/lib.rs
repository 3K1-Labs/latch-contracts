//! # Recipient Allowlist Policy Module
//!
//! This policy restricts a context rule's signers to a fixed set of
//! recipient addresses. It is designed to be attached to a `CallContract`
//! context rule, where it intercepts `transfer(from, to, amount)` calls
//! and ensures the `to` address (the recipient) is in the allowlist.
#![no_std]

use soroban_sdk::{
    auth::{Context, ContractContext},
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, Address,
    Env, Symbol, TryIntoVal, Vec,
};
use stellar_accounts::{
    policies::Policy,
    smart_account::{ContextRule, ContextRuleType, Signer},
};

/// Event emitted when a recipient policy is enforced.
#[contractevent]
#[derive(Clone)]
pub struct RecipientEnforced {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub recipient: Address,
}

/// Event emitted when a recipient policy is installed.
#[contractevent]
#[derive(Clone, Debug)]
pub struct RecipientInstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub allowed_recipients: Vec<Address>,
}

/// Event emitted when a recipient policy is uninstalled.
#[contractevent]
#[derive(Clone, Debug)]
pub struct RecipientUninstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
}

/// Installation parameters for the recipient allowlist policy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RecipientAccountParams {
    /// The recipient addresses that this policy is permitted to transfer to.
    pub allowed_recipients: Vec<Address>,
}

/// Internal storage structure for the recipient allowlist.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RecipientData {
    /// The recipient addresses that this policy is permitted to transfer to.
    pub allowed_recipients: Vec<Address>,
}

/// Error codes for recipient allowlist policy operations.
///
/// This is a standalone, independently-deployed contract, not a module of
/// the upstream `stellar-accounts` crate, so it is not part of that crate's
/// shared error-numbering convention (see its `CLAUDE.md`). Numbering starts
/// fresh at `1`.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RecipientError {
    /// The smart account does not have a recipient policy installed for this
    /// context rule.
    SmartAccountNotInstalled = 1,
    /// Only the `CallContract` context rule type is allowed.
    OnlyCallContractAllowed = 2,
    /// `allowed_recipients` was empty or exceeded `MAX_ALLOWED_RECIPIENTS`.
    InvalidAllowedRecipients = 3,
    /// The context is not a `Context::Contract` invocation for `transfer`,
    /// or the recipient address is not in the allowlist.
    RecipientNotAllowed = 4,
    /// The policy was already installed for this smart account and context
    /// rule.
    AlreadyInstalled = 5,
    /// The call arguments could not be parsed to extract a recipient.
    InvalidTransferArgs = 6,
}

/// Storage keys for recipient policy data.
#[contracttype]
pub enum RecipientStorageKey {
    /// Storage key for the allowlist of a smart account context rule.
    AccountContext(Address, u32),
}

// ################## CONSTANTS ##################

const DAY_IN_LEDGERS: u32 = 17280;
pub const RECIPIENT_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub const RECIPIENT_TTL_THRESHOLD: u32 = RECIPIENT_EXTEND_AMOUNT - DAY_IN_LEDGERS;

/// Maximum number of allowed recipients per policy.
/// Bounds the linear scan performed in `enforce` and the storage size.
pub const MAX_ALLOWED_RECIPIENTS: u32 = 10;

// ################## HELPER ##################
fn emit_recipient_enforced(
    e: &Env,
    smart_account: &Address,
    context_rule_id: u32,
    recipient: &Address,
) {
    RecipientEnforced {
        smart_account: smart_account.clone(),
        context_rule_id,
        recipient: recipient.clone(),
    }
    .publish(e);
}

fn emit_recipient_installed(
    e: &Env,
    smart_account: &Address,
    context_rule_id: u32,
    allowed_recipients: &Vec<Address>,
) {
    RecipientInstalled {
        smart_account: smart_account.clone(),
        context_rule_id,
        allowed_recipients: allowed_recipients.clone(),
    }
    .publish(e);
}

fn emit_recipient_uninstalled(e: &Env, smart_account: &Address, context_rule_id: u32) {
    RecipientUninstalled { smart_account: smart_account.clone(), context_rule_id }.publish(e);
}

#[contract]
pub struct RecipientAllowlistPolicy;

#[contractimpl]
impl Policy for RecipientAllowlistPolicy {
    type AccountParams = RecipientAccountParams;

    /// Enforces the recipient policy: the context must be a `Context::Contract`
    /// invocation of a function named "transfer", and the recipient (2nd
    /// argument) must be in the installed allowlist. Requires authorization
    /// from the smart account.
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
    /// * [`RecipientError::RecipientNotAllowed`] - When there are no
    ///   authenticated signers, the context is not a `Context::Contract`
    ///   invocation for `transfer`, or the recipient is not in the allowlist.
    /// * [`RecipientError::InvalidTransferArgs`] - When the arguments are
    ///   malformed.
    /// * refer to [`get_allowed_recipients`] errors.
    ///
    /// # Events
    ///
    /// * topics - `["recipient_enforced", smart_account: Address]`
    /// * data - `[context_rule_id: u32, recipient: Address]`
    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        // Require authorization from the smart_account
        smart_account.require_auth();

        if authenticated_signers.is_empty() {
            panic_with_error!(e, RecipientError::RecipientNotAllowed)
        }

        let allowed_recipients =
            Self::get_allowed_recipients(e, context_rule.id, smart_account.clone());

        match context {
            Context::Contract(ContractContext { fn_name, args, .. }) => {
                if fn_name != Symbol::new(e, "transfer") {
                    panic_with_error!(e, RecipientError::RecipientNotAllowed)
                }

                // A standard SEP-41 `transfer` is `transfer(from, to, amount)`.
                // The recipient is the second argument (index 1).
                let recipient_val = args
                    .get(1)
                    .unwrap_or_else(|| panic_with_error!(e, RecipientError::InvalidTransferArgs));
                let recipient: Address = recipient_val
                    .try_into_val(e)
                    .unwrap_or_else(|_| panic_with_error!(e, RecipientError::InvalidTransferArgs));

                if !allowed_recipients.contains(recipient.clone()) {
                    panic_with_error!(e, RecipientError::RecipientNotAllowed)
                }

                emit_recipient_enforced(e, &smart_account, context_rule.id, &recipient);
            }
            _ => panic_with_error!(e, RecipientError::RecipientNotAllowed),
        }
    }

    /// Installs the recipient policy on a smart account. Only `CallContract`
    /// context type is allowed, since the allowlisted recipients are only
    /// meaningful when pinned to one target contract. Requires authorization
    /// from the smart account.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `params` - Installation parameters containing the allowed recipients.
    /// * `context_rule` - The context rule for this policy.
    /// * `smart_account` - The address of the smart account.
    ///
    /// # Errors
    ///
    /// * [`RecipientError::OnlyCallContractAllowed`] - When the context rule
    ///   type is not `CallContract`.
    /// * [`RecipientError::InvalidAllowedRecipients`] - When
    ///   `allowed_recipients` is empty or exceeds `MAX_ALLOWED_RECIPIENTS`.
    /// * [`RecipientError::AlreadyInstalled`] - When the policy was already
    ///   installed for this smart account and context rule.
    ///
    /// # Events
    ///
    /// * topics - `["recipient_installed", smart_account: Address]`
    /// * data - `[context_rule_id: u32, allowed_recipients: Vec<Address>]`
    fn install(
        e: &Env,
        params: Self::AccountParams,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        // Require authorization from the smart_account
        smart_account.require_auth();

        if !matches!(context_rule.context_type, ContextRuleType::CallContract(_)) {
            panic_with_error!(e, RecipientError::OnlyCallContractAllowed)
        }

        if params.allowed_recipients.is_empty()
            || params.allowed_recipients.len() > MAX_ALLOWED_RECIPIENTS
        {
            panic_with_error!(e, RecipientError::InvalidAllowedRecipients)
        }

        let key = RecipientStorageKey::AccountContext(smart_account.clone(), context_rule.id);

        if e.storage().persistent().has(&key) {
            panic_with_error!(e, RecipientError::AlreadyInstalled)
        }

        let data = RecipientData { allowed_recipients: params.allowed_recipients.clone() };

        e.storage().persistent().set(&key, &data);

        emit_recipient_installed(e, &smart_account, context_rule.id, &params.allowed_recipients);
    }

    /// Uninstalls the recipient policy from a smart account, removing all
    /// stored allowlist data for the account and context rule. Requires
    /// authorization from the smart account.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `context_rule` - The context rule for this policy.
    /// * `smart_account` - The address of the smart account.
    ///
    /// # Errors
    ///
    /// * [`RecipientError::SmartAccountNotInstalled`] - When the policy is not
    ///   installed for the given smart account and context rule.
    ///
    /// # Events
    ///
    /// * topics - `["recipient_uninstalled", smart_account: Address]`
    /// * data - `[context_rule_id: u32]`
    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        // Require authorization from the smart_account
        smart_account.require_auth();

        let key = RecipientStorageKey::AccountContext(smart_account.clone(), context_rule.id);

        if !e.storage().persistent().has(&key) {
            panic_with_error!(e, RecipientError::SmartAccountNotInstalled)
        }

        e.storage().persistent().remove(&key);

        emit_recipient_uninstalled(e, &smart_account, context_rule.id);
    }
}

// ################## QUERY & MUTATION ##################

#[contractimpl]
impl RecipientAllowlistPolicy {
    /// Retrieves the allowlist for a smart account's recipient policy.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `context_rule_id` - The context rule ID for this policy.
    /// * `smart_account` - The address of the smart account.
    ///
    /// # Errors
    ///
    /// * [`RecipientError::SmartAccountNotInstalled`] - When the smart account
    ///   does not have a recipient policy installed.
    pub fn get_allowed_recipients(
        e: &Env,
        context_rule_id: u32,
        smart_account: Address,
    ) -> Vec<Address> {
        let key = RecipientStorageKey::AccountContext(smart_account, context_rule_id);
        e.storage()
            .persistent()
            .get::<_, RecipientData>(&key)
            .inspect(|_| {
                e.storage().persistent().extend_ttl(
                    &key,
                    RECIPIENT_TTL_THRESHOLD,
                    RECIPIENT_EXTEND_AMOUNT,
                );
            })
            .map(|data| data.allowed_recipients)
            .unwrap_or_else(|| panic_with_error!(e, RecipientError::SmartAccountNotInstalled))
    }

    /// Updates the allowlist for a smart account's recipient policy.
    ///
    /// # Arguments
    ///
    /// * `e` - Access to the Soroban environment.
    /// * `allowed_recipients` - The new list of allowed recipients.
    /// * `context_rule_id` - The context rule ID for this policy.
    /// * `smart_account` - The address of the smart account.
    ///
    /// # Errors
    ///
    /// * [`RecipientError::SmartAccountNotInstalled`] - When the policy is not
    ///   installed for the given smart account and context rule.
    /// * [`RecipientError::InvalidAllowedRecipients`] - When
    ///   `allowed_recipients` is empty or exceeds `MAX_ALLOWED_RECIPIENTS`.
    pub fn set_allowed_recipients(
        e: Env,
        allowed_recipients: Vec<Address>,
        context_rule_id: u32,
        smart_account: Address,
    ) {
        smart_account.require_auth();

        if allowed_recipients.is_empty() || allowed_recipients.len() > MAX_ALLOWED_RECIPIENTS {
            panic_with_error!(&e, RecipientError::InvalidAllowedRecipients)
        }

        let key = RecipientStorageKey::AccountContext(smart_account.clone(), context_rule_id);

        if !e.storage().persistent().has(&key) {
            panic_with_error!(&e, RecipientError::SmartAccountNotInstalled)
        }

        let data = RecipientData { allowed_recipients: allowed_recipients.clone() };

        e.storage().persistent().set(&key, &data);
    }
}

#[cfg(test)]
mod test;
