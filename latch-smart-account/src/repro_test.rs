#![cfg(test)]
extern crate std;

use multi_token_spending_limit_policy::{
    MultiTokenSpendingLimitAccountParams, MultiTokenSpendingLimitPolicy,
};
use soroban_sdk::{
    auth::{Context, ContractContext},
    contract, contractimpl, contracttype, symbol_short,
    testutils::{Address as _, Ledger},
    vec, Address, Bytes, Env, IntoVal, Map, String, Val, Vec,
};
use spending_limit_policy::SpendingLimitPolicy;
use stellar_accounts::{
    policies::spending_limit::SpendingLimitAccountParams,
    smart_account::{self as smart_account, AuthPayload, ContextRuleType, Signer},
};

use super::{LatchSmartAccount, LatchSmartAccountClient};

#[contracttype]
#[derive(Clone)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

#[contract]
struct MockOracle;
#[contractimpl]
impl MockOracle {
    pub fn decimals(_e: Env) -> u32 {
        8
    }
    pub fn lastprice(e: Env, _asset: Val) -> Option<PriceData> {
        Some(PriceData { price: 100_000_000, timestamp: e.ledger().timestamp() })
    }
}

#[contract]
struct MockToken;
#[contractimpl]
impl MockToken {
    pub fn decimals(_e: Env) -> u32 {
        0
    }
}

fn transfer_ctx(e: &Env, token: &Address, amount: i128) -> Context {
    Context::Contract(ContractContext {
        contract: token.clone(),
        fn_name: symbol_short!("transfer"),
        args: vec![
            e,
            Address::generate(e).into_val(e),
            Address::generate(e).into_val(e),
            amount.into_val(e),
        ],
    })
}

/// Account with an owner Default rule plus a CallContract(token) rule bound to a
/// session signer. The ATTACKER holds neither key and submits an EMPTY signer map.
fn setup(e: &Env) -> (Address, LatchSmartAccountClient<'_>, Address, u32) {
    e.mock_all_auths();
    e.ledger().set_sequence_number(1_000);
    e.ledger().set_timestamp(1_000_000);
    let owner = vec![e, Signer::Delegated(Address::generate(e))];
    let account_id = e.register(LatchSmartAccount, (owner, Map::<Address, Val>::new(e)));
    let client = LatchSmartAccountClient::new(e, &account_id);
    let token = e.register(MockToken, ());
    let session = vec![e, Signer::Delegated(Address::generate(e))];
    let rule = client.add_context_rule(
        &ContextRuleType::CallContract(token.clone()),
        &String::from_str(e, "spend"),
        &None,
        &session,
        &Map::new(e),
    );
    (account_id, client, token, rule.id)
}

fn empty_sigs(e: &Env, rule_id: u32) -> AuthPayload {
    AuthPayload { signers: Map::new(e), context_rule_ids: vec![e, rule_id] }
}

#[test]
fn multi_token_rule_authorizes_with_no_signatures() {
    let e = Env::default();
    let (account_id, client, token, rule_id) = setup(&e);
    let oracle = e.register(MockOracle, ());
    let policy = e.register(MultiTokenSpendingLimitPolicy, ());
    let params: Val = MultiTokenSpendingLimitAccountParams {
        spending_limit_usd: 1_000,
        period_ledgers: 100,
        oracle_address: oracle,
        allowed_tokens: vec![&e, token.clone()],
    }
    .into_val(&e);
    client.add_policy(&rule_id, &policy, &params);

    let ctxs = Vec::from_array(&e, [transfer_ctx(&e, &token, 999)]);
    let hash = e.crypto().sha256(&Bytes::from_array(&e, &[7u8; 32]));
    e.as_contract(&account_id, || {
        let r = smart_account::do_check_auth(&e, &hash, &empty_sigs(&e, rule_id), &ctxs);
        assert!(r.is_ok(), "expected the unsigned spend to pass: {:?}", r);
    });
}

#[test]
#[should_panic]
fn oz_spending_limit_rule_rejects_no_signatures() {
    let e = Env::default();
    let (account_id, client, token, rule_id) = setup(&e);
    let policy = e.register(SpendingLimitPolicy, ());
    let params: Val =
        SpendingLimitAccountParams { spending_limit: 1_000, period_ledgers: 100 }.into_val(&e);
    client.add_policy(&rule_id, &policy, &params);

    let ctxs = Vec::from_array(&e, [transfer_ctx(&e, &token, 999)]);
    let hash = e.crypto().sha256(&Bytes::from_array(&e, &[7u8; 32]));
    e.as_contract(&account_id, || {
        let _ = smart_account::do_check_auth(&e, &hash, &empty_sigs(&e, rule_id), &ctxs);
    });
}
