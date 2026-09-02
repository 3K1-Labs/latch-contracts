//! # Recurring Escrow — pre-funded recurring payment contract template
//!
//! A single-purpose satellite contract deployed from a user's Latch smart
//! account. The owner deposits a lump sum of a single SEP-41 token up front;
//! a designated payee can then pull a fixed amount on a fixed schedule (measured
//! in ledger sequences), up to the funded total, without needing the owner to
//! re-authorize every single payment.
//!
//! # Deposit (top-up)
//!
//! No explicit `deposit()` method — the owner (or anyone) simply sends more
//! tokens directly to the contract's address via a plain `token::transfer`.
//!
//! # Payment lifecycle
//!
//! 1. The payee calls [`pull()`](RecurringEscrow::pull) when they expect a
//!    payment.
//! 2. The contract checks that at least `period_ledgers` have elapsed since
//!    the last successful pull (or since deployment for the first pull).
//! 3. The contract checks that its own balance covers `amount_per_period` in
//!    full.
//! 4. If both checks pass, the full `amount_per_period` is transferred to the
//!    specified recipient and the `last_pull_ledger` is updated.
//! 5. If either check fails, the call reverts — no partial release.
//!
//! # Cancellation
//!
//! The owner may cancel at any time via [`cancel()`](RecurringEscrow::cancel),
//! which transfers the entire remaining balance back to a specified address
//! and permanently disables future `pull()` calls. Unlike an inheritance vault
//! there is no second-party trust concern here — the payee never has a
//! competing claim to funds not yet pulled.
//!
//! # What This Is Not
//!
//! - Not a variable / escalating payment schedule — fixed amount per period.
//! - Not a multi-payee contract — one payee per deployment.
//! - Not an automatic push-payment — the payee must actively call `pull`;
//!   Soroban has no native cron primitive.
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
    /// The smart-account address that owns this escrow.
    Owner,
    /// The address authorized to pull payments.
    Payee,
    /// The SEP-41 token held by this escrow.
    Token,
    /// Number of stroops released per period.
    AmountPerPeriod,
    /// Number of ledgers that must elapse between pulls.
    PeriodLedgers,
    /// Ledger sequence of the last successful pull (0 before first pull).
    LastPullLedger,
    /// Whether the escrow has been cancelled.
    Cancelled,
}

/// Contract error codes.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    /// `amount_per_period` was not positive.
    InvalidAmount = 1,
    /// `period_ledgers` was zero.
    InvalidPeriod = 2,
    /// `pull()` was called before a full period had elapsed.
    TooEarly = 3,
    /// The contract balance is insufficient to cover `amount_per_period`.
    InsufficientBalance = 4,
    /// The escrow has been cancelled; no further pulls are allowed.
    Cancelled = 5,
}

// ────────────────────────────────────────────────────────────────────────────
// Events
// ────────────────────────────────────────────────────────────────────────────

/// Emitted on the first pull (or when `last_pull_ledger` is 0) to mark the
/// start of the schedule, and on every subsequent successful pull.
#[contractevent]
#[derive(Clone, Debug)]
pub struct Pullled {
    #[topic]
    pub payee: Address,
    pub to: Address,
    pub amount: i128,
    pub ledger: u32,
}

/// Emitted when the owner cancels the escrow.
#[contractevent]
#[derive(Clone, Debug)]
pub struct Cancelled {
    #[topic]
    pub owner: Address,
    pub to: Address,
    pub refunded: i128,
}

// ────────────────────────────────────────────────────────────────────────────
// Constants
// ────────────────────────────────────────────────────────────────────────────

const DAY_IN_LEDGERS: u32 = 17_280;
const EXTEND_AMOUNT: u32 = 120 * DAY_IN_LEDGERS;
const TTL_THRESHOLD: u32 = EXTEND_AMOUNT - DAY_IN_LEDGERS;

// ────────────────────────────────────────────────────────────────────────────
// Contract
// ────────────────────────────────────────────────────────────────────────────

#[contract]
pub struct RecurringEscrow;

#[contractimpl]
impl RecurringEscrow {
    // ── Constructor ────────────────────────────────────────────────────

    /// Initializes the recurring escrow.
    ///
    /// # Arguments
    ///
    /// * `owner` — the smart-account address that funds and controls the
    ///   escrow.
    /// * `payee` — the address authorized to pull periodic payments.
    /// * `amount_per_period` — number of stroops released per period (must be
    ///   > 0).
    /// * `period_ledgers` — number of ledgers between allowed pulls (must be
    ///   > 0).
    /// * `token` — the SEP-41 token contract address.
    pub fn __constructor(
        e: &Env,
        owner: Address,
        payee: Address,
        amount_per_period: i128,
        period_ledgers: u32,
        token: Address,
    ) {
        if amount_per_period <= 0 {
            panic_with_error!(e, EscrowError::InvalidAmount);
        }
        if period_ledgers == 0 {
            panic_with_error!(e, EscrowError::InvalidPeriod);
        }

        e.storage().persistent().set(&DataKey::Owner, &owner);
        e.storage().persistent().set(&DataKey::Payee, &payee);
        e.storage().persistent().set(&DataKey::Token, &token);
        e.storage().persistent().set(&DataKey::AmountPerPeriod, &amount_per_period);
        e.storage().persistent().set(&DataKey::PeriodLedgers, &period_ledgers);
        e.storage().persistent().set(&DataKey::LastPullLedger, &0u32);
        e.storage().persistent().set(&DataKey::Cancelled, &false);

        e.storage().persistent().extend_ttl(&DataKey::Owner, TTL_THRESHOLD, EXTEND_AMOUNT);
        e.storage().persistent().extend_ttl(&DataKey::Payee, TTL_THRESHOLD, EXTEND_AMOUNT);
        e.storage().persistent().extend_ttl(&DataKey::Token, TTL_THRESHOLD, EXTEND_AMOUNT);
    }

    // ── Pull ───────────────────────────────────────────────────────────

    /// Releases one period's worth of tokens to `to`.
    ///
    /// Requires `payee.require_auth()`. Reverts if:
    /// - Less than `period_ledgers` have elapsed since the last pull (or
    ///   deployment for the first pull).
    /// - The contract balance is less than `amount_per_period`.
    /// - The escrow has been cancelled.
    pub fn pull(e: Env, to: Address) {
        // ── Auth gate ──
        let payee: Address = e.storage().persistent().get(&DataKey::Payee).unwrap();
        payee.require_auth();

        // ── Cancelled check ──
        let cancelled: bool = e.storage().persistent().get(&DataKey::Cancelled).unwrap();
        if cancelled {
            panic_with_error!(&e, EscrowError::Cancelled);
        }

        // ── Period gate ──
        let period_ledgers: u32 =
            e.storage().persistent().get(&DataKey::PeriodLedgers).unwrap();
        let last_pull: u32 = e.storage().persistent().get(&DataKey::LastPullLedger).unwrap();
        let current = e.ledger().sequence();

        if last_pull == 0 {
            // First pull: no wait required (schedule starts at deployment).
        } else if current < last_pull + period_ledgers {
            panic_with_error!(&e, EscrowError::TooEarly);
        }

        // ── Balance gate ──
        let token_addr: Address = e.storage().persistent().get(&DataKey::Token).unwrap();
        let amount: i128 =
            e.storage().persistent().get(&DataKey::AmountPerPeriod).unwrap();
        let balance =
            token::TokenClient::new(&e, &token_addr).balance(&e.current_contract_address());

        if balance < amount {
            panic_with_error!(&e, EscrowError::InsufficientBalance);
        }

        // ── Transfer ──
        token::TokenClient::new(&e, &token_addr)
            .transfer(&e.current_contract_address(), &to, &amount);

        // ── Update state ──
        e.storage().persistent().set(&DataKey::LastPullLedger, &current);

        Self::extend_ttl(&e);

        Pullled { payee, to, amount, ledger: current }.publish(&e);
    }

    // ── Cancel ─────────────────────────────────────────────────────────

    /// Cancels the escrow, transferring the entire remaining token balance to
    /// `to` and permanently disabling future `pull()` calls.
    ///
    /// Requires `owner.require_auth()`.
    pub fn cancel(e: Env, to: Address) {
        let owner: Address = e.storage().persistent().get(&DataKey::Owner).unwrap();
        owner.require_auth();

        // Already cancelled — no-op (idempotent).
        let already: bool = e.storage().persistent().get(&DataKey::Cancelled).unwrap();
        if already {
            return;
        }

        let token_addr: Address = e.storage().persistent().get(&DataKey::Token).unwrap();
        let balance =
            token::TokenClient::new(&e, &token_addr).balance(&e.current_contract_address());

        if balance > 0 {
            token::TokenClient::new(&e, &token_addr)
                .transfer(&e.current_contract_address(), &to, &balance);
        }

        e.storage().persistent().set(&DataKey::Cancelled, &true);

        Self::extend_ttl(&e);

        Cancelled { owner, to, refunded: balance }.publish(&e);
    }

    // ── Read-only queries ──────────────────────────────────────────────

    pub fn get_owner(e: &Env) -> Address {
        e.storage().persistent().get(&DataKey::Owner).unwrap()
    }

    pub fn get_payee(e: &Env) -> Address {
        e.storage().persistent().get(&DataKey::Payee).unwrap()
    }

    pub fn get_token(e: &Env) -> Address {
        e.storage().persistent().get(&DataKey::Token).unwrap()
    }

    pub fn get_amount_per_period(e: &Env) -> i128 {
        e.storage().persistent().get(&DataKey::AmountPerPeriod).unwrap()
    }

    pub fn get_period_ledgers(e: &Env) -> u32 {
        e.storage().persistent().get(&DataKey::PeriodLedgers).unwrap()
    }

    pub fn get_last_pull_ledger(e: &Env) -> u32 {
        e.storage().persistent().get(&DataKey::LastPullLedger).unwrap()
    }

    pub fn is_cancelled(e: &Env) -> bool {
        e.storage().persistent().get(&DataKey::Cancelled).unwrap()
    }

    pub fn get_balance(e: &Env) -> i128 {
        let token_addr: Address = e.storage().persistent().get(&DataKey::Token).unwrap();
        token::TokenClient::new(e, &token_addr).balance(&e.current_contract_address())
    }

    // ── Internal ───────────────────────────────────────────────────────

    fn extend_ttl(e: &Env) {
        for key in [
            DataKey::Owner,
            DataKey::Payee,
            DataKey::Token,
            DataKey::AmountPerPeriod,
            DataKey::PeriodLedgers,
            DataKey::LastPullLedger,
            DataKey::Cancelled,
        ] {
            e.storage().persistent().extend_ttl(&key, TTL_THRESHOLD, EXTEND_AMOUNT);
        }
    }
}

#[cfg(test)]
mod test;
