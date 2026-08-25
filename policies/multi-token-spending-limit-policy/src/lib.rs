//! Oracle-denominated multi-token spending limit policy for Latch smart accounts.
#![no_std]

use soroban_sdk::{
    auth::Context, contract, contracterror, contractimpl, contracttype, Address, Env,
    Symbol, Val, Vec,
};
use stellar_accounts::{
    policies::Policy,
    smart_account::{ContextRule, Signer},
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Error {
    OracleUnreachable = 1,
    StaleOraclePrice = 2,
    SpendingLimitExceeded = 3,
}

#[contracttype]
#[derive(Clone)]
pub struct PriceData {
    pub price_usd: i128,
    pub updated_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct MultiTokenSpendingLimitAccountParams {
    pub spending_limit_usd: i128,
    pub oracle_address: Address,
    pub allowed_tokens: Vec<Address>,
}

#[contracttype]
#[derive(Clone)]
pub struct SpendingEntry {
    pub amount_usd: i128,
    pub ledger: u32,
}

#[contracttype]
pub enum DataKey {
    Limit(Address, u32),
    Oracle(Address, u32),
    AllowedTokens(Address, u32),
    SpendingEntries(Address, u32),
}

const MAX_STALENESS_LEDGERS: u32 = 100;
const ORACLE_FN: Symbol = Symbol::short("get_price");

#[contract]
pub struct MultiTokenSpendingLimitPolicy;

#[contractimpl]
impl Policy for MultiTokenSpendingLimitPolicy {
    type AccountParams = MultiTokenSpendingLimitAccountParams;

    fn install(
        e: &Env,
        install_params: Self::AccountParams,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        let id = context_rule.id;
        e.storage().persistent().set(
            &DataKey::Limit(smart_account.clone(), id),
            &install_params.spending_limit_usd,
        );
        e.storage().persistent().set(
            &DataKey::Oracle(smart_account.clone(), id),
            &install_params.oracle_address,
        );
        e.storage().persistent().set(
            &DataKey::AllowedTokens(smart_account.clone(), id),
            &install_params.allowed_tokens,
        );
    }

    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        let id = context_rule.id;
        e.storage().persistent().remove(&DataKey::Limit(smart_account.clone(), id));
        e.storage().persistent().remove(&DataKey::Oracle(smart_account.clone(), id));
        e.storage().persistent().remove(&DataKey::AllowedTokens(smart_account.clone(), id));
        e.storage().persistent().remove(&DataKey::SpendingEntries(smart_account, id));
    }

    fn enforce(
        e: &Env,
        context: Context,
        _authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        let id = context_rule.id;

        let Context::Contract(c) = context else {
            panic!("multi-token spending limit only applies to CallContract contexts");
        };

        if c.fn_name != Symbol::short("transfer") {
            panic!("only transfer calls are allowed");
        }

        let amount: i128 = c.args.get(2).unwrap().try_into_val(e).unwrap();
        let target = c.contract;

        let allowed_tokens: Vec<Address> = e
            .storage()
            .persistent()
            .get(&DataKey::AllowedTokens(smart_account.clone(), id))
            .expect("policy not installed");

        if !allowed_tokens.contains(&target) {
            panic!("token not in allowed list");
        }

        let oracle: Address = e
            .storage()
            .persistent()
            .get(&DataKey::Oracle(smart_account.clone(), id))
            .expect("oracle not configured");

        let price_data: PriceData = e
            .invoke_contract(&oracle, &ORACLE_FN, soroban_sdk::vec![e, target])
            .try_into()
            .unwrap();

        let current = e.ledger().sequence();
        if current.saturating_sub(price_data.updated_at as u32) > MAX_STALENESS_LEDGERS {
            panic!("stale oracle price");
        }

        let amount_usd = amount
            .saturating_mul(price_data.price_usd)
            .saturating_div(100_000_000);

        let limit: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::Limit(smart_account.clone(), id))
            .expect("limit not set");

        let mut entries: Vec<SpendingEntry> = e
            .storage()
            .persistent()
            .get(&DataKey::SpendingEntries(smart_account.clone(), id))
            .unwrap_or(Vec::new(e));

        let mut total: i128 = 0;
        let mut kept = Vec::new(e);
        for entry in entries.iter() {
            if current.saturating_sub(entry.ledger) <= context_rule.period_ledgers {
                total = total.saturating_add(entry.amount_usd);
                kept.push_back(entry);
            }
        }

        if total.saturating_add(amount_usd) > limit {
            panic!("spending limit exceeded");
        }

        kept.push_back(SpendingEntry { amount_usd, ledger: current });
        e.storage().persistent().set(
            &DataKey::SpendingEntries(smart_account, id),
            &kept,
        );
    }
}
