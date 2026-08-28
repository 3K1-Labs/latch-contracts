#![cfg(test)]

extern crate std;

use session_policy::{SessionAccountParams, SessionPolicy};
use soroban_sdk::{
    auth::{
        Context, ContractContext, ContractExecutable, CreateContractWithConstructorHostFnContext,
    },
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    vec, Address, Bytes, BytesN, Env, Executable, IntoVal, Map, String, Symbol, Val, Vec,
};
use spending_limit_policy::SpendingLimitPolicy;
use stellar_accounts::{
    policies::{spending_limit::SpendingLimitAccountParams, Policy},
    smart_account::{self as smart_account, AuthPayload, ContextRuleType, Signer},
};

use super::{LatchSmartAccount, LatchSmartAccountClient};

// A real, tiny, already-existing no-op contract (zero-arg constructor),
// reused from `factory-contract`'s own testdata rather than building a new
// fixture — deploying it via `env.deployer().upload_contract_wasm(...)`
// exercises the real Soroban host deployer, not a mocked one.
mod dummy_singleton {
    soroban_sdk::contractimport!(file = "testdata/dummy_singleton.wasm");
}

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

// ################## DEPLOY_CONTRACT: DEPLOYMENT MECHANICS ##################
//
// `mock_all_auths()` bypasses `__check_auth` entirely (it never runs), so
// these tests don't exercise context-rule matching — see the
// CREATE_CONTRACT AUTHORIZATION section below for that. What `mock_all_auths`
// does NOT bypass is the actual Soroban host deployer: `deploy_v2` here runs
// against real uploaded wasm, so these tests prove the deployment mechanics
// themselves (address derivation, collision handling, discoverability
// storage) against a real deployed contract, not a mocked one.

fn deployed_wasm_hash(env: &Env) -> BytesN<32> {
    env.deployer().upload_contract_wasm(dummy_singleton::WASM)
}

#[test]
fn deploy_contract_deploys_real_wasm_and_derives_deterministic_address() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (account_id, client) = register_account(&env, &signers, &policies);

    let wasm_hash = deployed_wasm_hash(&env);
    let salt = BytesN::from_array(&env, &[7u8; 32]);

    // Predicted independently of `deploy_contract`, from the deployer +
    // salt alone — this is what "deterministic address" means.
    let expected_address =
        env.deployer().with_address(account_id.clone(), salt.clone()).deployed_address();

    let deployed = client.deploy_contract(&wasm_hash, &salt, &vec![&env]);

    assert_eq!(deployed, expected_address);
    assert_eq!(client.get_deployed_contract_count(), 1);
    let record = client.get_deployed_contract(&0);
    assert_eq!(record.address, deployed);
    assert_eq!(record.wasm_hash, wasm_hash);
}

#[test]
fn deploy_contract_records_multiple_deployments_in_order() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    let wasm_hash = deployed_wasm_hash(&env);

    let first =
        client.deploy_contract(&wasm_hash, &BytesN::from_array(&env, &[1u8; 32]), &vec![&env]);
    let second =
        client.deploy_contract(&wasm_hash, &BytesN::from_array(&env, &[2u8; 32]), &vec![&env]);

    assert_eq!(client.get_deployed_contract_count(), 2);
    assert_eq!(client.get_deployed_contract(&0).address, first);
    assert_eq!(client.get_deployed_contract(&1).address, second);
}

#[test]
#[should_panic]
fn deploy_contract_rejects_reusing_same_salt() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    let wasm_hash = deployed_wasm_hash(&env);
    let salt = BytesN::from_array(&env, &[3u8; 32]);

    client.deploy_contract(&wasm_hash, &salt, &vec![&env]);
    // Same `(wasm_hash, salt)` from the same deployer derives the same
    // address as the first deployment — the host itself rejects deploying
    // over an address that already has a contract, so a caller can't
    // accidentally collide with or overwrite an already-deployed satellite
    // by reusing a salt.
    client.deploy_contract(&wasm_hash, &salt, &vec![&env]);
}

#[test]
#[should_panic(expected = "Error(Contract, #4000)")] // DeployedContractNotFound
fn get_deployed_contract_rejects_out_of_range_index() {
    let env = Env::default();
    env.mock_all_auths();

    let signers = default_signers(&env);
    let policies = Map::new(&env);
    let (_account_id, client) = register_account(&env, &signers, &policies);

    client.get_deployed_contract(&0);
}

// ################## DEPLOY_CONTRACT: CREATE_CONTRACT AUTHORIZATION #########
//
// Same technique as the session-key tests above: `deploy_v2`'s resulting
// `CreateContractWithCtorHostFn` auth context can't be expressed through
// `mock_auths`/`MockAuthInvoke` (which only models `ContractFn`-shaped
// invocations), so these exercise `smart_account::do_check_auth` directly
// against a hand-built `Context::CreateContractWithCtorHostFn` — the same
// context `deploy_contract`'s real `deploy_v2` call produces internally, per
// `get_validated_context_by_id`'s own match arm for it.

fn get_create_context(
    wasm_hash: BytesN<32>,
    salt: BytesN<32>,
    constructor_args: Vec<Val>,
) -> Context {
    Context::CreateContractWithCtorHostFn(CreateContractWithConstructorHostFnContext {
        executable: ContractExecutable::Wasm(wasm_hash),
        salt,
        constructor_args,
    })
}

#[test]
fn create_contract_rule_with_matching_signer_authorizes_deployment() {
    let env = Env::default();
    env.mock_all_auths();

    let owner_signers = default_signers(&env);
    let policies = Map::new(&env);
    let (account_id, client) = register_account(&env, &owner_signers, &policies);

    let wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deploy_signer = Signer::Delegated(Address::generate(&env));
    let deploy_signers = vec![&env, deploy_signer];

    let rule = client.add_context_rule(
        &ContextRuleType::CreateContract(wasm_hash.clone()),
        &String::from_str(&env, "deploy"),
        &None,
        &deploy_signers,
        &Map::new(&env),
    );

    let salt = BytesN::from_array(&env, &[9u8; 32]);
    let context = get_create_context(wasm_hash, salt, vec![&env]);
    let auth_contexts = Vec::from_array(&env, [context]);
    let signatures = create_signatures(&env, &deploy_signers, vec![&env, rule.id]);
    let payload_hash = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));

    env.as_contract(&account_id, || {
        let result = smart_account::do_check_auth(&env, &payload_hash, &signatures, &auth_contexts);
        assert!(result.is_ok());
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3002)")] // UnvalidatedContext
fn create_contract_rule_rejects_different_wasm_hash() {
    let env = Env::default();
    env.mock_all_auths();

    let owner_signers = default_signers(&env);
    let policies = Map::new(&env);
    let (account_id, client) = register_account(&env, &owner_signers, &policies);

    let allowed_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deploy_signer = Signer::Delegated(Address::generate(&env));
    let deploy_signers = vec![&env, deploy_signer];

    let rule = client.add_context_rule(
        &ContextRuleType::CreateContract(allowed_wasm_hash),
        &String::from_str(&env, "deploy"),
        &None,
        &deploy_signers,
        &Map::new(&env),
    );

    // A different wasm hash than the one the rule allows.
    let other_wasm_hash = BytesN::from_array(&env, &[2u8; 32]);
    let salt = BytesN::from_array(&env, &[9u8; 32]);
    let context = get_create_context(other_wasm_hash, salt, vec![&env]);
    let auth_contexts = Vec::from_array(&env, [context]);
    let signatures = create_signatures(&env, &deploy_signers, vec![&env, rule.id]);
    let payload_hash = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));

    env.as_contract(&account_id, || {
        let _ = smart_account::do_check_auth(&env, &payload_hash, &signatures, &auth_contexts);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3002)")] // UnvalidatedContext
fn create_contract_rule_rejects_non_matching_signer() {
    let env = Env::default();
    env.mock_all_auths();

    let owner_signers = default_signers(&env);
    let policies = Map::new(&env);
    let (account_id, client) = register_account(&env, &owner_signers, &policies);

    let wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    let deploy_signer = Signer::Delegated(Address::generate(&env));
    let deploy_signers = vec![&env, deploy_signer];

    let rule = client.add_context_rule(
        &ContextRuleType::CreateContract(wasm_hash.clone()),
        &String::from_str(&env, "deploy"),
        &None,
        &deploy_signers,
        &Map::new(&env),
    );

    // A signer that has nothing to do with the rule's signer set.
    let attacker_signer = Signer::Delegated(Address::generate(&env));
    let attacker_signers = vec![&env, attacker_signer];

    let salt = BytesN::from_array(&env, &[9u8; 32]);
    let context = get_create_context(wasm_hash, salt, vec![&env]);
    let auth_contexts = Vec::from_array(&env, [context]);
    let signatures = create_signatures(&env, &attacker_signers, vec![&env, rule.id]);
    let payload_hash = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));

    env.as_contract(&account_id, || {
        let _ = smart_account::do_check_auth(&env, &payload_hash, &signatures, &auth_contexts);
    });
}
