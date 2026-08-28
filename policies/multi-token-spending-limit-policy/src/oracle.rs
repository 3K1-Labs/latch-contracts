//! SEP-40 / Reflector-compatible oracle interface.
//!
//! Everything specific to *talking to the oracle* lives here — the wire
//! types it returns, and the two cross-contract calls this policy makes
//! against it (`decimals`, `lastprice`). `lib.rs` only ever sees the
//! results (`fetch_usd_divisor`, `fetch_price`), not the oracle's shape.

use soroban_sdk::{contracttype, panic_with_error, symbol_short, Address, Env, IntoVal, Symbol};

use crate::Error;

/// SEP-40 asset descriptor, as understood by a Reflector-compatible oracle.
/// Reflector prices either a Stellar contract address directly, or an
/// external ticker (e.g. `"BTC"`, `"USD"`) on feeds that track off-chain
/// assets — this policy only ever queries the former, one allowed token at
/// a time.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

/// Price feed shape returned by a SEP-40 / Reflector-compatible oracle's
/// `lastprice`. Field names and the enclosing `Option` match the oracle's
/// actual return type so this decodes correctly against a real deployment.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

/// A generous upper bound on a plausible oracle `decimals()` value. Guards
/// `10i128.checked_pow` against overflow if a misconfigured or malicious
/// oracle address returns something absurd.
const MAX_ORACLE_DECIMALS: u32 = 30;

const LASTPRICE_FN: Symbol = symbol_short!("lastprice");
const DECIMALS_FN: Symbol = symbol_short!("decimals");

/// Queries the oracle's `decimals()` and converts it into a base-10 divisor,
/// failing closed if the value can't plausibly be used as one.
///
/// Called once, at `install` time — a given oracle deployment's precision
/// doesn't change afterwards, so there's no need to pay for this call again
/// on every `enforce`.
pub fn fetch_usd_divisor(e: &Env, oracle_address: &Address) -> i128 {
    let decimals: u32 = e.invoke_contract(oracle_address, &DECIMALS_FN, soroban_sdk::vec![e]);
    if decimals > MAX_ORACLE_DECIMALS {
        panic_with_error!(e, Error::InvalidOracleResponse)
    }
    10i128
        .checked_pow(decimals)
        .unwrap_or_else(|| panic_with_error!(e, Error::InvalidOracleResponse))
}

/// Queries the oracle's `lastprice` for `token`, failing closed if the
/// oracle call reverts or returns `None` (no price known for that asset).
///
/// Unlike the divisor, this is deliberately *not* cached — a price is only
/// meaningful at the moment it's read, so `enforce` calls this fresh on
/// every transfer rather than reusing a stored value.
pub fn fetch_price(e: &Env, oracle_address: &Address, token: Address) -> PriceData {
    let price_data: Option<PriceData> = e.invoke_contract(
        oracle_address,
        &LASTPRICE_FN,
        soroban_sdk::vec![e, Asset::Stellar(token).into_val(e)],
    );
    price_data.unwrap_or_else(|| panic_with_error!(e, Error::InvalidOracleResponse))
}
