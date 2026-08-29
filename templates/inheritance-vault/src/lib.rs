//! Dead-man's-switch inheritance vault — a "personal contract" template.
//!
//! An informal succession mechanism: funds stay under `owner`'s normal
//! control as long as they periodically call `check_in`, and become
//! claimable by `beneficiary` once `owner` goes silent for
//! `inactivity_period_ledgers`. A different trigger model from a pure
//! timelock — this releases based on *inactivity*, not elapsed time alone,
//! and hands a second party (the beneficiary) real, if conditional, power
//! over the funds once triggered.
//!
//! Deployed as a satellite contract owned by a Latch smart account (see the
//! account-authorized contract-deployment entrypoint this template is meant
//! to be installed through), one instance per owner/beneficiary pair — not a
//! shared singleton like `ed25519-verifier`/`threshold-policy`.
//!
//! # How it works
//!
//! - `__constructor(owner, beneficiary, inactivity_period_ledgers)` — sets
//!   the vault's parties and inactivity window, and seeds `last_active_ledger`
//!   to the deployment ledger (deploying the vault counts as the first
//!   check-in).
//! - `check_in` — the only method `owner` needs to call periodically. Resets
//!   `last_active_ledger` to now. Requires `owner`'s authorization, and is
//!   itself rejected once the vault has become claimable (see below).
//! - `update_beneficiary` / `extend_inactivity_period` — owner-gated
//!   reconfiguration, available at any time *before* the vault becomes
//!   claimable.
//! - `claim(token, to)` — requires `beneficiary`'s authorization and that the
//!   vault has become claimable. Transfers this contract's entire balance of
//!   `token` to `to` and returns the amount transferred.
//! - `is_claimable` / `get_vault_data` — pure reads for clients to poll
//!   status without guessing at internal storage layout.
//!
//! # Security Warning: no owner override once claimable
//!
//! Once `e.ledger().sequence() >= last_active_ledger + inactivity_period_ledgers`,
//! the vault is permanently claimable: `check_in`, `update_beneficiary`, and
//! `extend_inactivity_period` all reject with [`InheritanceVaultError::AlreadyClaimable`]
//! from that point on, even with valid `owner` authorization. This is
//! deliberate — without it, `owner` (or anyone who can get `owner` to sign,
//! e.g. under duress, or an owner surfacing the moment they notice a pending
//! claim) could grief a legitimate claim in flight by resetting the clock at
//! the last second. Once triggered, it stays triggered; there is no path
//! back to owner control for that vault instance.
//!
//! # Security Warning: this is a blunt instrument
//!
//! This contract cannot distinguish "owner is deceased/incapacitated" from
//! any other reason `owner` stopped checking in (lost device, lost keys,
//! extended travel, simple forgetfulness). It has no dispute or arbitration
//! mechanism, and none is planned — that is a client/product/legal concern,
//! not something a simple on-chain timer can safely arbitrate. Choose
//! `inactivity_period_ledgers` accordingly, and treat this as informal
//! succession, not a substitute for real estate planning.
//!
//! # Security Warning: storage TTL is an operational, not contractual, concern
//!
//! This contract is meant to sit dormant for potentially very long
//! stretches — that's the entire point of a dead-man's switch. If nobody
//! ever submits a transaction touching this contract's storage during a long
//! inactivity window, its ledger entries can still expire the way any
//! Soroban contract's can, independent of `inactivity_period_ledgers`. This
//! contract extends its own TTL generously (see [`VAULT_EXTEND_AMOUNT`]) on
//! every state-changing call, but if `owner` never checks in and
//! `beneficiary` doesn't act on it either, nothing here submits a
//! transaction on its own. Keeping the entry alive across a genuinely long
//! silence is the same "extend footprint TTL" operation any address can
//! submit permissionlessly for any contract, with no authorization from
//! `owner` or `beneficiary` required — a client/keep-alive concern, not
//! something this contract's logic can or should manage itself.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, token,
    Address, Env,
};

/// Vault configuration and liveness state, stored as a single instance entry
/// — this contract is a one-per-deployment singleton, not shared across
/// multiple owners like a policy contract.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultData {
    pub owner: Address,
    pub beneficiary: Address,
    pub inactivity_period_ledgers: u32,
    pub last_active_ledger: u32,
}

#[contracttype]
pub enum InheritanceVaultStorageKey {
    Vault,
}

/// Error codes for inheritance vault operations.
///
/// This is a standalone, independently-deployed contract, not a module of
/// the upstream `stellar-accounts` crate, so it is not part of that crate's
/// shared error-numbering convention. Numbering starts fresh at `1`.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum InheritanceVaultError {
    /// `beneficiary` was the same address as `owner`.
    InvalidBeneficiary = 1,
    /// `inactivity_period_ledgers` was zero.
    InvalidInactivityPeriod = 2,
    /// The inactivity threshold has already been reached. The vault is
    /// claimable, and by design `owner` can no longer check in or
    /// reconfigure it from this point on — see the module-level "no owner
    /// override" warning.
    AlreadyClaimable = 3,
    /// The inactivity threshold has not yet been reached; `beneficiary`
    /// cannot claim.
    NotYetClaimable = 4,
    /// This contract holds no balance of the requested token to claim.
    NoFundsToClaim = 5,
}

/// Event emitted when `owner` checks in.
#[contractevent]
#[derive(Clone, Debug)]
pub struct CheckedIn {
    #[topic]
    pub owner: Address,
    pub last_active_ledger: u32,
}

/// Event emitted when `owner` changes the beneficiary before the vault
/// becomes claimable.
#[contractevent]
#[derive(Clone, Debug)]
pub struct BeneficiaryUpdated {
    #[topic]
    pub owner: Address,
    pub old_beneficiary: Address,
    pub new_beneficiary: Address,
}

/// Event emitted when `owner` changes the inactivity period before the
/// vault becomes claimable.
#[contractevent]
#[derive(Clone, Debug)]
pub struct InactivityPeriodUpdated {
    #[topic]
    pub owner: Address,
    pub old_inactivity_period_ledgers: u32,
    pub new_inactivity_period_ledgers: u32,
}

/// Event emitted when `beneficiary` claims the vault's balance of `token`.
#[contractevent]
#[derive(Clone, Debug)]
pub struct Claimed {
    #[topic]
    pub beneficiary: Address,
    pub token: Address,
    pub to: Address,
    pub amount: i128,
}

// ################## CONSTANTS ##################

const DAY_IN_LEDGERS: u32 = 17280;

/// TTL extension amount used on every state-changing call, chosen to match
/// the common network-configured ceiling on a single `extend_ttl` request
/// (~180 days) — see the module-level TTL warning for why a generous bump
/// here is not sufficient on its own for very long inactivity windows.
pub const VAULT_EXTEND_AMOUNT: u32 = 180 * DAY_IN_LEDGERS;
pub const VAULT_TTL_THRESHOLD: u32 = VAULT_EXTEND_AMOUNT - DAY_IN_LEDGERS;

// ################## STORAGE HELPERS ##################

fn load(e: &Env) -> VaultData {
    let key = InheritanceVaultStorageKey::Vault;
    e.storage().instance().extend_ttl(VAULT_TTL_THRESHOLD, VAULT_EXTEND_AMOUNT);
    e.storage().instance().get(&key).unwrap()
}

fn store(e: &Env, data: &VaultData) {
    let key = InheritanceVaultStorageKey::Vault;
    e.storage().instance().set(&key, data);
    e.storage().instance().extend_ttl(VAULT_TTL_THRESHOLD, VAULT_EXTEND_AMOUNT);
}

fn is_claimable_inner(e: &Env, data: &VaultData) -> bool {
    let threshold = data.last_active_ledger.saturating_add(data.inactivity_period_ledgers);
    e.ledger().sequence() >= threshold
}

fn require_not_claimable(e: &Env, data: &VaultData) {
    if is_claimable_inner(e, data) {
        panic_with_error!(e, InheritanceVaultError::AlreadyClaimable)
    }
}

#[contract]
pub struct InheritanceVault;

#[contractimpl]
impl InheritanceVault {
    /// Sets `owner`, `beneficiary`, and `inactivity_period_ledgers`, and
    /// seeds `last_active_ledger` to the deployment ledger.
    ///
    /// # Errors
    ///
    /// * [`InheritanceVaultError::InvalidBeneficiary`] - If `beneficiary ==
    ///   owner`.
    /// * [`InheritanceVaultError::InvalidInactivityPeriod`] - If
    ///   `inactivity_period_ledgers == 0`.
    pub fn __constructor(
        e: Env,
        owner: Address,
        beneficiary: Address,
        inactivity_period_ledgers: u32,
    ) {
        if beneficiary == owner {
            panic_with_error!(&e, InheritanceVaultError::InvalidBeneficiary)
        }
        if inactivity_period_ledgers == 0 {
            panic_with_error!(&e, InheritanceVaultError::InvalidInactivityPeriod)
        }

        store(
            &e,
            &VaultData {
                owner,
                beneficiary,
                inactivity_period_ledgers,
                last_active_ledger: e.ledger().sequence(),
            },
        );
    }

    /// Resets the inactivity clock to now. The only method `owner` needs to
    /// call periodically to keep the vault under their own control.
    ///
    /// # Errors
    ///
    /// * [`InheritanceVaultError::AlreadyClaimable`] - If the inactivity
    ///   threshold has already been reached — see the module-level "no
    ///   owner override" warning.
    ///
    /// # Events
    ///
    /// * topics - `["checked_in", owner: Address]`
    /// * data - `[last_active_ledger: u32]`
    pub fn check_in(e: &Env) {
        let mut data = load(e);
        data.owner.require_auth();
        require_not_claimable(e, &data);

        data.last_active_ledger = e.ledger().sequence();
        let last_active_ledger = data.last_active_ledger;
        let owner = data.owner.clone();
        store(e, &data);

        CheckedIn { owner, last_active_ledger }.publish(e);
    }

    /// Changes the beneficiary. Owner-gated, and only available before the
    /// vault becomes claimable.
    ///
    /// # Errors
    ///
    /// * [`InheritanceVaultError::AlreadyClaimable`] - If the inactivity
    ///   threshold has already been reached.
    /// * [`InheritanceVaultError::InvalidBeneficiary`] - If `new_beneficiary
    ///   == owner`.
    ///
    /// # Events
    ///
    /// * topics - `["beneficiary_updated", owner: Address]`
    /// * data - `[old_beneficiary: Address, new_beneficiary: Address]`
    pub fn update_beneficiary(e: &Env, new_beneficiary: Address) {
        let mut data = load(e);
        data.owner.require_auth();
        require_not_claimable(e, &data);

        if new_beneficiary == data.owner {
            panic_with_error!(e, InheritanceVaultError::InvalidBeneficiary)
        }

        let owner = data.owner.clone();
        let old_beneficiary = data.beneficiary.clone();
        data.beneficiary = new_beneficiary.clone();
        store(e, &data);

        BeneficiaryUpdated { owner, old_beneficiary, new_beneficiary }.publish(e);
    }

    /// Changes the inactivity period. Owner-gated, and only available before
    /// the vault becomes claimable.
    ///
    /// # Errors
    ///
    /// * [`InheritanceVaultError::AlreadyClaimable`] - If the inactivity
    ///   threshold has already been reached.
    /// * [`InheritanceVaultError::InvalidInactivityPeriod`] - If
    ///   `new_inactivity_period_ledgers == 0`.
    ///
    /// # Events
    ///
    /// * topics - `["inactivity_period_updated", owner: Address]`
    /// * data - `[old_inactivity_period_ledgers: u32, new_inactivity_period_ledgers: u32]`
    pub fn extend_inactivity_period(e: &Env, new_inactivity_period_ledgers: u32) {
        let mut data = load(e);
        data.owner.require_auth();
        require_not_claimable(e, &data);

        if new_inactivity_period_ledgers == 0 {
            panic_with_error!(e, InheritanceVaultError::InvalidInactivityPeriod)
        }

        let owner = data.owner.clone();
        let old_inactivity_period_ledgers = data.inactivity_period_ledgers;
        data.inactivity_period_ledgers = new_inactivity_period_ledgers;
        store(e, &data);

        InactivityPeriodUpdated {
            owner,
            old_inactivity_period_ledgers,
            new_inactivity_period_ledgers,
        }
        .publish(e);
    }

    /// Transfers this contract's entire balance of `token` to `to`.
    /// Requires `beneficiary`'s authorization and that the inactivity
    /// threshold has been reached. Releases everything at once — there is no
    /// partial-claim or beneficiary-side "wait" mechanism.
    ///
    /// # Errors
    ///
    /// * [`InheritanceVaultError::NotYetClaimable`] - If the inactivity
    ///   threshold has not been reached.
    /// * [`InheritanceVaultError::NoFundsToClaim`] - If this contract holds
    ///   no balance of `token`.
    ///
    /// # Events
    ///
    /// * topics - `["claimed", beneficiary: Address]`
    /// * data - `[token: Address, to: Address, amount: i128]`
    pub fn claim(e: &Env, token: Address, to: Address) -> i128 {
        let data = load(e);
        data.beneficiary.require_auth();

        if !is_claimable_inner(e, &data) {
            panic_with_error!(e, InheritanceVaultError::NotYetClaimable)
        }

        let token_client = token::TokenClient::new(e, &token);
        let amount = token_client.balance(&e.current_contract_address());
        if amount <= 0 {
            panic_with_error!(e, InheritanceVaultError::NoFundsToClaim)
        }

        token_client.transfer(&e.current_contract_address(), &to, &amount);

        Claimed { beneficiary: data.beneficiary, token, to, amount }.publish(e);

        amount
    }

    /// Returns whether the inactivity threshold has been reached (i.e.
    /// whether `beneficiary` can currently call `claim`).
    pub fn is_claimable(e: &Env) -> bool {
        let data = load(e);
        is_claimable_inner(e, &data)
    }

    /// Returns the vault's current configuration and liveness state.
    pub fn get_vault_data(e: &Env) -> VaultData {
        load(e)
    }
}

#[cfg(test)]
mod test;
