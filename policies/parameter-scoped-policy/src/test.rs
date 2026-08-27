#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{
    auth::{Context, ContractContext},
    testutils::Address as _,
    Address, Env, IntoVal, Map, String, Symbol, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

fn setup() -> (Env, Address, u32, Address, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let smart_account = Address::generate(&e);
    let target_contract = Address::generate(&e);
    let context_rule_id = 1;
    // Register the contract so the host knows it exists
    let policy_id = e.register(ParameterScopedPolicy, ());
    (e, smart_account, context_rule_id, target_contract, policy_id)
}

fn create_context_rule(e: &Env, target: &Address) -> ContextRule {
    let signer = Address::generate(e);
    let mut signers = Vec::new(e);
    signers.push_back(Signer::Delegated(signer));

    ContextRule {
        id: 1,
        context_type: ContextRuleType::CallContract(target.clone()),
        name: String::from_str(e, "parameter_scoped"),
        signers,
        signer_ids: Vec::new(e),
        policies: Vec::new(e),
        policy_ids: Vec::new(e),
        valid_until: None,
    }
}

fn create_contract_context(
    e: &Env,
    target: &Address,
    fn_name: &str,
    args: Vec<soroban_sdk::Val>,
) -> Context {
    Context::Contract(ContractContext {
        contract: target.clone(),
        fn_name: Symbol::new(e, fn_name),
        args,
    })
}

#[test]
fn test_install_and_get_conditions() {
    let (e, smart_account, rule_id, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);

    let mut conditions = Map::new(&e);
    let conds = Vec::from_array(
        &e,
        [Condition {
            arg_index: 0,
            operator: Operator::Eq,
            expected_value: ExpectedValue::U32(42),
        }],
    );
    conditions.set(Symbol::new(&e, "transfer"), conds);

    let params = ParameterScopedAccountParams { conditions };

    // Wrap the execution so the host knows a contract is running
    e.as_contract(&policy_id, || {
        conditions::install(&e, &params, &rule, &smart_account);
        let stored = conditions::get_conditions(&e, rule_id, &smart_account);
        assert_eq!(stored.len(), 1);
    });
}

// ==========================================
// U32 OPERATOR TESTS
// ==========================================

fn run_operator_test(operator: Operator, expected: u32, actual: u32) {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);

    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition { arg_index: 0, operator, expected_value: ExpectedValue::U32(expected) }],
        ),
    );

    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });

    let context =
        create_contract_context(&e, &target, "swap", Vec::from_array(&e, [actual.into_val(&e)]));
    let signers = Vec::from_array(&e, [Signer::Delegated(Address::generate(&e))]);

    e.as_contract(&policy_id, || {
        conditions::enforce(&e, &context, &signers, &rule, &smart_account);
    });
}

#[test]
fn test_operator_eq_accept() {
    run_operator_test(Operator::Eq, 100, 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_operator_eq_reject() {
    run_operator_test(Operator::Eq, 100, 101);
}

#[test]
fn test_operator_neq_accept() {
    run_operator_test(Operator::Neq, 100, 101);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_operator_neq_reject() {
    run_operator_test(Operator::Neq, 100, 100);
}

#[test]
fn test_operator_gt_accept() {
    run_operator_test(Operator::Gt, 100, 101);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_operator_gt_reject_equal() {
    run_operator_test(Operator::Gt, 100, 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_operator_gt_reject_less() {
    run_operator_test(Operator::Gt, 100, 99);
}

#[test]
fn test_operator_gte_accept_greater() {
    run_operator_test(Operator::Gte, 100, 101);
}

#[test]
fn test_operator_gte_accept_equal() {
    run_operator_test(Operator::Gte, 100, 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_operator_gte_reject() {
    run_operator_test(Operator::Gte, 100, 99);
}

#[test]
fn test_operator_lt_accept() {
    run_operator_test(Operator::Lt, 100, 99);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_operator_lt_reject_equal() {
    run_operator_test(Operator::Lt, 100, 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_operator_lt_reject_greater() {
    run_operator_test(Operator::Lt, 100, 101);
}

#[test]
fn test_operator_lte_accept_less() {
    run_operator_test(Operator::Lte, 100, 99);
}

#[test]
fn test_operator_lte_accept_equal() {
    run_operator_test(Operator::Lte, 100, 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_operator_lte_reject() {
    run_operator_test(Operator::Lte, 100, 101);
}

// ==========================================
// ADDITIONAL TYPE TESTS (I32, U64, I64, U128, I128, Sym, Addr)
// ==========================================

#[test]
fn test_type_i32_accept() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);
    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 0,
                operator: Operator::Lt,
                expected_value: ExpectedValue::I32(0),
            }],
        ),
    );
    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });
    let context =
        create_contract_context(&e, &target, "swap", Vec::from_array(&e, [(-10i32).into_val(&e)]));
    let signers = Vec::from_array(&e, [Signer::Delegated(Address::generate(&e))]);
    e.as_contract(&policy_id, || {
        conditions::enforce(&e, &context, &signers, &rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_type_u64_reject() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);
    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 0,
                operator: Operator::Eq,
                expected_value: ExpectedValue::U64(500),
            }],
        ),
    );
    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });
    let context =
        create_contract_context(&e, &target, "swap", Vec::from_array(&e, [501u64.into_val(&e)]));
    let signers = Vec::from_array(&e, [Signer::Delegated(Address::generate(&e))]);
    e.as_contract(&policy_id, || {
        conditions::enforce(&e, &context, &signers, &rule, &smart_account);
    });
}

#[test]
fn test_type_i64_accept() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);
    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 0,
                operator: Operator::Gte,
                expected_value: ExpectedValue::I64(-1000),
            }],
        ),
    );
    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });
    let context =
        create_contract_context(&e, &target, "swap", Vec::from_array(&e, [(-500i64).into_val(&e)]));
    let signers = Vec::from_array(&e, [Signer::Delegated(Address::generate(&e))]);
    e.as_contract(&policy_id, || {
        conditions::enforce(&e, &context, &signers, &rule, &smart_account);
    });
}

#[test]
fn test_type_u128_accept() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);
    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 0,
                operator: Operator::Gte,
                expected_value: ExpectedValue::U128(1_000_000),
            }],
        ),
    );
    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });
    let context = create_contract_context(
        &e,
        &target,
        "swap",
        Vec::from_array(&e, [1_000_000u128.into_val(&e)]),
    );
    let signers = Vec::from_array(&e, [Signer::Delegated(Address::generate(&e))]);
    e.as_contract(&policy_id, || {
        conditions::enforce(&e, &context, &signers, &rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_type_i128_reject() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);
    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 0,
                operator: Operator::Lt,
                expected_value: ExpectedValue::I128(-500),
            }],
        ),
    );
    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });
    let context = create_contract_context(
        &e,
        &target,
        "swap",
        Vec::from_array(&e, [(-500i128).into_val(&e)]),
    );
    let signers = Vec::from_array(&e, [Signer::Delegated(Address::generate(&e))]);
    e.as_contract(&policy_id, || {
        conditions::enforce(&e, &context, &signers, &rule, &smart_account);
    });
}

#[test]
fn test_type_sym_accept() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);
    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 0,
                operator: Operator::Eq,
                expected_value: ExpectedValue::Sym(Symbol::new(&e, "USDC")),
            }],
        ),
    );
    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });
    let context = create_contract_context(
        &e,
        &target,
        "swap",
        Vec::from_array(&e, [Symbol::new(&e, "USDC").into_val(&e)]),
    );
    let signers = Vec::from_array(&e, [Signer::Delegated(Address::generate(&e))]);
    e.as_contract(&policy_id, || {
        conditions::enforce(&e, &context, &signers, &rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_type_addr_reject() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);
    let expected_addr = Address::generate(&e);
    let actual_addr = Address::generate(&e);
    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 0,
                operator: Operator::Eq,
                expected_value: ExpectedValue::Addr(expected_addr),
            }],
        ),
    );
    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });
    let context = create_contract_context(
        &e,
        &target,
        "swap",
        Vec::from_array(&e, [actual_addr.into_val(&e)]),
    );
    let signers = Vec::from_array(&e, [Signer::Delegated(Address::generate(&e))]);
    e.as_contract(&policy_id, || {
        conditions::enforce(&e, &context, &signers, &rule, &smart_account);
    });
}

// ==========================================
// EDGE CASES
// ==========================================

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_install_sym_rejects_invalid_operator() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);
    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 0,
                operator: Operator::Gt, // Invalid for Sym
                expected_value: ExpectedValue::Sym(Symbol::new(&e, "USDC")),
            }],
        ),
    );
    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_install_addr_rejects_invalid_operator() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);
    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 0,
                operator: Operator::Lt, // Invalid for Addr
                expected_value: ExpectedValue::Addr(Address::generate(&e)),
            }],
        ),
    );
    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_enforce_fails_index_out_of_bounds() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);

    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 1,
                operator: Operator::Eq,
                expected_value: ExpectedValue::U32(100),
            }],
        ),
    );

    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });

    let context =
        create_contract_context(&e, &target, "swap", Vec::from_array(&e, [100u32.into_val(&e)]));
    let signers = Vec::from_array(&e, [Signer::Delegated(Address::generate(&e))]);

    e.as_contract(&policy_id, || {
        conditions::enforce(&e, &context, &signers, &rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_enforce_fails_unconfigured_function() {
    let (e, smart_account, _, target, policy_id) = setup();
    let rule = create_context_rule(&e, &target);

    let mut conditions = Map::new(&e);
    conditions.set(
        Symbol::new(&e, "swap"),
        Vec::from_array(
            &e,
            [Condition {
                arg_index: 0,
                operator: Operator::Eq,
                expected_value: ExpectedValue::U32(100),
            }],
        ),
    );

    e.as_contract(&policy_id, || {
        conditions::install(
            &e,
            &ParameterScopedAccountParams { conditions },
            &rule,
            &smart_account,
        );
    });

    let context = create_contract_context(&e, &target, "approve", Vec::new(&e));
    let signers = Vec::from_array(&e, [Signer::Delegated(Address::generate(&e))]);

    e.as_contract(&policy_id, || {
        conditions::enforce(&e, &context, &signers, &rule, &smart_account);
    });
}
