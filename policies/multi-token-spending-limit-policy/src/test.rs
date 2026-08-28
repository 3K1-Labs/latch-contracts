#![cfg(test)]
extern crate std;

use soroban_sdk::{
    auth::{Context, ContractContext},
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    vec, Address, Env, IntoVal, String, Symbol, Val, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

use crate::{
    Asset, MultiTokenSpendingLimitAccountParams, MultiTokenSpendingLimitPolicy,
    MultiTokenSpendingLimitPolicyClient, PriceData, LEDGER_CLOSE_TIME_SECS, MAX_STALENESS_LEDGERS,
};

/// $1.00 at the mock oracle's default 8-decimal fixed-point convention.
const ONE_USD: i128 = 100_000_000;

// ################## MOCK ORACLE ##################

/// A minimal stand-in for a SEP-40 / Reflector-compatible price oracle.
/// Tests configure the price (and, where relevant, the decimals) it
/// returns; `enforce` and `install` call `lastprice`/`decimals` through
/// `invoke_contract` exactly as they would against a real deployment like
/// Reflector.
#[contract]
struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn set_price(e: Env, price: Option<PriceData>) {
        e.storage().instance().set(&symbol_short!("price"), &price);
    }

    pub fn set_decimals(e: Env, decimals: u32) {
        e.storage().instance().set(&symbol_short!("decimals"), &decimals);
    }

    pub fn lastprice(e: Env, _asset: Asset) -> Option<PriceData> {
        e.storage().instance().get(&symbol_short!("price")).unwrap()
    }

    pub fn decimals(e: Env) -> u32 {
        e.storage().instance().get(&symbol_short!("decimals")).unwrap_or(8)
    }
}

// ################## HELPERS ##################

fn create_context_rule(e: &Env, call_target: Address) -> ContextRule {
    let signer = Address::generate(e);
    let mut signers = Vec::new(e);
    signers.push_back(Signer::Delegated(signer));

    ContextRule {
        id: 1,
        context_type: ContextRuleType::CallContract(call_target),
        name: String::from_str(e, "multi-token-spending-limit"),
        signers,
        signer_ids: Vec::new(e),
        policies: Vec::new(e),
        policy_ids: Vec::new(e),
        valid_until: None,
    }
}

fn setup_env<'a>() -> (Env, Address, MultiTokenSpendingLimitPolicyClient<'a>) {
    let e = Env::default();
    e.mock_all_auths();
    // Non-zero start so `saturating_sub` in the rolling-window math behaves
    // the same way it will on a live ledger, rather than hitting the
    // zero-floor edge case.
    e.ledger().set_sequence_number(1_000);
    e.ledger().set_timestamp(1_000_000);

    let smart_account = Address::generate(&e);
    let contract_id = e.register(MultiTokenSpendingLimitPolicy, ());
    let client = MultiTokenSpendingLimitPolicyClient::new(&e, &contract_id);

    (e, smart_account, client)
}

/// Deploys a mock oracle at the standard 8-decimal precision, matching
/// `ONE_USD`.
fn setup_oracle(e: &Env, price: i128) -> Address {
    setup_oracle_with_decimals(e, price, 8)
}

fn setup_oracle_with_decimals(e: &Env, price: i128, decimals: u32) -> Address {
    let oracle_id = e.register(MockOracle, ());
    let oracle_client = MockOracleClient::new(e, &oracle_id);
    oracle_client.set_decimals(&decimals);
    oracle_client.set_price(&Some(PriceData { price, timestamp: e.ledger().timestamp() }));
    oracle_id
}

fn transfer_context(e: &Env, token: &Address, amount: i128) -> Context {
    let from = Address::generate(e);
    let to = Address::generate(e);
    let args: Vec<Val> = vec![e, from.into_val(e), to.into_val(e), amount.into_val(e)];
    Context::Contract(ContractContext {
        contract: token.clone(),
        fn_name: Symbol::new(e, "transfer"),
        args,
    })
}

fn install_policy(
    e: &Env,
    client: &MultiTokenSpendingLimitPolicyClient,
    smart_account: &Address,
    oracle: &Address,
    allowed_tokens: Vec<Address>,
    spending_limit_usd: i128,
    period_ledgers: u32,
) -> ContextRule {
    let rule = create_context_rule(e, allowed_tokens.get(0).unwrap());
    client.install(
        &MultiTokenSpendingLimitAccountParams {
            spending_limit_usd,
            period_ledgers,
            oracle_address: oracle.clone(),
            allowed_tokens,
        },
        &rule,
        smart_account,
    );
    rule
}

// ################## INSTALL ##################

#[test]
fn test_install_and_get_policy_data() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);
    let tokens = vec![&e, token];

    let rule = install_policy(&e, &client, &smart_account, &oracle, tokens.clone(), 1_000, 100);

    let data = client.get_policy_data(&rule.id, &smart_account);
    assert_eq!(data.spending_limit_usd, 1_000);
    assert_eq!(data.period_ledgers, 100);
    assert_eq!(data.oracle_address, oracle);
    assert_eq!(data.allowed_tokens, tokens);
    assert_eq!(data.cached_total_spent_usd, 0);
    // Cached from the oracle's `decimals() == 8` at install time.
    assert_eq!(data.usd_divisor, ONE_USD);
}

#[test]
fn test_install_caches_non_standard_oracle_decimals() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle_with_decimals(&e, 1_000_000, 6);
    let token = Address::generate(&e);

    let rule = install_policy(&e, &client, &smart_account, &oracle, vec![&e, token], 1_000, 100);

    let data = client.get_policy_data(&rule.id, &smart_account);
    assert_eq!(data.usd_divisor, 1_000_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")] // InvalidContextRule
fn test_install_rejects_non_call_contract() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    let mut rule = create_context_rule(&e, token.clone());
    rule.context_type = ContextRuleType::Default;

    client.install(
        &MultiTokenSpendingLimitAccountParams {
            spending_limit_usd: 1_000,
            period_ledgers: 100,
            oracle_address: oracle,
            allowed_tokens: vec![&e, token],
        },
        &rule,
        &smart_account,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // InvalidInstallParams
fn test_install_rejects_non_positive_limit() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    install_policy(&e, &client, &smart_account, &oracle, vec![&e, token], 0, 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // InvalidInstallParams
fn test_install_rejects_zero_period() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    install_policy(&e, &client, &smart_account, &oracle, vec![&e, token], 1_000, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // InvalidInstallParams
fn test_install_rejects_empty_allowed_tokens() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let rule = create_context_rule(&e, Address::generate(&e));

    client.install(
        &MultiTokenSpendingLimitAccountParams {
            spending_limit_usd: 1_000,
            period_ledgers: 100,
            oracle_address: oracle,
            allowed_tokens: Vec::new(&e),
        },
        &rule,
        &smart_account,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")] // AlreadyInstalled
fn test_install_rejects_double_install() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);
    let tokens = vec![&e, token];

    let rule = install_policy(&e, &client, &smart_account, &oracle, tokens.clone(), 1_000, 100);
    client.install(
        &MultiTokenSpendingLimitAccountParams {
            spending_limit_usd: 1_000,
            period_ledgers: 100,
            oracle_address: oracle,
            allowed_tokens: tokens,
        },
        &rule,
        &smart_account,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")] // InvalidOracleResponse
fn test_install_rejects_oracle_decimals_too_large() {
    let (e, smart_account, client) = setup_env();
    // 31 decimals overflows the `10i128.checked_pow` bound this policy
    // enforces (`MAX_ORACLE_DECIMALS == 30`).
    let oracle = setup_oracle_with_decimals(&e, ONE_USD, 31);
    let token = Address::generate(&e);

    install_policy(&e, &client, &smart_account, &oracle, vec![&e, token], 1_000, 100);
}

// ################## UNINSTALL ##################

#[test]
fn test_uninstall_clears_state() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    let rule = install_policy(&e, &client, &smart_account, &oracle, vec![&e, token], 1_000, 100);
    client.uninstall(&rule, &smart_account);

    let res = client.try_get_policy_data(&rule.id, &smart_account);
    assert!(res.is_err());
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")] // NotInstalled
fn test_uninstall_rejects_not_installed() {
    let (e, smart_account, client) = setup_env();
    let rule = create_context_rule(&e, Address::generate(&e));

    client.uninstall(&rule, &smart_account);
}

// ################## ENFORCE ##################

#[test]
fn test_enforce_accepts_within_limit() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    let rule =
        install_policy(&e, &client, &smart_account, &oracle, vec![&e, token.clone()], 1_000, 100);

    let context = transfer_context(&e, &token, 500);
    client.enforce(&context, &Vec::new(&e), &rule, &smart_account);

    let data = client.get_policy_data(&rule.id, &smart_account);
    assert_eq!(data.cached_total_spent_usd, 500);
    assert_eq!(data.spending_history.len(), 1);
}

#[test]
fn test_enforce_converts_using_non_standard_oracle_decimals() {
    let (e, smart_account, client) = setup_env();
    // A 6-decimal oracle pricing the token at $1.00: 1_000_000 == $1 at
    // 6 decimals, same real-world price as `ONE_USD` at 8 decimals.
    let oracle = setup_oracle_with_decimals(&e, 1_000_000, 6);
    let token = Address::generate(&e);

    let rule =
        install_policy(&e, &client, &smart_account, &oracle, vec![&e, token.clone()], 1_000, 100);

    let context = transfer_context(&e, &token, 500);
    client.enforce(&context, &Vec::new(&e), &rule, &smart_account);

    let data = client.get_policy_data(&rule.id, &smart_account);
    // Same result as the 8-decimal case: 500 tokens at $1.00 == $500,
    // regardless of the oracle's own precision.
    assert_eq!(data.cached_total_spent_usd, 500);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")] // TokenNotAllowed
fn test_enforce_rejects_token_not_allowed() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let allowed_token = Address::generate(&e);
    let other_token = Address::generate(&e);

    let rule =
        install_policy(&e, &client, &smart_account, &oracle, vec![&e, allowed_token], 1_000, 100);

    let context = transfer_context(&e, &other_token, 500);
    client.enforce(&context, &Vec::new(&e), &rule, &smart_account);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")] // InvalidTransferArgs
fn test_enforce_rejects_non_transfer_call() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    let rule =
        install_policy(&e, &client, &smart_account, &oracle, vec![&e, token.clone()], 1_000, 100);

    let from = Address::generate(&e);
    let to = Address::generate(&e);
    let args: Vec<Val> = vec![&e, from.into_val(&e), to.into_val(&e), 500i128.into_val(&e)];
    let context = Context::Contract(ContractContext {
        contract: token,
        fn_name: Symbol::new(&e, "mint"),
        args,
    });

    client.enforce(&context, &Vec::new(&e), &rule, &smart_account);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")] // InvalidTransferArgs
fn test_enforce_rejects_missing_amount_arg() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    let rule =
        install_policy(&e, &client, &smart_account, &oracle, vec![&e, token.clone()], 1_000, 100);

    let from = Address::generate(&e);
    let to = Address::generate(&e);
    // Only two args (missing the amount), so `args.get(2)` returns `None`.
    let args: Vec<Val> = vec![&e, from.into_val(&e), to.into_val(&e)];
    let context = Context::Contract(ContractContext {
        contract: token,
        fn_name: Symbol::new(&e, "transfer"),
        args,
    });

    client.enforce(&context, &Vec::new(&e), &rule, &smart_account);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")] // InvalidTransferArgs
fn test_enforce_rejects_negative_amount() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    let rule =
        install_policy(&e, &client, &smart_account, &oracle, vec![&e, token.clone()], 1_000, 100);

    let context = transfer_context(&e, &token, -500);
    client.enforce(&context, &Vec::new(&e), &rule, &smart_account);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")] // InvalidOracleResponse
fn test_enforce_rejects_when_oracle_has_no_price() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    let rule =
        install_policy(&e, &client, &smart_account, &oracle, vec![&e, token.clone()], 1_000, 100);

    // The oracle stops reporting a price for this asset entirely —
    // `lastprice` now returns `None`, as a real oracle would for an asset
    // it doesn't track (rather than reverting).
    let oracle_client = MockOracleClient::new(&e, &oracle);
    oracle_client.set_price(&None);

    let context = transfer_context(&e, &token, 500);
    client.enforce(&context, &Vec::new(&e), &rule, &smart_account);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")] // StaleOraclePrice
fn test_enforce_rejects_stale_price() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    let rule =
        install_policy(&e, &client, &smart_account, &oracle, vec![&e, token.clone()], 1_000, 100);

    // Push the ledger clock past the trusted staleness window without
    // refreshing the oracle's price.
    let max_staleness_secs = MAX_STALENESS_LEDGERS as u64 * LEDGER_CLOSE_TIME_SECS;
    e.ledger().set_timestamp(e.ledger().timestamp() + max_staleness_secs + 1);

    let context = transfer_context(&e, &token, 500);
    client.enforce(&context, &Vec::new(&e), &rule, &smart_account);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")] // SpendingLimitExceeded
fn test_enforce_rejects_over_limit() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    let rule =
        install_policy(&e, &client, &smart_account, &oracle, vec![&e, token.clone()], 1_000, 100);

    let context = transfer_context(&e, &token, 1_001);
    client.enforce(&context, &Vec::new(&e), &rule, &smart_account);
}

#[test]
fn test_enforce_tracks_spend_across_multiple_tokens() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token_a = Address::generate(&e);
    let token_b = Address::generate(&e);

    let rule = install_policy(
        &e,
        &client,
        &smart_account,
        &oracle,
        vec![&e, token_a.clone(), token_b.clone()],
        1_000,
        100,
    );

    // Spend 600 through token A and 300 through token B — both denominated
    // in USD via the same oracle, so they share one 1,000 USD cap.
    client.enforce(&transfer_context(&e, &token_a, 600), &Vec::new(&e), &rule, &smart_account);
    client.enforce(&transfer_context(&e, &token_b, 300), &Vec::new(&e), &rule, &smart_account);

    let data = client.get_policy_data(&rule.id, &smart_account);
    assert_eq!(data.cached_total_spent_usd, 900);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")] // SpendingLimitExceeded
fn test_enforce_rejects_combined_spend_over_limit_across_tokens() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token_a = Address::generate(&e);
    let token_b = Address::generate(&e);

    let rule = install_policy(
        &e,
        &client,
        &smart_account,
        &oracle,
        vec![&e, token_a.clone(), token_b.clone()],
        1_000,
        100,
    );

    client.enforce(&transfer_context(&e, &token_a, 600), &Vec::new(&e), &rule, &smart_account);
    // 600 (token A) + 500 (token B) = 1,100 > 1,000 limit.
    client.enforce(&transfer_context(&e, &token_b, 500), &Vec::new(&e), &rule, &smart_account);
}

#[test]
fn test_enforce_allows_spend_again_after_window_expires() {
    let (e, smart_account, client) = setup_env();
    let oracle = setup_oracle(&e, ONE_USD);
    let token = Address::generate(&e);

    let period_ledgers = 100;
    let rule = install_policy(
        &e,
        &client,
        &smart_account,
        &oracle,
        vec![&e, token.clone()],
        1_000,
        period_ledgers,
    );

    client.enforce(&transfer_context(&e, &token, 900), &Vec::new(&e), &rule, &smart_account);

    // Would exceed the limit right now (900 + 900 > 1,000)...
    let res = client.try_enforce(
        &transfer_context(&e, &token, 900),
        &Vec::new(&e),
        &rule,
        &smart_account,
    );
    assert!(res.is_err());

    // ...but once the rolling window has fully passed, the old entry is
    // evicted and the same spend succeeds.
    e.ledger().set_sequence_number(e.ledger().sequence() + period_ledgers + 1);
    client.enforce(&transfer_context(&e, &token, 900), &Vec::new(&e), &rule, &smart_account);

    let data = client.get_policy_data(&rule.id, &smart_account);
    assert_eq!(data.cached_total_spent_usd, 900);
    assert_eq!(data.spending_history.len(), 1);
}

// ################## QUERY ##################

#[test]
#[should_panic(expected = "Error(Contract, #4)")] // NotInstalled
fn test_get_policy_data_rejects_not_installed() {
    let (_e, smart_account, client) = setup_env();
    client.get_policy_data(&1, &smart_account);
}
