//! # Recipient Allowlist Policy Module
//!
//! Constrains *who* may receive funds for a `CallContract` context rule.
//! Pair with a token contract rule and (optionally) `spending-limit-policy`
//! so a session key may transfer only to a known set of addresses.
//!
//! Only intercepts calls literally named `transfer`, reading the recipient
//! as the second positional argument — the SEP-41
//! `transfer(from, to, amount)` shape (same assumption as spending-limit).
use soroban_sdk::{
    auth::{Context, ContractContext},
    contracterror, contractevent, contracttype, panic_with_error, symbol_short, Address, Env,
    TryFromVal, Val, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

/// Event emitted when a recipient allowlist policy is enforced.
#[contractevent]
#[derive(Clone)]
pub struct RecipientAllowlistEnforced {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub recipient: Address,
}

/// Event emitted when a recipient allowlist policy is installed.
#[contractevent]
#[derive(Clone, Debug)]
pub struct RecipientAllowlistInstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub allowed_recipients: Vec<Address>,
}

/// Event emitted when a recipient allowlist policy is uninstalled.
#[contractevent]
#[derive(Clone, Debug)]
pub struct RecipientAllowlistUninstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
}

/// Installation parameters for the recipient allowlist policy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RecipientAllowlistAccountParams {
    /// Recipient addresses permitted to receive transfers.
    pub allowed_recipients: Vec<Address>,
}

/// Internal storage structure for the recipient allowlist.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RecipientAllowlistData {
    pub allowed_recipients: Vec<Address>,
}

/// Error codes for recipient allowlist policy operations.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RecipientAllowlistError {
    /// Policy is not installed for this smart account and context rule.
    SmartAccountNotInstalled = 1,
    /// Only the `CallContract` context rule type is allowed.
    OnlyCallContractAllowed = 2,
    /// `allowed_recipients` was empty or exceeded `MAX_ALLOWED_RECIPIENTS`.
    InvalidAllowedRecipients = 3,
    /// Call is not an allowlisted transfer, or recipient is not permitted.
    RecipientNotAllowed = 4,
    /// Policy was already installed for this smart account and context rule.
    AlreadyInstalled = 5,
}

/// Storage keys for recipient allowlist policy data.
#[contracttype]
pub enum RecipientAllowlistStorageKey {
    AccountContext(Address, u32),
}

// ################## CONSTANTS ##################

const DAY_IN_LEDGERS: u32 = 17280;
pub const RECIPIENT_ALLOWLIST_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub const RECIPIENT_ALLOWLIST_TTL_THRESHOLD: u32 =
    RECIPIENT_ALLOWLIST_EXTEND_AMOUNT - DAY_IN_LEDGERS;

/// Maximum number of allowed recipients per policy install.
pub const MAX_ALLOWED_RECIPIENTS: u32 = 20;

// ################## QUERY STATE ##################

/// Retrieves the recipient allowlist for a smart account's policy.
pub fn get_allowed_recipients(
    e: &Env,
    context_rule_id: u32,
    smart_account: &Address,
) -> Vec<Address> {
    let key = RecipientAllowlistStorageKey::AccountContext(smart_account.clone(), context_rule_id);
    e.storage()
        .persistent()
        .get::<_, RecipientAllowlistData>(&key)
        .inspect(|_| {
            e.storage().persistent().extend_ttl(
                &key,
                RECIPIENT_ALLOWLIST_TTL_THRESHOLD,
                RECIPIENT_ALLOWLIST_EXTEND_AMOUNT,
            );
        })
        .map(|data| data.allowed_recipients)
        .unwrap_or_else(|| {
            panic_with_error!(e, RecipientAllowlistError::SmartAccountNotInstalled)
        })
}

// ################## CHANGE STATE ##################

/// Enforces that a `transfer(from, to, amount)` call's `to` is allowlisted.
pub fn enforce(
    e: &Env,
    context: &Context,
    authenticated_signers: &Vec<Signer>,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    smart_account.require_auth();

    if authenticated_signers.is_empty() {
        panic_with_error!(e, RecipientAllowlistError::RecipientNotAllowed)
    }

    let allowed = get_allowed_recipients(e, context_rule.id, smart_account);

    match context {
        Context::Contract(ContractContext { fn_name, args, .. }) => {
            if fn_name != &symbol_short!("transfer") {
                panic_with_error!(e, RecipientAllowlistError::RecipientNotAllowed)
            }
            // SEP-41 transfer(from, to, amount) — recipient is argument index 1.
            if args.len() < 2 {
                panic_with_error!(e, RecipientAllowlistError::RecipientNotAllowed)
            }
            let to_val: Val = args.get(1).unwrap();
            let Ok(to) = Address::try_from_val(e, &to_val) else {
                panic_with_error!(e, RecipientAllowlistError::RecipientNotAllowed)
            };

            if !allowed.contains(&to) {
                panic_with_error!(e, RecipientAllowlistError::RecipientNotAllowed)
            }

            RecipientAllowlistEnforced {
                smart_account: smart_account.clone(),
                context_rule_id: context_rule.id,
                recipient: to,
            }
            .publish(e);
        }
        _ => panic_with_error!(e, RecipientAllowlistError::RecipientNotAllowed),
    }
}

/// Installs the recipient allowlist policy on a smart account.
pub fn install(
    e: &Env,
    params: &RecipientAllowlistAccountParams,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    smart_account.require_auth();

    if !matches!(context_rule.context_type, ContextRuleType::CallContract(_)) {
        panic_with_error!(e, RecipientAllowlistError::OnlyCallContractAllowed)
    }

    if params.allowed_recipients.is_empty()
        || params.allowed_recipients.len() > MAX_ALLOWED_RECIPIENTS
    {
        panic_with_error!(e, RecipientAllowlistError::InvalidAllowedRecipients)
    }

    let key = RecipientAllowlistStorageKey::AccountContext(smart_account.clone(), context_rule.id);

    if e.storage().persistent().has(&key) {
        panic_with_error!(e, RecipientAllowlistError::AlreadyInstalled)
    }

    let data = RecipientAllowlistData {
        allowed_recipients: params.allowed_recipients.clone(),
    };

    e.storage().persistent().set(&key, &data);

    RecipientAllowlistInstalled {
        smart_account: smart_account.clone(),
        context_rule_id: context_rule.id,
        allowed_recipients: params.allowed_recipients.clone(),
    }
    .publish(e);
}

/// Replaces the stored recipient allowlist after installation.
pub fn set_allowed_recipients(
    e: &Env,
    allowed_recipients: &Vec<Address>,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    smart_account.require_auth();

    if allowed_recipients.is_empty() || allowed_recipients.len() > MAX_ALLOWED_RECIPIENTS {
        panic_with_error!(e, RecipientAllowlistError::InvalidAllowedRecipients)
    }

    let key = RecipientAllowlistStorageKey::AccountContext(smart_account.clone(), context_rule.id);

    if !e.storage().persistent().has(&key) {
        panic_with_error!(e, RecipientAllowlistError::SmartAccountNotInstalled)
    }

    let data = RecipientAllowlistData {
        allowed_recipients: allowed_recipients.clone(),
    };
    e.storage().persistent().set(&key, &data);
}

/// Uninstalls the recipient allowlist policy.
pub fn uninstall(e: &Env, context_rule: &ContextRule, smart_account: &Address) {
    smart_account.require_auth();

    let key = RecipientAllowlistStorageKey::AccountContext(smart_account.clone(), context_rule.id);

    if !e.storage().persistent().has(&key) {
        panic_with_error!(e, RecipientAllowlistError::SmartAccountNotInstalled)
    }

    e.storage().persistent().remove(&key);

    RecipientAllowlistUninstalled {
        smart_account: smart_account.clone(),
        context_rule_id: context_rule.id,
    }
    .publish(e);
}
