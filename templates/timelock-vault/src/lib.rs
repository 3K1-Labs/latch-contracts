//! # Timelock Vault — per-user savings contract template
//!
//! A simple, single-purpose contract that users deploy from their own Latch
//! smart account. Funds (native XLM or any SAC/SEP-41 token) sent to the
//! vault stay locked until a specified ledger sequence, regardless of who is
//! asking — the timelock is enforced unconditionally. Withdrawal additionally
//! requires owner authorization, so the vault adds a time gate *on top of*
//! the account's existing signer/policy stack; it never replaces it.
//!
//! # Deposit
//!
//! No explicit `deposit()` is *required* — anyone can send funds to the
//! vault's contract address via a plain `token::transfer`. A thin
//! [`deposit()`](TimelockVault::deposit) convenience method is provided
//! anyway to emit a [`Deposited`] event for indexer and audit-trail
//! purposes.
//!
//! # Withdrawal
//!
//! [`withdraw()`](TimelockVault::withdraw) enforces **both**:
//!
//! 1. `owner.require_auth()` — only the owning smart account may withdraw.
//! 2. `e.ledger().sequence() >= unlock_ledger` — the ledger must be at or past
//!    the unlock point.
//!
//! Neither check alone is sufficient: owner-only would defeat the timelock,
//! ledger-only would let anyone drain it.
//!
//! # Immutability
//!
//! `unlock_ledger` is set once in the constructor and cannot be changed. An
//! `extend_lock()` (push-only, never earlier) is a reasonable follow-up if
//! there is demand, but for a first version immutability is simpler and
//! easier to audit.
//!
//! # What This Is Not
//!
//! - Not a vesting schedule (linear/cliff release).
//! - Not a dead-man's switch or inheritance vault.
//! - Not a recurring payment escrow.
//! - Not a shared singleton — each user deploys their own instance.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, token,
    Address, Env,
};

// ────────────────────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────────────────────

/// Persistent storage keys.
#[contracttype]
pub enum DataKey {
    /// The smart-account address that owns this vault.
    Owner,
    /// Ledger sequence at which withdrawal becomes possible.
    UnlockLedger,
}

/// Contract error codes.
///
/// Numbering starts fresh at `1` — this is a standalone template, not part of
/// the upstream `stellar-accounts` error-numbering convention.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum VaultError {
    /// `withdraw` was called before `unlock_ledger`.
    StillLocked = 1,
    /// The constructor received an `unlock_ledger` that is not in the future.
    InvalidUnlockLedger = 2,
}

// ────────────────────────────────────────────────────────────────────────────
// Events
// ────────────────────────────────────────────────────────────────────────────

/// Emitted by [`TimelockVault::deposit`] for indexing purposes.
#[contractevent]
#[derive(Clone, Debug)]
pub struct Deposited {
    #[topic]
    pub token: Address,
    pub from: Address,
    pub amount: i128,
}

/// Emitted by [`TimelockVault::withdraw`] on a successful withdrawal.
#[contractevent]
#[derive(Clone, Debug)]
pub struct Withdrawn {
    #[topic]
    pub token: Address,
    pub to: Address,
    pub amount: i128,
}

// ────────────────────────────────────────────────────────────────────────────
// Constants
// ────────────────────────────────────────────────────────────────────────────

const DAY_IN_LEDGERS: u32 = 17_280;
/// How far to extend persistent-entry TTLs on access.
const EXTEND_AMOUNT: u32 = 120 * DAY_IN_LEDGERS; // ~120 days
/// Minimum remaining TTL before we bother extending.
const TTL_THRESHOLD: u32 = EXTEND_AMOUNT - DAY_IN_LEDGERS;

// ────────────────────────────────────────────────────────────────────────────
// Contract
// ────────────────────────────────────────────────────────────────────────────

#[contract]
pub struct TimelockVault;

#[contractimpl]
impl TimelockVault {
    // ── Constructor ────────────────────────────────────────────────────

    /// Initializes the vault.
    ///
    /// # Arguments
    ///
    /// * `owner` — the user's smart-account C-address. Withdrawal goes through
    ///   the account's full signer/policy stack; the vault only adds the time
    ///   lock on top.
    /// * `unlock_ledger` — the ledger sequence at or after which withdrawal
    ///   becomes possible. Must be strictly greater than the current ledger.
    ///
    /// # Errors
    ///
    /// * [`VaultError::InvalidUnlockLedger`] — `unlock_ledger` is not in the
    ///   future.
    pub fn __constructor(e: &Env, owner: Address, unlock_ledger: u32) {
        if unlock_ledger <= e.ledger().sequence() {
            panic_with_error!(e, VaultError::InvalidUnlockLedger);
        }

        e.storage().persistent().set(&DataKey::Owner, &owner);
        e.storage().persistent().set(&DataKey::UnlockLedger, &unlock_ledger);

        // Ensure the config outlives the lock itself.
        e.storage().persistent().extend_ttl(&DataKey::Owner, TTL_THRESHOLD, EXTEND_AMOUNT);
        e.storage().persistent().extend_ttl(&DataKey::UnlockLedger, TTL_THRESHOLD, EXTEND_AMOUNT);
    }

    // ── Deposit ────────────────────────────────────────────────────────

    /// Convenience method that transfers tokens into the vault and emits a
    /// [`Deposited`] event for indexing.
    ///
    /// Not strictly required — anyone can `token::transfer` directly to the
    /// vault address. This method exists purely so that deposits are
    /// discoverable by event-based indexers without scanning every token
    /// contract's transfer logs.
    ///
    /// # Arguments
    ///
    /// * `from` — the depositor. `from.require_auth()` is called.
    /// * `token` — the SEP-41 / SAC token contract address.
    /// * `amount` — stroops (or equivalent smallest unit) to deposit.
    pub fn deposit(e: Env, from: Address, token: Address, amount: i128) {
        from.require_auth();

        let vault_addr = e.current_contract_address();
        token::TokenClient::new(&e, &token).transfer(&from, &vault_addr, &amount);

        Deposited { token, from, amount }.publish(&e);
    }

    // ── Withdraw ───────────────────────────────────────────────────────

    /// Transfers tokens out of the vault. Requires **both** owner
    /// authorization and the ledger being at or past `unlock_ledger`.
    ///
    /// # Arguments
    ///
    /// * `token` — the SEP-41 / SAC token contract address to withdraw.
    /// * `amount` — stroops (or equivalent smallest unit) to withdraw.
    /// * `to` — recipient address.
    ///
    /// # Errors
    ///
    /// * [`VaultError::StillLocked`] — current ledger is before
    ///   `unlock_ledger`.
    /// * Panics via `require_auth` if caller is not the owner.
    pub fn withdraw(e: Env, token: Address, amount: i128, to: Address) {
        // ── Owner gate ──
        let owner: Address = e.storage().persistent().get(&DataKey::Owner).unwrap();
        owner.require_auth();

        // ── Time gate ──
        let unlock_ledger: u32 = e.storage().persistent().get(&DataKey::UnlockLedger).unwrap();
        if e.ledger().sequence() < unlock_ledger {
            panic_with_error!(&e, VaultError::StillLocked);
        }

        // ── Transfer ──
        let vault_addr = e.current_contract_address();
        token::TokenClient::new(&e, &token).transfer(&vault_addr, &to, &amount);

        // Bump TTLs so the vault stays alive for future withdrawals.
        e.storage().persistent().extend_ttl(&DataKey::Owner, TTL_THRESHOLD, EXTEND_AMOUNT);
        e.storage().persistent().extend_ttl(&DataKey::UnlockLedger, TTL_THRESHOLD, EXTEND_AMOUNT);

        Withdrawn { token, to, amount }.publish(&e);
    }

    // ── Read-only queries ──────────────────────────────────────────────

    /// Returns the vault owner's address.
    pub fn get_owner(e: &Env) -> Address {
        e.storage().persistent().get(&DataKey::Owner).unwrap()
    }

    /// Returns the ledger sequence at which withdrawal unlocks.
    pub fn get_unlock_ledger(e: &Env) -> u32 {
        e.storage().persistent().get(&DataKey::UnlockLedger).unwrap()
    }

    /// Returns the vault's current balance of the given token.
    pub fn get_balance(e: &Env, token: Address) -> i128 {
        token::TokenClient::new(e, &token).balance(&e.current_contract_address())
    }
}

#[cfg(test)]
mod test;
