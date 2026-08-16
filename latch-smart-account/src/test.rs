#![cfg(test)]

extern crate std;

use soroban_sdk::{
    contract, contractimpl, testutils::Address as _, vec, Address, Env, IntoVal, Map, String,
    Symbol, Val, Vec,
};
use stellar_accounts::{
    policies::Policy,
    smart_account::{ContextRuleType, Signer},
};

use super::{LatchSmartAccount, LatchSmartAccountClient};

#[contract]
struct MockTargetContract;

#[contractimpl]
impl MockTargetContract {
    pub fn set(e: Env, value: u32) {
        e.storage().persistent().set(&Symbol::new(&e, "value"), &value);
    }

    pub fn get(e: Env) -> u32 {
        e.storage().persistent().get(&Symbol::new(&e, "value")).unwrap_or(0)
    }
}

#[contract]
struct MockPolicyContract;

#[contractimpl]
impl Policy for MockPolicyContract {
    type AccountParams = Val;

    fn enforce(
        _e: &Env,
        _context: soroban_sdk::auth::Context,
        _authenticated_signers: Vec<Signer>,
        _context_rule: stellar_accounts::smart_account::ContextRule,
        _smart_account: Address,
    ) {
    }

    fn install(
        _e: &Env,
        _install_params: Val,
        _context_rule: stellar_accounts::smart_account::ContextRule,
        _smart_account: Address,
    ) {
    }

    fn uninstall(
        _e: &Env,
        _context_rule: stellar_accounts::smart_account::ContextRule,
        _smart_account: Address,
    ) {
    }
}

fn default_signers(env: &Env) -> Vec<Signer> {
    vec![env, Signer::Delegated(Address::generate(env))]
}

fn register_account<'a>(
    env: &'a Env,
    signers: &Vec<Signer>,
    policies: &Map<Address, Val>,
) -> (Address, LatchSmartAccountClient<'a>) {
    let account_id = env.register(LatchSmartAccount, (signers.clone(), policies.clone()));
    let client = LatchSmartAccountClient::new(env, &account_id);
    (account_id, client)
}

#[test]
fn constructor_creates_one_default_rule_named_default() {
    let env = Env::default();
    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    assert_eq!(client.get_context_rules_count(), 1);
    let rule = client.get_context_rule(&0);
    assert_eq!(rule.name, String::from_str(&env, "default"));
    assert_eq!(rule.context_type, ContextRuleType::Default);
    assert_eq!(rule.signers, signers);
    assert_eq!(rule.valid_until, None);
}

#[test]
fn execute_forwards_calls() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    let target_id = env.register(MockTargetContract, ());
    let target_client = MockTargetContractClient::new(&env, &target_id);

    client.execute(&target_id, &Symbol::new(&env, "set"), &vec![&env, 7u32.into_val(&env)]);

    assert_eq!(target_client.get(), 7);
}

#[test]
#[should_panic]
fn execute_requires_self_auth() {
    let env = Env::default();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    let target_id = env.register(MockTargetContract, ());
    client.execute(&target_id, &Symbol::new(&env, "set"), &vec![&env, 1u32.into_val(&env)]);
}

#[test]
fn add_context_rule_succeeds_with_self_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let some_contract = Address::generate(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    let added = client.add_context_rule(
        &ContextRuleType::CallContract(some_contract),
        &String::from_str(&env, "secondary"),
        &None,
        &signers,
        &policies,
    );

    assert_eq!(added.name, String::from_str(&env, "secondary"));
    assert_eq!(client.get_context_rules_count(), 2);
}

#[test]
#[should_panic]
fn add_context_rule_requires_self_auth() {
    let env = Env::default();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let some_contract = Address::generate(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    client.add_context_rule(
        &ContextRuleType::CallContract(some_contract),
        &String::from_str(&env, "secondary"),
        &None,
        &signers,
        &policies,
    );
}

#[test]
fn add_signer_and_policy_succeed_with_self_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    let default_rule = client.get_context_rule(&0);
    let new_signer = Signer::Delegated(Address::generate(&env));
    let signer_id = client.add_signer(&default_rule.id, &new_signer);

    let policy_id = env.register(MockPolicyContract, ());
    let install_param: Val = Val::from_void().into();
    let added_policy_id = client.add_policy(&default_rule.id, &policy_id, &install_param);

    let updated_rule = client.get_context_rule(&default_rule.id);
    assert!(updated_rule.signers.contains(&new_signer));
    assert_eq!(signer_id, 1);
    assert_eq!(added_policy_id, 0);
    assert!(updated_rule.policies.contains(&policy_id));
}

#[test]
#[should_panic]
fn add_signer_requires_self_auth() {
    let env = Env::default();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    let default_rule = client.get_context_rule(&0);
    client.add_signer(&default_rule.id, &Signer::Delegated(Address::generate(&env)));
}

#[test]
#[should_panic]
fn add_policy_requires_self_auth() {
    let env = Env::default();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    let default_rule = client.get_context_rule(&0);
    let policy_id = env.register(MockPolicyContract, ());
    let install_param: Val = Val::from_void().into();
    client.add_policy(&default_rule.id, &policy_id, &install_param);
}
