//! Vesting schedule personal contract template for Latch smart accounts.
//!
//! A lightweight satellite contract owned by a user's smart account (or account
//! address) that holds SEP-41 tokens and releases them over time according to a
//! linear schedule, with an optional cliff.
//!
//! # Architecture & Trust Model
//!
//! - **Personal contract**: Deployed per schedule, owned by the beneficiary's
//!   smart account.
//! - **Single-token design**: Configured with a dedicated token address at
//!   construction time.
//! - **Owner authorization**: Only the configured `owner` address can invoke
//!   `claim`.
//! - **Non-revocable**: Once deployed and funded, vesting is unconditional and
//!   cannot be cancelled or redirected by a third party.
//! - **Exact arithmetic**: Uses wide integer multiplication to guarantee that
//!   rounding leaves zero residual dust upon schedule completion, with no
//!   overflow up to `i128::MAX`.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, Address,
    Env,
};

// ################## CONSTANTS ##################

const DAY_IN_LEDGERS: u32 = 17280;
const INSTANCE_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
const INSTANCE_TTL_THRESHOLD: u32 = INSTANCE_EXTEND_AMOUNT - DAY_IN_LEDGERS;

// ################## STORAGE & TYPES ##################

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Schedule,
}

/// Stored configuration and state of a vesting schedule.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleData {
    /// Owner (beneficiary) address authorized to claim vested tokens.
    pub owner: Address,
    /// SEP-41 token contract address being vested.
    pub token: Address,
    /// Total amount of tokens to vest across the entire schedule.
    pub total_amount: i128,
    /// Ledger sequence at which linear vesting begins.
    pub start_ledger: u32,
    /// Optional cliff ledger sequence before which zero tokens are claimable.
    pub cliff_ledger: Option<u32>,
    /// Ledger sequence at which 100% of tokens are vested.
    pub end_ledger: u32,
    /// Cumulative token amount claimed so far.
    pub claimed_amount: i128,
}

// ################## ERRORS ##################

/// Error codes for the vesting schedule contract.
///
/// Standalone contract starting error numbering at `1`.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VestingScheduleError {
    /// The contract has already been initialized.
    AlreadyInitialized = 1,
    /// Total amount must be strictly greater than zero.
    InvalidAmount = 2,
    /// `start_ledger` must be strictly less than `end_ledger`.
    InvalidLedgerRange = 3,
    /// `cliff_ledger` must be between `start_ledger` and `end_ledger`
    /// inclusive.
    InvalidCliffLedger = 4,
    /// The vesting contract has not been initialized.
    NotInitialized = 5,
}

// ################## EVENTS ##################

/// Event emitted when a vesting schedule contract is constructed.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleCreated {
    #[topic]
    pub owner: Address,
    #[topic]
    pub token: Address,
    pub total_amount: i128,
    pub start_ledger: u32,
    pub cliff_ledger: Option<u32>,
    pub end_ledger: u32,
}

/// Event emitted when vested tokens are claimed.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Claimed {
    #[topic]
    pub owner: Address,
    #[topic]
    pub to: Address,
    pub amount: i128,
    pub ledger: u32,
}

/// Emits the [`ScheduleCreated`] event.
pub fn emit_schedule_created(
    e: &Env,
    owner: &Address,
    token: &Address,
    total_amount: i128,
    start_ledger: u32,
    cliff_ledger: Option<u32>,
    end_ledger: u32,
) {
    ScheduleCreated {
        owner: owner.clone(),
        token: token.clone(),
        total_amount,
        start_ledger,
        cliff_ledger,
        end_ledger,
    }
    .publish(e);
}

/// Emits the [`Claimed`] event.
pub fn emit_claimed(e: &Env, owner: &Address, to: &Address, amount: i128, ledger: u32) {
    Claimed { owner: owner.clone(), to: to.clone(), amount, ledger }.publish(e);
}

// ################## CONTRACT ##################

#[contract]
pub struct VestingScheduleContract;

#[contractimpl]
impl VestingScheduleContract {
    /// Initializes a new vesting schedule.
    ///
    /// # Arguments
    ///
    /// * `e` - The Soroban environment.
    /// * `owner` - The account authorized to claim vested tokens.
    /// * `token` - The SEP-41 token contract being vested.
    /// * `total_amount` - The total token amount to vest (must be > 0).
    /// * `start_ledger` - The ledger sequence where vesting starts.
    /// * `cliff_ledger` - Optional cliff ledger (must be `start_ledger <= cliff
    ///   <= end_ledger`).
    /// * `end_ledger` - The ledger sequence where vesting completes (must be >
    ///   `start_ledger`).
    ///
    /// # Errors
    ///
    /// * [`VestingScheduleError::AlreadyInitialized`] - If the contract was
    ///   already initialized.
    /// * [`VestingScheduleError::InvalidAmount`] - If `total_amount <= 0`.
    /// * [`VestingScheduleError::InvalidLedgerRange`] - If `start_ledger >=
    ///   end_ledger`.
    /// * [`VestingScheduleError::InvalidCliffLedger`] - If `cliff_ledger` is
    ///   out of bounds.
    ///
    /// # Events
    ///
    /// Emits [`ScheduleCreated`].
    pub fn __constructor(
        e: Env,
        owner: Address,
        token: Address,
        total_amount: i128,
        start_ledger: u32,
        cliff_ledger: Option<u32>,
        end_ledger: u32,
    ) {
        if e.storage().instance().has(&DataKey::Schedule) {
            panic_with_error!(&e, VestingScheduleError::AlreadyInitialized);
        }
        if total_amount <= 0 {
            panic_with_error!(&e, VestingScheduleError::InvalidAmount);
        }
        if start_ledger >= end_ledger {
            panic_with_error!(&e, VestingScheduleError::InvalidLedgerRange);
        }
        if let Some(cliff) = cliff_ledger {
            if cliff < start_ledger || cliff > end_ledger {
                panic_with_error!(&e, VestingScheduleError::InvalidCliffLedger);
            }
        }

        let schedule = ScheduleData {
            owner: owner.clone(),
            token: token.clone(),
            total_amount,
            start_ledger,
            cliff_ledger,
            end_ledger,
            claimed_amount: 0,
        };

        e.storage().instance().set(&DataKey::Schedule, &schedule);
        extend_instance_ttl(&e);

        emit_schedule_created(
            &e,
            &owner,
            &token,
            total_amount,
            start_ledger,
            cliff_ledger,
            end_ledger,
        );
    }

    /// Claims all currently vested and unclaimed tokens, transferring them to
    /// `to`.
    ///
    /// Requires authorization from `owner`. Returns the amount of tokens
    /// transferred (0 if nothing is currently claimable).
    ///
    /// # Arguments
    ///
    /// * `e` - The Soroban environment.
    /// * `to` - The recipient address to receive the released tokens.
    ///
    /// # Errors
    ///
    /// * [`VestingScheduleError::NotInitialized`] - If the contract has not
    ///   been initialized.
    ///
    /// # Events
    ///
    /// Emits [`Claimed`] if `claimable > 0`.
    pub fn claim(e: Env, to: Address) -> i128 {
        extend_instance_ttl(&e);
        let mut schedule = get_schedule_data(&e);
        schedule.owner.require_auth();

        let current_ledger = e.ledger().sequence();
        let vested = compute_vested(
            schedule.total_amount,
            schedule.start_ledger,
            schedule.cliff_ledger,
            schedule.end_ledger,
            current_ledger,
        );

        let claimable = vested.saturating_sub(schedule.claimed_amount);
        if claimable > 0 {
            schedule.claimed_amount =
                schedule.claimed_amount.checked_add(claimable).unwrap_or(schedule.total_amount);

            e.storage().instance().set(&DataKey::Schedule, &schedule);

            soroban_sdk::token::Client::new(&e, &schedule.token).transfer(
                &e.current_contract_address(),
                &to,
                &claimable,
            );

            emit_claimed(&e, &schedule.owner, &to, claimable, current_ledger);
        }

        claimable
    }

    /// Returns the full stored schedule data.
    pub fn get_schedule(e: Env) -> ScheduleData {
        extend_instance_ttl(&e);
        get_schedule_data(&e)
    }

    /// Returns the total amount vested as of the current ledger.
    pub fn vested_amount(e: Env) -> i128 {
        extend_instance_ttl(&e);
        let schedule = get_schedule_data(&e);
        compute_vested(
            schedule.total_amount,
            schedule.start_ledger,
            schedule.cliff_ledger,
            schedule.end_ledger,
            e.ledger().sequence(),
        )
    }

    /// Returns the amount currently claimable (`vested_amount -
    /// claimed_amount`).
    pub fn claimable_amount(e: Env) -> i128 {
        extend_instance_ttl(&e);
        let schedule = get_schedule_data(&e);
        let vested = compute_vested(
            schedule.total_amount,
            schedule.start_ledger,
            schedule.cliff_ledger,
            schedule.end_ledger,
            e.ledger().sequence(),
        );
        vested.saturating_sub(schedule.claimed_amount)
    }

    /// Returns the cumulative amount claimed so far.
    pub fn claimed_amount(e: Env) -> i128 {
        extend_instance_ttl(&e);
        get_schedule_data(&e).claimed_amount
    }

    /// Returns the schedule owner address.
    pub fn owner(e: Env) -> Address {
        extend_instance_ttl(&e);
        get_schedule_data(&e).owner
    }

    /// Returns the vested token address.
    pub fn token(e: Env) -> Address {
        extend_instance_ttl(&e);
        get_schedule_data(&e).token
    }
}

// ################## HELPERS ##################

fn extend_instance_ttl(e: &Env) {
    e.storage().instance().extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_EXTEND_AMOUNT);
}

fn get_schedule_data(e: &Env) -> ScheduleData {
    e.storage()
        .instance()
        .get(&DataKey::Schedule)
        .unwrap_or_else(|| panic_with_error!(e, VestingScheduleError::NotInitialized))
}

/// Computes the cumulative amount vested at `current_ledger`.
///
/// - Returns `0` before `start_ledger`.
/// - Returns `0` before `cliff_ledger` (if set).
/// - Returns `total_amount` at or after `end_ledger`.
/// - Otherwise returns `(total_amount * (current_ledger - start_ledger)) /
///   (end_ledger - start_ledger)`.
pub fn compute_vested(
    total_amount: i128,
    start_ledger: u32,
    cliff_ledger: Option<u32>,
    end_ledger: u32,
    current_ledger: u32,
) -> i128 {
    if current_ledger < start_ledger {
        return 0;
    }
    if let Some(cliff) = cliff_ledger {
        if current_ledger < cliff {
            return 0;
        }
    }
    if current_ledger >= end_ledger {
        return total_amount;
    }

    let elapsed = (current_ledger - start_ledger) as u64;
    let duration = (end_ledger - start_ledger) as u64;

    mul_div_u128(total_amount as u128, elapsed, duration) as i128
}

/// Exact multiplication and division: `(total * elapsed) / duration`.
///
/// Avoids 128-bit overflow by splitting high and low 64-bit words when `total *
/// elapsed` would exceed `u128::MAX`.
fn mul_div_u128(total: u128, elapsed: u64, duration: u64) -> u128 {
    let elapsed = elapsed as u128;
    let duration = duration as u128;

    if let Some(prod) = total.checked_mul(elapsed) {
        prod / duration
    } else {
        let hi = (total >> 64) as u64 as u128;
        let lo = (total as u64) as u128;
        let hi_prod = hi * elapsed;
        let lo_prod = lo * elapsed;

        let hi_div = hi_prod / duration;
        let hi_rem = hi_prod % duration;

        let lo_sum = (hi_rem << 64) + lo_prod;
        let lo_div = lo_sum / duration;

        (hi_div << 64) + lo_div
    }
}

#[cfg(test)]
mod test;
