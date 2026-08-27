//! # Parameter Scoped Policy Module
//!
//! This policy restricts a context rule's signers to specific parameter values
//! when invoking a contract function.

use soroban_sdk::{
    auth::{Context, ContractContext},
    contracterror, contractevent, contracttype, panic_with_error, Address, Env, Map, Symbol, TryIntoVal, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Operator {
    Eq = 0,
    Neq = 1,
    Gt = 2,
    Gte = 3,
    Lt = 4,
    Lte = 5,
}

/// Explicit types supported for parameter condition checks.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ExpectedValue {
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    U128(u128),
    I128(i128),
    Sym(Symbol),
    Addr(Address),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    pub arg_index: u32,
    pub operator: Operator,
    pub expected_value: ExpectedValue,
}

#[contractevent]
#[derive(Clone)]
pub struct ConditionsEnforced {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
    pub fn_name: Symbol,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ConditionsInstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ConditionsUninstalled {
    #[topic]
    pub smart_account: Address,
    pub context_rule_id: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ParameterScopedAccountParams {
    pub conditions: Map<Symbol, Vec<Condition>>,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ParameterScopedData {
    pub conditions: Map<Symbol, Vec<Condition>>,
}

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum ParameterScopedError {
    SmartAccountNotInstalled = 1,
    OnlyCallContractAllowed = 2,
    InvalidConditions = 3,
    MethodNotAllowed = 4,
    ConditionFailed = 5,
    AlreadyInstalled = 6,
    ArgumentIndexOutOfBounds = 7,
}

#[contracttype]
pub enum ParameterScopedStorageKey {
    AccountContext(Address, u32),
}

const DAY_IN_LEDGERS: u32 = 17280;
pub const PARAMETER_SCOPED_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub const PARAMETER_SCOPED_TTL_THRESHOLD: u32 = PARAMETER_SCOPED_EXTEND_AMOUNT - DAY_IN_LEDGERS;
pub const MAX_FNS: u32 = 10;
pub const MAX_CONDITIONS_PER_FN: u32 = 5;

pub fn emit_conditions_enforced(e: &Env, smart_account: Address, context_rule_id: u32, fn_name: Symbol) {
    ConditionsEnforced {
        smart_account,
        context_rule_id,
        fn_name,
    }
    .publish(e);
}

pub fn emit_conditions_installed(e: &Env, smart_account: Address, context_rule_id: u32) {
    ConditionsInstalled {
        smart_account,
        context_rule_id,
    }
    .publish(e);
}

pub fn emit_conditions_uninstalled(e: &Env, smart_account: Address, context_rule_id: u32) {
    ConditionsUninstalled {
        smart_account,
        context_rule_id,
    }
    .publish(e);
}

pub fn get_conditions(e: &Env, context_rule_id: u32, smart_account: &Address) -> Map<Symbol, Vec<Condition>> {
    let key = ParameterScopedStorageKey::AccountContext(smart_account.clone(), context_rule_id);
    e.storage()
        .persistent()
        .get::<_, ParameterScopedData>(&key)
        .inspect(|_| {
            e.storage().persistent().extend_ttl(
                &key,
                PARAMETER_SCOPED_TTL_THRESHOLD,
                PARAMETER_SCOPED_EXTEND_AMOUNT,
            );
        })
        .map(|data| data.conditions)
        .unwrap_or_else(|| panic_with_error!(e, ParameterScopedError::SmartAccountNotInstalled))
}

pub fn enforce(
    e: &Env,
    context: &Context,
    authenticated_signers: &Vec<Signer>,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    smart_account.require_auth();

    if authenticated_signers.is_empty() {
        panic_with_error!(e, ParameterScopedError::MethodNotAllowed)
    }

    let configured_fns = get_conditions(e, context_rule.id, smart_account);

    match context {
        Context::Contract(ContractContext { fn_name, args, .. }) => {
            let fn_conditions = configured_fns
                .get(fn_name.clone())
                .unwrap_or_else(|| panic_with_error!(e, ParameterScopedError::MethodNotAllowed));

            for cond in fn_conditions.into_iter() {
                let actual_arg = args
                    .get(cond.arg_index)
                    .unwrap_or_else(|| panic_with_error!(e, ParameterScopedError::ArgumentIndexOutOfBounds));

                let passed = match &cond.expected_value {
                    ExpectedValue::U32(expected) => {
                        if let Ok(actual) = actual_arg.try_into_val(e) {
                            let actual: u32 = actual;
                            match cond.operator {
                                Operator::Eq => actual == *expected,
                                Operator::Neq => actual != *expected,
                                Operator::Gt => actual > *expected,
                                Operator::Gte => actual >= *expected,
                                Operator::Lt => actual < *expected,
                                Operator::Lte => actual <= *expected,
                            }
                        } else {
                            false
                        }
                    }
                    ExpectedValue::I32(expected) => {
                        if let Ok(actual) = actual_arg.try_into_val(e) {
                            let actual: i32 = actual;
                            match cond.operator {
                                Operator::Eq => actual == *expected,
                                Operator::Neq => actual != *expected,
                                Operator::Gt => actual > *expected,
                                Operator::Gte => actual >= *expected,
                                Operator::Lt => actual < *expected,
                                Operator::Lte => actual <= *expected,
                            }
                        } else {
                            false
                        }
                    }
                    ExpectedValue::U64(expected) => {
                        if let Ok(actual) = actual_arg.try_into_val(e) {
                            let actual: u64 = actual;
                            match cond.operator {
                                Operator::Eq => actual == *expected,
                                Operator::Neq => actual != *expected,
                                Operator::Gt => actual > *expected,
                                Operator::Gte => actual >= *expected,
                                Operator::Lt => actual < *expected,
                                Operator::Lte => actual <= *expected,
                            }
                        } else {
                            false
                        }
                    }
                    ExpectedValue::I64(expected) => {
                        if let Ok(actual) = actual_arg.try_into_val(e) {
                            let actual: i64 = actual;
                            match cond.operator {
                                Operator::Eq => actual == *expected,
                                Operator::Neq => actual != *expected,
                                Operator::Gt => actual > *expected,
                                Operator::Gte => actual >= *expected,
                                Operator::Lt => actual < *expected,
                                Operator::Lte => actual <= *expected,
                            }
                        } else {
                            false
                        }
                    }
                    ExpectedValue::U128(expected) => {
                        if let Ok(actual) = actual_arg.try_into_val(e) {
                            let actual: u128 = actual;
                            match cond.operator {
                                Operator::Eq => actual == *expected,
                                Operator::Neq => actual != *expected,
                                Operator::Gt => actual > *expected,
                                Operator::Gte => actual >= *expected,
                                Operator::Lt => actual < *expected,
                                Operator::Lte => actual <= *expected,
                            }
                        } else {
                            false
                        }
                    }
                    ExpectedValue::I128(expected) => {
                        if let Ok(actual) = actual_arg.try_into_val(e) {
                            let actual: i128 = actual;
                            match cond.operator {
                                Operator::Eq => actual == *expected,
                                Operator::Neq => actual != *expected,
                                Operator::Gt => actual > *expected,
                                Operator::Gte => actual >= *expected,
                                Operator::Lt => actual < *expected,
                                Operator::Lte => actual <= *expected,
                            }
                        } else {
                            false
                        }
                    }
                    ExpectedValue::Sym(expected) => {
                        if let Ok(actual) = actual_arg.try_into_val(e) {
                            let actual: Symbol = actual;
                            match cond.operator {
                                Operator::Eq => actual == *expected,
                                Operator::Neq => actual != *expected,
                                _ => false,
                            }
                        } else {
                            false
                        }
                    }
                    ExpectedValue::Addr(expected) => {
                        if let Ok(actual) = actual_arg.try_into_val(e) {
                            let actual: Address = actual;
                            match cond.operator {
                                Operator::Eq => actual == *expected,
                                Operator::Neq => actual != *expected,
                                _ => false,
                            }
                        } else {
                            false
                        }
                    }
                };

                if !passed {
                    panic_with_error!(e, ParameterScopedError::ConditionFailed);
                }
            }
            emit_conditions_enforced(e, smart_account.clone(), context_rule.id, fn_name.clone());
        }
        _ => panic_with_error!(e, ParameterScopedError::MethodNotAllowed),
    }
}

pub fn install(
    e: &Env,
    params: &ParameterScopedAccountParams,
    context_rule: &ContextRule,
    smart_account: &Address,
) {
    smart_account.require_auth();

    if !matches!(context_rule.context_type, ContextRuleType::CallContract(_)) {
        panic_with_error!(e, ParameterScopedError::OnlyCallContractAllowed)
    }

    if params.conditions.is_empty() || params.conditions.len() > MAX_FNS {
        panic_with_error!(e, ParameterScopedError::InvalidConditions)
    }

    for (_, fn_conditions) in params.conditions.iter() {
        if fn_conditions.is_empty() || fn_conditions.len() > MAX_CONDITIONS_PER_FN {
            panic_with_error!(e, ParameterScopedError::InvalidConditions)
        }

        // Validate operator compatibility
        for cond in fn_conditions.iter() {
            match cond.expected_value {
                ExpectedValue::Sym(_) | ExpectedValue::Addr(_) => {
                    if cond.operator != Operator::Eq && cond.operator != Operator::Neq {
                        panic_with_error!(e, ParameterScopedError::InvalidConditions)
                    }
                }
                _ => {}
            }
        }
    }

    let key = ParameterScopedStorageKey::AccountContext(smart_account.clone(), context_rule.id);
    if e.storage().persistent().has(&key) {
        panic_with_error!(e, ParameterScopedError::AlreadyInstalled)
    }

    let data = ParameterScopedData {
        conditions: params.conditions.clone(),
    };
    e.storage().persistent().set(&key, &data);
    emit_conditions_installed(e, smart_account.clone(), context_rule.id);
}

pub fn uninstall(e: &Env, context_rule: &ContextRule, smart_account: &Address) {
    smart_account.require_auth();
    let key = ParameterScopedStorageKey::AccountContext(smart_account.clone(), context_rule.id);
    if !e.storage().persistent().has(&key) {
        panic_with_error!(e, ParameterScopedError::SmartAccountNotInstalled)
    }
    e.storage().persistent().remove(&key);
    emit_conditions_uninstalled(e, smart_account.clone(), context_rule.id);
}
