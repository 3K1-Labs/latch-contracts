//! SEP-40 / Reflector-compatible oracle interface.
//!
//! Everything specific to *talking to the oracle* lives here — the wire
//! types it returns, and the two cross-contract calls this policy makes
//! against it (`decimals`, `lastprice`). `lib.rs` only ever sees the
//! results (`fetch_usd_divisor`, `fetch_price`), not the oracle's shape.

use soroban_sdk::{contracttype, panic_with_error, symbol_short, Address, Env, IntoVal, Symbol};

use crate::Error;

/// A generous upper bound on a plausible token `decimals()` value. Guards
/// `10i128.checked_pow` against overflow if a misconfigured or malicious
/// token address returns something absurd. Real SEP-41 tokens are 7 on
/// Stellar; nothing legitimate approaches this.
const MAX_TOKEN_DECIMALS: u32 = 20;

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

/// Queries a token contract's `decimals()` and converts it into the base-10
/// divisor `10.pow(decimals)` used to turn a raw `transfer` amount (in the
/// token's own smallest unit) into a whole-token count during USD accounting.
///
/// Called once per allowed token at `install` time — a token's decimals do
/// not change — and the result is cached in `PolicyData::token_divisors`,
/// parallel to `allowed_tokens`. Fails closed ([`Error::InvalidTokenResponse`])
/// if the call reverts, the address is not a SEP-41 token, or it reports a
/// `decimals` value too large to build a divisor from.
pub fn fetch_token_divisor(e: &Env, token: &Address) -> i128 {
    let decimals: u32 = e.invoke_contract(token, &DECIMALS_FN, soroban_sdk::vec![e]);
    if decimals > MAX_TOKEN_DECIMALS {
        panic_with_error!(e, Error::InvalidTokenResponse)
    }
    10i128
        .checked_pow(decimals)
        .unwrap_or_else(|| panic_with_error!(e, Error::InvalidTokenResponse))
}

/// Queries the oracle's `lastprice` for `token`, failing closed if the
/// oracle call reverts, returns `None` (no price known for that asset), or
/// returns a value that can't be trusted for USD accounting.
///
/// A returned `PriceData` is guaranteed to have `price > 0` and a
/// `timestamp` no later than the current ledger. Both are enforced here
/// rather than in `enforce`, because a non-positive price would make
/// `amount_usd` zero or negative downstream (a transfer that consumes no
/// limit, or one that *reduces* the running total), and a future timestamp
/// would slip past `enforce`'s staleness check (its `saturating_sub` floors
/// at zero). A misconfigured or compromised oracle must never be able to
/// *loosen* the limit.
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
    let price_data =
        price_data.unwrap_or_else(|| panic_with_error!(e, Error::InvalidOracleResponse));

    if price_data.price <= 0 || price_data.timestamp > e.ledger().timestamp() {
        panic_with_error!(e, Error::InvalidOracleResponse)
    }

    price_data
}
