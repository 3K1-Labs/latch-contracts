#![cfg(test)]

extern crate std;

use session_policy::{SessionAccountParams, SessionPolicy};
use soroban_sdk::{
    auth::{Context, ContractContext},
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    vec, Address, Bytes, BytesN, Env, Executable, IntoVal, Map, String, Symbol, Val, Vec,
};
use spending_limit_policy::SpendingLimitPolicy;
use stellar_accounts::{
    policies::{
        simple_threshold::SimpleThresholdAccountParams, spending_limit::SpendingLimitAccountParams,
        Policy,
    },
    smart_account::{self as smart_account, AuthPayload, ContextRuleType, Signer},
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

#[test]
fn upgrade_succeeds_with_self_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (account_id, client) = register_account(&env, &signers, &policies);

    // Re-upgrading to the account's own currently-running wasm — a real,
    // valid hash (not a dummy one), so this proves the upgrade mechanism
    // actually succeeds end-to-end when properly self-authorized, not just
    // that the auth check passes.
    let Some(Executable::Wasm(wasm_hash)) = account_id.executable() else {
        panic!("expected a wasm-backed account");
    };

    client.upgrade(&wasm_hash, &account_id);
}

#[test]
#[should_panic]
fn upgrade_requires_self_auth() {
    let env = Env::default();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (account_id, client) = register_account(&env, &signers, &policies);

    let new_wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.upgrade(&new_wasm_hash, &account_id);
}

// ################## SESSION KEY INTEGRATION TESTS ##################
//
// These tests exercise `smart_account::do_check_auth` directly (as OZ's own
// `context_rules.rs` test suite does) rather than routing through the
// account's `require_auth` machinery, so a `Signer::Delegated` address can
// stand in for an ephemeral session key without needing real signature
// verification — verifier cryptography itself is already covered by the
// `latch-verifiers` crates' own test suites.

fn get_context(contract: Address, fn_name: Symbol, args: Vec<Val>) -> Context {
    Context::Contract(ContractContext { contract, fn_name, args })
}

fn create_signatures(e: &Env, signers: &Vec<Signer>, context_rule_ids: Vec<u32>) -> AuthPayload {
    let mut signature_map = Map::new(e);
    for signer in signers.iter() {
        signature_map.set(signer, Bytes::new(e));
    }
    AuthPayload { signers: signature_map, context_rule_ids }
}

/// Sets up a smart account with a `CallContract(target)` session rule bound
/// to one `Signer::Delegated` session key, installs the session policy with
/// the given `allowed_fns`, and returns everything needed to drive
/// `do_check_auth` against it.
fn setup_session_rule<'a>(
    env: &'a Env,
    allowed_fns: Vec<Symbol>,
) -> (Address, LatchSmartAccountClient<'a>, Address, u32, Vec<Signer>) {
    env.mock_all_auths();

    let owner_signers = default_signers(env);
    let policies = Map::new(env);
    let (account_id, client) = register_account(env, &owner_signers, &policies);

    let target_id = env.register(MockTargetContract, ());

    let session_signer = Signer::Delegated(Address::generate(env));
    let session_signers = vec![env, session_signer];

    let rule = client.add_context_rule(
        &ContextRuleType::CallContract(target_id.clone()),
        &String::from_str(env, "session"),
        &None,
        &session_signers,
        &Map::new(env),
    );

    let session_policy_id = env.register(SessionPolicy, ());
    let install_param: Val = SessionAccountParams { allowed_fns }.into_val(env);
    client.add_policy(&rule.id, &session_policy_id, &install_param);

    (account_id, client, target_id, rule.id, session_signers)
}

#[test]
fn session_signer_allowed_call_succeeds() {
    let env = Env::default();
    let (account_id, _client, target_id, rule_id, session_signers) =
        setup_session_rule(&env, vec![&env, Symbol::new(&env, "set")]);

    let context = get_context(target_id, Symbol::new(&env, "set"), vec![&env, 7u32.into_val(&env)]);
    let auth_contexts = Vec::from_array(&env, [context]);
    let signatures = create_signatures(&env, &session_signers, vec![&env, rule_id]);
    let payload_hash = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));

    env.as_contract(&account_id, || {
        let result = smart_account::do_check_auth(&env, &payload_hash, &signatures, &auth_contexts);
        assert!(result.is_ok());
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn session_signer_disallowed_method_rejected() {
    let env = Env::default();
    let (account_id, _client, target_id, rule_id, session_signers) =
        setup_session_rule(&env, vec![&env, Symbol::new(&env, "set")]);

    // "get" is not in the allowlist, only "set" is.
    let context = get_context(target_id, Symbol::new(&env, "get"), vec![&env]);
    let auth_contexts = Vec::from_array(&env, [context]);
    let signatures = create_signatures(&env, &session_signers, vec![&env, rule_id]);
    let payload_hash = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));

    env.as_contract(&account_id, || {
        let _ = smart_account::do_check_auth(&env, &payload_hash, &signatures, &auth_contexts);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3002)")]
fn session_call_after_valid_until_expiry_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let owner_signers = default_signers(&env);
    let policies = Map::new(&env);
    let (account_id, client) = register_account(&env, &owner_signers, &policies);

    let target_id = env.register(MockTargetContract, ());
    let session_signer = Signer::Delegated(Address::generate(&env));
    let session_signers = vec![&env, session_signer];

    let expiry_ledger = env.ledger().sequence() + 1;
    let rule = client.add_context_rule(
        &ContextRuleType::CallContract(target_id.clone()),
        &String::from_str(&env, "session"),
        &Some(expiry_ledger),
        &session_signers,
        &Map::new(&env),
    );

    let session_policy_id = env.register(SessionPolicy, ());
    let install_param: Val =
        SessionAccountParams { allowed_fns: vec![&env, Symbol::new(&env, "set")] }.into_val(&env);
    client.add_policy(&rule.id, &session_policy_id, &install_param);

    // Advance past the rule's expiry.
    env.ledger().with_mut(|li| {
        li.sequence_number = expiry_ledger + 1;
    });

    let context = get_context(target_id, Symbol::new(&env, "set"), vec![&env, 7u32.into_val(&env)]);
    let auth_contexts = Vec::from_array(&env, [context]);
    let signatures = create_signatures(&env, &session_signers, vec![&env, rule.id]);
    let payload_hash = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));

    env.as_contract(&account_id, || {
        let _ = smart_account::do_check_auth(&env, &payload_hash, &signatures, &auth_contexts);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3000)")]
fn session_call_after_remove_context_rule_rejected() {
    let env = Env::default();
    let (account_id, client, target_id, rule_id, session_signers) =
        setup_session_rule(&env, vec![&env, Symbol::new(&env, "set")]);

    client.remove_context_rule(&rule_id);

    let context = get_context(target_id, Symbol::new(&env, "set"), vec![&env, 7u32.into_val(&env)]);
    let auth_contexts = Vec::from_array(&env, [context]);
    let signatures = create_signatures(&env, &session_signers, vec![&env, rule_id]);
    let payload_hash = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));

    env.as_contract(&account_id, || {
        let _ = smart_account::do_check_auth(&env, &payload_hash, &signatures, &auth_contexts);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3221)")]
fn session_plus_spending_limit_over_limit_rejected_even_when_method_allowed() {
    let env = Env::default();
    env.mock_all_auths();

    let owner_signers = default_signers(&env);
    let policies = Map::new(&env);
    let (account_id, client) = register_account(&env, &owner_signers, &policies);

    let target_id = env.register(MockTargetContract, ());
    let session_signer = Signer::Delegated(Address::generate(&env));
    let session_signers = vec![&env, session_signer];

    let rule = client.add_context_rule(
        &ContextRuleType::CallContract(target_id.clone()),
        &String::from_str(&env, "session"),
        &None,
        &session_signers,
        &Map::new(&env),
    );

    let session_policy_id = env.register(SessionPolicy, ());
    let session_install_param: Val =
        SessionAccountParams { allowed_fns: vec![&env, symbol_short!("transfer")] }.into_val(&env);
    client.add_policy(&rule.id, &session_policy_id, &session_install_param);

    let spending_limit_policy_id = env.register(SpendingLimitPolicy, ());
    let spending_install_param: Val =
        SpendingLimitAccountParams { spending_limit: 1_000_000, period_ledgers: 100 }
            .into_val(&env);
    client.add_policy(&rule.id, &spending_limit_policy_id, &spending_install_param);

    // "transfer" is allowlisted, but the amount exceeds the spending limit.
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let context = get_context(
        target_id,
        symbol_short!("transfer"),
        vec![&env, from.into_val(&env), to.into_val(&env), 2_000_000i128.into_val(&env)],
    );
    let auth_contexts = Vec::from_array(&env, [context]);
    let signatures = create_signatures(&env, &session_signers, vec![&env, rule.id]);
    let payload_hash = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));

    env.as_contract(&account_id, || {
        let _ = smart_account::do_check_auth(&env, &payload_hash, &signatures, &auth_contexts);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn session_plus_spending_limit_disallowed_method_rejected_even_when_under_limit() {
    let env = Env::default();
    env.mock_all_auths();

    let owner_signers = default_signers(&env);
    let policies = Map::new(&env);
    let (account_id, client) = register_account(&env, &owner_signers, &policies);

    let target_id = env.register(MockTargetContract, ());
    let session_signer = Signer::Delegated(Address::generate(&env));
    let session_signers = vec![&env, session_signer];

    let rule = client.add_context_rule(
        &ContextRuleType::CallContract(target_id.clone()),
        &String::from_str(&env, "session"),
        &None,
        &session_signers,
        &Map::new(&env),
    );

    let session_policy_id = env.register(SessionPolicy, ());
    let session_install_param: Val =
        SessionAccountParams { allowed_fns: vec![&env, symbol_short!("transfer")] }.into_val(&env);
    client.add_policy(&rule.id, &session_policy_id, &session_install_param);

    let spending_limit_policy_id = env.register(SpendingLimitPolicy, ());
    let spending_install_param: Val =
        SpendingLimitAccountParams { spending_limit: 1_000_000, period_ledgers: 100 }
            .into_val(&env);
    client.add_policy(&rule.id, &spending_limit_policy_id, &spending_install_param);

    // Well under the spending limit, but "withdraw" is not the allowlisted
    // "transfer" method.
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let context = get_context(
        target_id,
        symbol_short!("withdraw"),
        vec![&env, from.into_val(&env), to.into_val(&env), 1i128.into_val(&env)],
    );
    let auth_contexts = Vec::from_array(&env, [context]);
    let signatures = create_signatures(&env, &session_signers, vec![&env, rule.id]);
    let payload_hash = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));

    env.as_contract(&account_id, || {
        let _ = smart_account::do_check_auth(&env, &payload_hash, &signatures, &auth_contexts);
    });
}

// ################## REMOVE_SIGNER TESTS ##################
//
// These tests exercise the `remove_signer` override on
// `LatchSmartAccount`, which probes attached policies via
// `try_invoke_contract` before delegating to OZ's `storage::remove_signer`.
// The threshold-policy and weighted-threshold-policy both implement
// `would_remain_reachable`; non-threshold policies are skipped.

use weighted_threshold_policy::WeightedThresholdPolicy;
use stellar_accounts::policies::weighted_threshold::WeightedThresholdAccountParams;

/// Sets up a smart account with N signers and a threshold-policy installed
/// at the given threshold, returning the account and client.
fn setup_with_threshold(
    env: &Env,
    signer_count: u32,
    threshold: u32,
) -> (Address, LatchSmartAccountClient<'_>) {
    env.mock_all_auths();

    let mut signers = Vec::new(env);
    for _ in 0..signer_count {
        signers.push_back(Signer::Delegated(Address::generate(env)));
    }
    let policies = Map::new(env);
    let (account_id, client) = register_account(env, &signers, &policies);

    let threshold_policy_id = env.register(threshold_policy::ThresholdPolicy, ());
    let install_param: Val = SimpleThresholdAccountParams { threshold }.into_val(env);
    client.add_policy(&0, &threshold_policy_id, &install_param);

    (account_id, client)
}

#[test]
fn remove_signer_succeeds_when_reachable() {
    let env = Env::default();
    // 3 signers, threshold=2. Remove one → 2 remaining ≥ 2 → ok.
    let (_account_id, client) = setup_with_threshold(&env, 3, 2);

    env.mock_all_auths();
    client.remove_signer(&0, &0);

    let rule = client.get_context_rule(&0);
    assert_eq!(rule.signers.len(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn remove_signer_panics_when_unreachable() {
    let env = Env::default();
    // 3 signers, threshold=3. Remove one → 2 remaining < 3 → blocked.
    let (_account_id, client) = setup_with_threshold(&env, 3, 3);

    env.mock_all_auths();
    client.remove_signer(&0, &0);
}

#[test]
fn remove_signer_succeeds_with_non_threshold_policy() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    // Attach MockPolicyContract — does not implement would_remain_reachable,
    // so the probe is skipped ("no opinion").
    let mock_policy_id = env.register(MockPolicyContract, ());
    let install_param: Val = Val::from_void().into();
    client.add_policy(&0, &mock_policy_id, &install_param);

    // The default rule has 1 signer. remove_signer should work
    // because the mock policy doesn't object.
    client.remove_signer(&0, &0);

    let rule = client.get_context_rule(&0);
    assert_eq!(rule.signers.len(), 0);
    // Last signer removed but a policy remains — allowed by OZ.
}

#[test]
#[should_panic]
fn remove_signer_requires_self_auth() {
    let env = Env::default();
    // No mock_all_auths — auth should fail.
    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    client.remove_signer(&0, &0);
}

#[test]
fn remove_signer_blocked_by_weighted_threshold_policy() {
    let env = Env::default();
    env.mock_all_auths();

    let signer_a = Signer::Delegated(Address::generate(&env));
    let signer_b = Signer::Delegated(Address::generate(&env));
    let signer_c = Signer::Delegated(Address::generate(&env));
    let signers = vec![&env, signer_a.clone(), signer_b.clone(), signer_c.clone()];
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    // Deploy weighted-threshold-policy and install with:
    // A=100, B=75, C=75, threshold=150.
    // Removing A (weight 100) → remaining = 75+75 = 150 ≥ 150 → reachable.
    let wtp_id = env.register(WeightedThresholdPolicy, ());
    let mut signer_weights = Map::new(&env);
    signer_weights.set(signer_a.clone(), 100u32);
    signer_weights.set(signer_b.clone(), 75u32);
    signer_weights.set(signer_c.clone(), 75u32);
    let install_param: Val = WeightedThresholdAccountParams { signer_weights, threshold: 150 }
        .into_val(&env);
    client.add_policy(&0, &wtp_id, &install_param);

    // Remove A (weight 100): remaining = 75+75 = 150 ≥ 150 → ok.
    // Find signer A's ID in the context rule.
    let rule = client.get_context_rule(&0);
    let signer_a_id = rule.signer_ids.get(0).unwrap();
    client.remove_signer(&0, &signer_a_id);

    let rule = client.get_context_rule(&0);
    assert_eq!(rule.signers.len(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn remove_signer_unreachable_blocked_by_weighted_threshold_policy() {
    let env = Env::default();
    env.mock_all_auths();

    let signer_a = Signer::Delegated(Address::generate(&env));
    let signer_b = Signer::Delegated(Address::generate(&env));
    let signer_c = Signer::Delegated(Address::generate(&env));
    let signers = vec![&env, signer_a.clone(), signer_b.clone(), signer_c.clone()];
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    // A=100, B=75, C=75, threshold=150.
    // Removing B (weight 75) → remaining = 100+75 = 175 ≥ 150 → reachable.
    // But removing B AND C → 100 < 150 → unreachable.
    // We test single removal: B removed → reachable, so let's use threshold=200
    // to make any single removal unreachable.
    let wtp_id = env.register(WeightedThresholdPolicy, ());
    let mut signer_weights = Map::new(&env);
    signer_weights.set(signer_a.clone(), 100u32);
    signer_weights.set(signer_b.clone(), 75u32);
    signer_weights.set(signer_c.clone(), 75u32);
    let install_param: Val = WeightedThresholdAccountParams { signer_weights, threshold: 200 }
        .into_val(&env);
    client.add_policy(&0, &wtp_id, &install_param);

    // Total weight = 250. Removing any single signer drops below 200.
    // Remove B (weight 75): remaining = 100+75 = 175 < 200 → blocked.
    let rule = client.get_context_rule(&0);
    let signer_b_id = rule.signer_ids.get(1).unwrap();
    client.remove_signer(&0, &signer_b_id);
}

// ################## BATCH_ADD_SIGNER TESTS ##################
//
// These tests exercise the `batch_add_signer` override with the
// `acknowledge_threshold_unchanged` parameter, which probes attached
// policies after adding signers and reverts if any policy objects and
// acknowledgment was not given.

#[test]
fn batch_add_signer_succeeds_with_acknowledgment() {
    let env = Env::default();
    // 3 signers, threshold=3. Adding a 4th → 4-of-3 ratio (weaker).
    let (_account_id, client) = setup_with_threshold(&env, 3, 3);

    let new_signer = Signer::Delegated(Address::generate(&env));
    env.mock_all_auths();
    // ack_threshold = true → proceeds despite weakening.
    client.batch_add_signer(&0, &vec![&env, new_signer.clone()], &true);

    let rule = client.get_context_rule(&0);
    assert_eq!(rule.signers.len(), 4);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn batch_add_signer_reverts_without_acknowledgment_when_policy_objects() {
    let env = Env::default();
    // 3 signers, threshold=3. Adding a 4th would weaken to 4-of-3.
    let (_account_id, client) = setup_with_threshold(&env, 3, 3);

    let new_signer = Signer::Delegated(Address::generate(&env));
    env.mock_all_auths();
    // ack_threshold = false → reverts.
    client.batch_add_signer(&0, &vec![&env, new_signer], &false);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn batch_add_signer_reverts_when_ack_false_regardless_of_policies() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    // Attach MockPolicyContract — does not implement would_remain_reachable.
    let mock_policy_id = env.register(MockPolicyContract, ());
    let install_param: Val = Val::from_void().into();
    client.add_policy(&0, &mock_policy_id, &install_param);

    let new_signer = Signer::Delegated(Address::generate(&env));
    // ack_threshold = false always reverts, even without a threshold policy.
    client.batch_add_signer(&0, &vec![&env, new_signer], &false);
}

#[test]
fn batch_add_signer_succeeds_without_threshold_policy_when_ack_true() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    // Attach MockPolicyContract — does not implement would_remain_reachable,
    // so the probe is skipped ("no opinion").
    let mock_policy_id = env.register(MockPolicyContract, ());
    let install_param: Val = Val::from_void().into();
    client.add_policy(&0, &mock_policy_id, &install_param);

    let new_signer = Signer::Delegated(Address::generate(&env));
    // ack_threshold = false always reverts, regardless of policies.
    // Use ack_threshold = true to succeed.
    client.batch_add_signer(&0, &vec![&env, new_signer], &true);

    let rule = client.get_context_rule(&0);
    assert_eq!(rule.signers.len(), 2);
}

#[test]
fn batch_add_signer_succeeds_when_reachable_with_acknowledgment() {
    let env = Env::default();
    // 3 signers, threshold=2. Adding a 4th → 4-of-2 ratio (weaker but still reachable).
    let (_account_id, client) = setup_with_threshold(&env, 3, 2);

    let new_signer = Signer::Delegated(Address::generate(&env));
    env.mock_all_auths();
    // Even though threshold-policy returns true (2 ≤ 4), we still need
    // acknowledgment because the ratio changed.
    client.batch_add_signer(&0, &vec![&env, new_signer], &true);

    let rule = client.get_context_rule(&0);
    assert_eq!(rule.signers.len(), 4);
}
