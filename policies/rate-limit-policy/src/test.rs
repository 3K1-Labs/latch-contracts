extern crate std;

use soroban_sdk::{
    auth::{Context, ContractContext, ContractExecutable, CreateContractHostFnContext},
    contract, symbol_short,
    testutils::{Address as _, Events, Ledger},
    vec, Address, BytesN, Env, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

use crate::rate_limit::*;

// ################## TEST HELPERS ##################

#[contract]
struct MockContract;

/// Build a `CallContract` context rule with one delegated signer.
fn create_context_rule(e: &Env) -> ContextRule {
    let signer = Address::generate(e);
    let mut signers = Vec::new(e);
    signers.push_back(Signer::Delegated(signer));

    ContextRule {
        id: 1,
        context_type: ContextRuleType::CallContract(Address::generate(e)),
        name: soroban_sdk::String::from_str(e, "rate_limit"),
        signers,
        signer_ids: Vec::new(e),
        policies: Vec::new(e),
        policy_ids: Vec::new(e),
        valid_until: None,
    }
}

fn create_contract_context(e: &Env) -> Context {
    Context::Contract(ContractContext {
        contract: Address::generate(e),
        fn_name: symbol_short!("call"),
        args: Vec::new(e),
    })
}

fn default_params(max_calls: u32, period_ledgers: u32) -> RateLimitAccountParams {
    RateLimitAccountParams { max_calls, period_ledgers }
}

// ################## INSTALL TESTS ##################

#[test]
fn install_success() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let context_rule = create_context_rule(&e);
        let params = default_params(10, 100);

        install(&e, &params, &context_rule, &smart_account);

        let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
        assert_eq!(data.max_calls, 10);
        assert_eq!(data.period_ledgers, 100);
        assert_eq!(data.cached_call_count, 0);
        assert_eq!(data.call_history.len(), 0);

        // install event was emitted
        assert_eq!(e.events().all().events().len(), 1);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn install_rejects_default_rule() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let mut context_rule = create_context_rule(&e);
        context_rule.context_type = ContextRuleType::Default;
        install(&e, &default_params(10, 100), &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn install_rejects_create_contract_rule() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let mut context_rule = create_context_rule(&e);
        context_rule.context_type =
            ContextRuleType::CreateContract(BytesN::from_array(&e, &[0u8; 32]));
        install(&e, &default_params(10, 100), &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn install_rejects_zero_max_calls() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let context_rule = create_context_rule(&e);
        install(&e, &default_params(0, 100), &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn install_rejects_zero_period_ledgers() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let context_rule = create_context_rule(&e);
        install(&e, &default_params(10, 0), &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn install_rejects_already_installed() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(10, 100), &context_rule, &smart_account);
    });
    e.as_contract(&address, || {
        install(&e, &default_params(10, 100), &context_rule, &smart_account);
    });
}

// ################## ENFORCE TESTS ##################

#[test]
fn enforce_allows_call_within_limit() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(5, 100), &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        let context = create_contract_context(&e);
        enforce(&e, &context, &context_rule.signers, &context_rule, &smart_account);

        let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
        assert_eq!(data.cached_call_count, 1);
        assert_eq!(data.call_history.len(), 1);
    });
}

#[test]
fn enforce_allows_calls_up_to_exact_limit() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(3, 100), &context_rule, &smart_account);
    });

    // 3 calls should all succeed (limit is 3)
    for i in 0..3u32 {
        e.as_contract(&address, || {
            let context = create_contract_context(&e);
            enforce(&e, &context, &context_rule.signers, &context_rule, &smart_account);
            let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
            assert_eq!(data.cached_call_count, i + 1);
        });
    }
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn enforce_rejects_call_exceeding_limit() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(2, 100), &context_rule, &smart_account);
    });

    // Make 2 calls (at the limit)
    for _ in 0..2 {
        e.as_contract(&address, || {
            let context = create_contract_context(&e);
            enforce(&e, &context, &context_rule.signers, &context_rule, &smart_account);
        });
    }

    // 3rd call must be rejected
    e.as_contract(&address, || {
        let context = create_contract_context(&e);
        enforce(&e, &context, &context_rule.signers, &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn enforce_rejects_non_contract_context() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(10, 100), &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        let context = Context::CreateContractHostFn(CreateContractHostFnContext {
            salt: BytesN::from_array(&e, &[1u8; 32]),
            executable: ContractExecutable::Wasm(BytesN::from_array(&e, &[1u8; 32])),
        });
        enforce(&e, &context, &context_rule.signers, &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn enforce_rejects_empty_authenticated_signers() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(10, 100), &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        let context = create_contract_context(&e);
        enforce(&e, &context, &Vec::new(&e), &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn enforce_rejects_when_not_installed() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let context = create_contract_context(&e);
        enforce(&e, &context, &context_rule.signers, &context_rule, &smart_account);
    });
}

#[test]
fn enforce_counts_any_fn_name() {
    // The policy must count calls regardless of function name.
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(5, 100), &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        let context_a = Context::Contract(ContractContext {
            contract: Address::generate(&e),
            fn_name: symbol_short!("swap"),
            args: Vec::new(&e),
        });
        enforce(&e, &context_a, &context_rule.signers, &context_rule, &smart_account);

        let context_b = Context::Contract(ContractContext {
            contract: Address::generate(&e),
            fn_name: symbol_short!("transfer"),
            args: Vec::new(&e),
        });
        enforce(&e, &context_b, &context_rule.signers, &context_rule, &smart_account);

        let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
        assert_eq!(data.cached_call_count, 2);
    });
}

// ################## ROLLING WINDOW EVICTION TESTS ##################

#[test]
fn rolling_window_evicts_old_entries_and_allows_new_calls() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    // max_calls=3, period=20 ledgers. Start at ledger 1.
    e.ledger().set_sequence_number(1);

    e.as_contract(&address, || {
        install(&e, &default_params(3, 20), &context_rule, &smart_account);
    });

    // Make 3 calls at ledger 1 — fills the window.
    for _ in 0..3 {
        e.as_contract(&address, || {
            let ctx = create_contract_context(&e);
            enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
        });
    }

    // Advance to ledger 22. Cutoff = 22 - 20 = 2; all entries at ledger 1 are
    // evicted (1 <= 2).
    e.ledger().set_sequence_number(22);

    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);

        let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
        // Only the one new call at ledger 22 should remain.
        assert_eq!(data.cached_call_count, 1);
        assert_eq!(data.call_history.len(), 1);
        assert_eq!(data.call_history.get(0).unwrap().ledger_sequence, 22);
    });
}

#[test]
fn rolling_window_boundary_exactly_at_cutoff_is_evicted() {
    // An entry AT the cutoff ledger (current - period) is evicted.
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.ledger().set_sequence_number(10);

    e.as_contract(&address, || {
        install(&e, &default_params(3, 10), &context_rule, &smart_account);
    });

    // Call at ledger 10.
    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
    });

    // Advance to ledger 20. Cutoff = 20 - 10 = 10. Entry at ledger 10 has
    // ledger_sequence <= 10, so it IS evicted.
    e.ledger().set_sequence_number(20);

    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);

        let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
        assert_eq!(data.cached_call_count, 1);
    });
}

#[test]
fn rolling_window_boundary_just_inside_window_is_kept() {
    // An entry one ledger AFTER the cutoff must NOT be evicted.
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.ledger().set_sequence_number(11);

    e.as_contract(&address, || {
        install(&e, &default_params(3, 10), &context_rule, &smart_account);
    });

    // Call at ledger 11.
    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
    });

    // Advance to ledger 20. Cutoff = 20 - 10 = 10. Entry at ledger 11 has
    // ledger_sequence = 11 > 10, so it must NOT be evicted.
    e.ledger().set_sequence_number(20);

    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);

        let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
        assert_eq!(data.cached_call_count, 2);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn rate_limit_still_enforced_after_partial_eviction() {
    // Some entries evicted, but enough remain to still exceed the limit.
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.ledger().set_sequence_number(1);

    e.as_contract(&address, || {
        install(&e, &default_params(3, 20), &context_rule, &smart_account);
    });

    // 1 call at ledger 1 (will be evicted at ledger 22)
    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
    });

    // 2 calls at ledger 15 (cutoff at ledger 22 = 22-20 = 2; ledger 15 > 2, kept)
    e.ledger().set_sequence_number(15);
    for _ in 0..2 {
        e.as_contract(&address, || {
            let ctx = create_contract_context(&e);
            enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
        });
    }

    // Advance to ledger 22. Entry at ledger 1 evicted (1 <= 2), but the 2
    // entries at ledger 15 (15 > 2) remain. cached_call_count = 2 = max_calls.
    // So the 4th call should be rejected.
    e.ledger().set_sequence_number(22);
    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
    });
}

// ################## SET_MAX_CALLS TESTS ##################

#[test]
fn set_max_calls_updates_limit() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(5, 100), &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        set_max_calls(&e, 20, &context_rule, &smart_account);

        let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
        assert_eq!(data.max_calls, 20);

        // event emitted
        assert_eq!(e.events().all().events().len(), 1);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn set_max_calls_rejects_zero() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(5, 100), &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        set_max_calls(&e, 0, &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn set_max_calls_rejects_when_not_installed() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        set_max_calls(&e, 10, &context_rule, &smart_account);
    });
}

#[test]
fn set_max_calls_lowering_below_current_count_then_rejects() {
    // Lower max_calls so that the current window count already exceeds it;
    // the next enforce call should be rejected.
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(5, 100), &context_rule, &smart_account);
    });

    // Make 3 calls.
    for _ in 0..3 {
        e.as_contract(&address, || {
            let ctx = create_contract_context(&e);
            enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
        });
    }

    // Lower limit to 3 (exactly the current count).
    e.as_contract(&address, || {
        set_max_calls(&e, 3, &context_rule, &smart_account);
    });

    // Next call should be rejected.
    let result = std::panic::catch_unwind(|| {
        e.as_contract(&address, || {
            let ctx = create_contract_context(&e);
            enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
        });
    });
    assert!(result.is_err(), "expected enforce to panic after limit was lowered");
}

// ################## GET_RATE_LIMIT_DATA TESTS ##################

#[test]
fn get_rate_limit_data_returns_installed_values() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(7, 200), &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
        assert_eq!(data.max_calls, 7);
        assert_eq!(data.period_ledgers, 200);
        assert_eq!(data.cached_call_count, 0);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn get_rate_limit_data_rejects_when_not_installed() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        get_rate_limit_data(&e, context_rule.id, &smart_account);
    });
}

// ################## UNINSTALL TESTS ##################

#[test]
fn uninstall_success() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(10, 100), &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        uninstall(&e, &context_rule, &smart_account);

        // uninstall event emitted
        assert_eq!(e.events().all().events().len(), 1);
    });

    // Storage key must be gone.
    e.as_contract(&address, || {
        let key = RateLimitStorageKey::AccountContext(smart_account.clone(), context_rule.id);
        assert!(!e.storage().persistent().has(&key));
    });
}

#[test]
fn uninstall_clears_call_history() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(10, 100), &context_rule, &smart_account);
    });

    // Record some calls.
    for _ in 0..5 {
        e.as_contract(&address, || {
            let ctx = create_contract_context(&e);
            enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
        });
    }

    // Uninstall.
    e.as_contract(&address, || {
        uninstall(&e, &context_rule, &smart_account);
    });

    // Re-install and confirm history is gone (fresh state).
    e.as_contract(&address, || {
        install(&e, &default_params(10, 100), &context_rule, &smart_account);
        let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
        assert_eq!(data.cached_call_count, 0);
        assert_eq!(data.call_history.len(), 0);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn uninstall_rejects_when_not_installed() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        uninstall(&e, &context_rule, &smart_account);
    });
}

// ################## INDEPENDENT ACCOUNT / CONTEXT ISOLATION TESTS ##################

#[test]
fn separate_smart_accounts_have_independent_counters() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let account_a = Address::generate(&e);
    let account_b = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(2, 100), &context_rule, &account_a);
        install(&e, &default_params(2, 100), &context_rule, &account_b);
    });

    // Fill account_a's limit.
    for _ in 0..2 {
        e.as_contract(&address, || {
            let ctx = create_contract_context(&e);
            enforce(&e, &ctx, &context_rule.signers, &context_rule, &account_a);
        });
    }

    // account_b should still be under limit.
    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &context_rule.signers, &context_rule, &account_b);

        let data = get_rate_limit_data(&e, context_rule.id, &account_b);
        assert_eq!(data.cached_call_count, 1);
    });
}

#[test]
fn separate_context_rules_have_independent_counters() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);

    e.mock_all_auths();

    let rule_a = create_context_rule(&e);
    let mut rule_b = create_context_rule(&e);
    rule_b.id = 2;

    e.as_contract(&address, || {
        install(&e, &default_params(2, 100), &rule_a, &smart_account);
        install(&e, &default_params(2, 100), &rule_b, &smart_account);
    });

    // Fill rule_a's limit.
    for _ in 0..2 {
        e.as_contract(&address, || {
            let ctx = create_contract_context(&e);
            enforce(&e, &ctx, &rule_a.signers, &rule_a, &smart_account);
        });
    }

    // rule_b should still be under limit.
    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &rule_b.signers, &rule_b, &smart_account);

        let data = get_rate_limit_data(&e, rule_b.id, &smart_account);
        assert_eq!(data.cached_call_count, 1);
    });
}

// ################## ENFORCE EVENT TESTS ##################

#[test]
fn enforce_emits_event_with_correct_call_count() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        install(&e, &default_params(10, 100), &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);

        // One event from enforce.
        assert_eq!(e.events().all().events().len(), 1);
    });
}

// ################## cached_call_count CONSISTENCY TEST ##################

#[test]
fn cached_call_count_stays_consistent_with_history_length_after_eviction() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e);

    e.mock_all_auths();

    e.ledger().set_sequence_number(1);

    e.as_contract(&address, || {
        install(&e, &default_params(10, 20), &context_rule, &smart_account);
    });

    // 4 calls at ledger 1.
    for _ in 0..4 {
        e.as_contract(&address, || {
            let ctx = create_contract_context(&e);
            enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
        });
    }

    // 3 calls at ledger 15.
    e.ledger().set_sequence_number(15);
    for _ in 0..3 {
        e.as_contract(&address, || {
            let ctx = create_contract_context(&e);
            enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);
        });
    }

    // Advance to ledger 22. Cutoff = 22 - 20 = 2. Entries at ledger 1 (1 <= 2)
    // are evicted; entries at ledger 15 are kept. 1 new call added.
    e.ledger().set_sequence_number(22);
    e.as_contract(&address, || {
        let ctx = create_contract_context(&e);
        enforce(&e, &ctx, &context_rule.signers, &context_rule, &smart_account);

        let data = get_rate_limit_data(&e, context_rule.id, &smart_account);
        // 3 (at ledger 15) + 1 (at ledger 22) = 4
        assert_eq!(data.cached_call_count, 4);
        assert_eq!(data.call_history.len(), 4);
    });
}
