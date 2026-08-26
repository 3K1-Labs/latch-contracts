extern crate std;

use soroban_sdk::{
    auth::Context,
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Events, Ledger},
    vec, Address, Env, IntoVal, Symbol, Val, Vec,
};

use crate::{TimelockAccountParams, TimelockPolicy};

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

fn create_call_context(_e: &Env, target: &Address, fn_name: Symbol, args: Vec<Val>) -> Context {
    Context::Contract(soroban_sdk::auth::ContractContext {
        contract: target.clone(),
        fn_name,
        args,
    })
}

fn create_context_rule(e: &Env, target: &Address) -> stellar_accounts::smart_account::ContextRule {
    let signer = Address::generate(e);
    let mut signers = Vec::new(e);
    signers.push_back(stellar_accounts::smart_account::Signer::Delegated(signer));

    stellar_accounts::smart_account::ContextRule {
        id: 1,
        context_type: stellar_accounts::smart_account::ContextRuleType::CallContract(
            target.clone(),
        ),
        name: soroban_sdk::String::from_str(e, "timelock"),
        signers,
        signer_ids: Vec::new(e),
        policies: Vec::new(e),
        policy_ids: Vec::new(e),
        valid_until: None,
    }
}

fn install(
    e: &Env,
    timelock_id: &Address,
    params: &TimelockAccountParams,
    context_rule: &stellar_accounts::smart_account::ContextRule,
    smart_account: &Address,
) {
    e.as_contract(timelock_id, || {
        crate::install_timelock(e, params, context_rule, smart_account);
    });
}

// ################## INSTALL TESTS ##################

#[test]
fn install_success() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);
    let target = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target);

    e.mock_all_auths();

    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 100, cancellable_by: Vec::new(&e) },
        &context_rule,
        &smart_account,
    );

    e.as_contract(&timelock_id, || {
        let config = crate::TimelockPolicy::get_config(&e, context_rule.id, &smart_account);
        assert_eq!(config.delay_ledgers, 100);
        assert!(config.cancellable_by.is_empty());
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn install_rejects_already_installed() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);
    let target = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target);
    let params = TimelockAccountParams { delay_ledgers: 100, cancellable_by: Vec::new(&e) };

    e.mock_all_auths();

    // First install succeeds
    install(&e, &timelock_id, &params, &context_rule, &smart_account);

    // Second install in the same frame panics with AlreadyInstalled
    e.as_contract(&timelock_id, || {
        crate::install_timelock(&e, &params, &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn install_rejects_zero_delay() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);
    let target = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target);

    e.mock_all_auths();

    e.as_contract(&timelock_id, || {
        let params = TimelockAccountParams { delay_ledgers: 0, cancellable_by: Vec::new(&e) };

        crate::install_timelock(&e, &params, &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn install_rejects_default_rule() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);

    let context_rule = stellar_accounts::smart_account::ContextRule {
        id: 1,
        context_type: stellar_accounts::smart_account::ContextRuleType::Default,
        name: soroban_sdk::String::from_str(&e, "default"),
        signers: Vec::new(&e),
        signer_ids: Vec::new(&e),
        policies: Vec::new(&e),
        policy_ids: Vec::new(&e),
        valid_until: None,
    };

    e.mock_all_auths();

    e.as_contract(&timelock_id, || {
        let params = TimelockAccountParams { delay_ledgers: 100, cancellable_by: Vec::new(&e) };

        crate::install_timelock(&e, &params, &context_rule, &smart_account);
    });
}

// ################## ENFORCE / PROPOSE TESTS ##################

#[test]
fn enforce_creates_proposal() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);
    let target = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target);

    e.mock_all_auths();

    // Install (separate closure to avoid double require_auth)
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 10, cancellable_by: Vec::new(&e) },
        &context_rule,
        &smart_account,
    );

    // Propose via enforce (separate closure)
    e.as_contract(&timelock_id, || {
        let context =
            create_call_context(&e, &target, symbol_short!("set"), vec![&e, 42u32.into_val(&e)]);
        let signers = context_rule.signers.clone();

        crate::enforce_proposal(&e, &context, &signers, &context_rule, &smart_account);

        // Verify proposal was stored
        let proposal = crate::TimelockPolicy::get_proposal(&e, &smart_account, 0);
        assert_eq!(proposal.target, target);
        assert_eq!(proposal.fn_name, symbol_short!("set"));
        assert_eq!(proposal.unlock_ledger, e.ledger().sequence() + 10);
        assert_eq!(proposal.context_rule_id, 1);

        // Events don't carry over across separate as_contract closures,
        // so only the proposed event is visible in this frame.
        assert_eq!(e.events().all().events().len(), 1);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn enforce_rejects_empty_signers() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);
    let target = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target);

    e.mock_all_auths();

    // Install
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 10, cancellable_by: Vec::new(&e) },
        &context_rule,
        &smart_account,
    );

    // Propose with empty signers
    e.as_contract(&timelock_id, || {
        let context =
            create_call_context(&e, &target, symbol_short!("set"), vec![&e, 42u32.into_val(&e)]);
        let empty_signers: Vec<stellar_accounts::smart_account::Signer> = Vec::new(&e);

        crate::enforce_proposal(&e, &context, &empty_signers, &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn enforce_rejects_when_not_installed() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);
    let target = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target);

    e.mock_all_auths();

    e.as_contract(&timelock_id, || {
        let context =
            create_call_context(&e, &target, symbol_short!("set"), vec![&e, 42u32.into_val(&e)]);
        let signers = context_rule.signers.clone();

        crate::enforce_proposal(&e, &context, &signers, &context_rule, &smart_account);
    });
}

#[test]
fn multiple_proposals_get_unique_ids() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);
    let target = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target);

    e.mock_all_auths();

    // Install
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 10, cancellable_by: Vec::new(&e) },
        &context_rule,
        &smart_account,
    );

    // Propose twice
    e.as_contract(&timelock_id, || {
        let context1 =
            create_call_context(&e, &target, symbol_short!("set"), vec![&e, 1u32.into_val(&e)]);
        let context2 =
            create_call_context(&e, &target, symbol_short!("set"), vec![&e, 2u32.into_val(&e)]);
        let signers = context_rule.signers.clone();

        crate::enforce_proposal(&e, &context1, &signers, &context_rule, &smart_account);
        crate::enforce_proposal(&e, &context2, &signers, &context_rule, &smart_account);

        let p0 = crate::TimelockPolicy::get_proposal(&e, &smart_account, 0);
        let p1 = crate::TimelockPolicy::get_proposal(&e, &smart_account, 1);
        assert_eq!(p0.unlock_ledger, p1.unlock_ledger);
        assert_ne!(p0.args, p1.args);
    });
}

// ################## EXECUTE TESTS ##################

#[test]
fn execute_after_delay_succeeds() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let target_id = e.register(MockTargetContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target_id);

    e.mock_all_auths();

    // Install
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 10, cancellable_by: Vec::new(&e) },
        &context_rule,
        &smart_account,
    );

    // Propose
    e.as_contract(&timelock_id, || {
        let context =
            create_call_context(&e, &target_id, symbol_short!("set"), vec![&e, 42u32.into_val(&e)]);
        let signers = context_rule.signers.clone();
        crate::enforce_proposal(&e, &context, &signers, &context_rule, &smart_account);
    });

    // Advance past the delay
    let unlock = e.ledger().sequence() + 10;
    e.ledger().with_mut(|li| {
        li.sequence_number = unlock;
    });

    // Execute
    e.as_contract(&timelock_id, || {
        TimelockPolicy::execute_pending(e.clone(), smart_account.clone(), 0);
    });

    // Verify target was called
    let target_client = MockTargetContractClient::new(&e, &target_id);
    assert_eq!(target_client.get(), 42);

    // Verify proposal was removed
    e.as_contract(&timelock_id, || {
        let key = crate::TimelockStorageKey::Proposal(smart_account.clone(), 0);
        assert!(!e.storage().persistent().has(&key));
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn execute_before_delay_fails() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let target_id = e.register(MockTargetContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target_id);

    e.mock_all_auths();

    // Install
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 100, cancellable_by: Vec::new(&e) },
        &context_rule,
        &smart_account,
    );

    // Propose
    e.as_contract(&timelock_id, || {
        let context =
            create_call_context(&e, &target_id, symbol_short!("set"), vec![&e, 42u32.into_val(&e)]);
        let signers = context_rule.signers.clone();
        crate::enforce_proposal(&e, &context, &signers, &context_rule, &smart_account);
    });

    // Try to execute immediately (without advancing ledger)
    e.as_contract(&timelock_id, || {
        TimelockPolicy::execute_pending(e.clone(), smart_account.clone(), 0);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn execute_nonexistent_proposal_fails() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);

    e.as_contract(&timelock_id, || {
        TimelockPolicy::execute_pending(e.clone(), smart_account.clone(), 999);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn execute_already_executed_proposal_fails() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let target_id = e.register(MockTargetContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target_id);

    e.mock_all_auths();

    // Install
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 5, cancellable_by: Vec::new(&e) },
        &context_rule,
        &smart_account,
    );

    // Propose
    e.as_contract(&timelock_id, || {
        let context =
            create_call_context(&e, &target_id, symbol_short!("set"), vec![&e, 42u32.into_val(&e)]);
        let signers = context_rule.signers.clone();
        crate::enforce_proposal(&e, &context, &signers, &context_rule, &smart_account);
    });

    // Advance + Execute once
    e.ledger().with_mut(|li| {
        li.sequence_number = e.ledger().sequence() + 5;
    });

    e.as_contract(&timelock_id, || {
        TimelockPolicy::execute_pending(e.clone(), smart_account.clone(), 0);
    });

    // Try to execute again — should fail
    e.as_contract(&timelock_id, || {
        TimelockPolicy::execute_pending(e.clone(), smart_account.clone(), 0);
    });
}

// ################## CANCEL TESTS ##################

#[test]
fn cancel_before_unlock_blocks_execution() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let target_id = e.register(MockTargetContract, ());
    let smart_account = Address::generate(&e);
    let canceller = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target_id);

    e.mock_all_auths();

    // Install with canceller
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 100, cancellable_by: vec![&e, canceller.clone()] },
        &context_rule,
        &smart_account,
    );

    // Propose
    e.as_contract(&timelock_id, || {
        let context =
            create_call_context(&e, &target_id, symbol_short!("set"), vec![&e, 42u32.into_val(&e)]);
        let signers = context_rule.signers.clone();
        crate::enforce_proposal(&e, &context, &signers, &context_rule, &smart_account);
    });

    // Cancel (canceller authorizes)
    e.as_contract(&timelock_id, || {
        TimelockPolicy::cancel(e.clone(), canceller.clone(), smart_account.clone(), 0);
    });

    // Try to execute — should fail with ProposalNotFound
    e.as_contract(&timelock_id, || {
        TimelockPolicy::execute_pending(e.clone(), smart_account.clone(), 0);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn cancel_after_execute_is_error() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let target_id = e.register(MockTargetContract, ());
    let smart_account = Address::generate(&e);
    let canceller = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target_id);

    e.mock_all_auths();

    // Install
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 5, cancellable_by: vec![&e, canceller.clone()] },
        &context_rule,
        &smart_account,
    );

    // Propose
    e.as_contract(&timelock_id, || {
        let context =
            create_call_context(&e, &target_id, symbol_short!("set"), vec![&e, 42u32.into_val(&e)]);
        let signers = context_rule.signers.clone();
        crate::enforce_proposal(&e, &context, &signers, &context_rule, &smart_account);
    });

    // Advance + Execute
    e.ledger().with_mut(|li| {
        li.sequence_number = e.ledger().sequence() + 5;
    });

    e.as_contract(&timelock_id, || {
        TimelockPolicy::execute_pending(e.clone(), smart_account.clone(), 0);
    });

    // Try to cancel — should fail (proposal already removed)
    e.as_contract(&timelock_id, || {
        TimelockPolicy::cancel(e.clone(), canceller.clone(), smart_account.clone(), 0);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn unauthorized_cancel_rejected() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let target_id = e.register(MockTargetContract, ());
    let smart_account = Address::generate(&e);
    let canceller = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target_id);

    e.mock_all_auths();

    // Install with specific canceller
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 100, cancellable_by: vec![&e, canceller.clone()] },
        &context_rule,
        &smart_account,
    );

    // Propose
    e.as_contract(&timelock_id, || {
        let context =
            create_call_context(&e, &target_id, symbol_short!("set"), vec![&e, 42u32.into_val(&e)]);
        let signers = context_rule.signers.clone();
        crate::enforce_proposal(&e, &context, &signers, &context_rule, &smart_account);
    });

    // Try to cancel from unauthorized address
    e.as_contract(&timelock_id, || {
        TimelockPolicy::cancel(e.clone(), unauthorized.clone(), smart_account.clone(), 0);
    });
}

#[test]
fn cancel_by_smart_account_when_cancellable_by_empty() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let target_id = e.register(MockTargetContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target_id);

    e.mock_all_auths();

    // Install with empty cancellable_by
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 100, cancellable_by: Vec::new(&e) },
        &context_rule,
        &smart_account,
    );

    // Propose
    e.as_contract(&timelock_id, || {
        let context =
            create_call_context(&e, &target_id, symbol_short!("set"), vec![&e, 42u32.into_val(&e)]);
        let signers = context_rule.signers.clone();
        crate::enforce_proposal(&e, &context, &signers, &context_rule, &smart_account);
    });

    // Smart account cancels (empty cancellable_by → only smart_account allowed)
    e.as_contract(&timelock_id, || {
        TimelockPolicy::cancel(e.clone(), smart_account.clone(), smart_account.clone(), 0);
    });

    // Verify proposal was removed
    e.as_contract(&timelock_id, || {
        let key = crate::TimelockStorageKey::Proposal(smart_account.clone(), 0);
        assert!(!e.storage().persistent().has(&key));
    });
}

// ################## UNINSTALL TESTS ##################

#[test]
fn uninstall_success() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);
    let target = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target);

    e.mock_all_auths();

    // Install
    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 100, cancellable_by: Vec::new(&e) },
        &context_rule,
        &smart_account,
    );

    // Uninstall (separate closure)
    e.as_contract(&timelock_id, || {
        crate::uninstall_timelock(&e, &context_rule, &smart_account);
        // Only the uninstall event is visible in this frame.
        assert_eq!(e.events().all().events().len(), 1);
    });

    // Verify config is removed
    e.as_contract(&timelock_id, || {
        let key = crate::TimelockStorageKey::Config(smart_account.clone(), context_rule.id);
        assert!(!e.storage().persistent().has(&key));
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn uninstall_rejects_when_not_installed() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);
    let target = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target);

    e.mock_all_auths();

    e.as_contract(&timelock_id, || {
        crate::uninstall_timelock(&e, &context_rule, &smart_account);
    });
}

// ################## QUERY TESTS ##################

#[test]
fn get_config_returns_installed_value() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);
    let canceller = Address::generate(&e);
    let target = Address::generate(&e);
    let context_rule = create_context_rule(&e, &target);

    e.mock_all_auths();

    install(
        &e,
        &timelock_id,
        &TimelockAccountParams { delay_ledgers: 50, cancellable_by: vec![&e, canceller.clone()] },
        &context_rule,
        &smart_account,
    );

    e.as_contract(&timelock_id, || {
        let config = TimelockPolicy::get_config(&e, context_rule.id, &smart_account);
        assert_eq!(config.delay_ledgers, 50);
        assert_eq!(config.cancellable_by.len(), 1);
        assert!(config.cancellable_by.contains(&canceller));
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn get_config_rejects_when_not_installed() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);

    e.as_contract(&timelock_id, || {
        TimelockPolicy::get_config(&e, 1, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn get_proposal_rejects_nonexistent() {
    let e = Env::default();
    let timelock_id = e.register(TimelockPolicy, ());
    let smart_account = Address::generate(&e);

    e.as_contract(&timelock_id, || {
        TimelockPolicy::get_proposal(&e, &smart_account, 0);
    });
}
