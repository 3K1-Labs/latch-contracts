//! # Parameter Scoped Policy Module
//!
//! This policy restricts a context rule's signers to specific parameter values
//! when invoking a contract function. Each [`Condition`] is `(arg_index,
//! operator, expected_value)` — it pulls the real argument at `arg_index` out
//! of the actual call and compares it against `expected_value`. Conditions
//! are configured per function name; calling a function with no configured
//! conditions is rejected outright, not silently allowed through — so
//! installing this policy also acts as its own function allowlist, on top of
//! whatever `session-policy` separately allows for the same context rule.
//!
//! ## Why this is useful: two motivating examples
//!
//! - **Spender pinning.** SEP-41's `approve(from, spender, amount,
//!   expiration_ledger)` lets a session key hand another contract permission to
//!   move tokens on the account's behalf. A compromised or misbehaving session
//!   key with unrestricted `approve` access could approve an
//!   attacker-controlled address for the full balance. Pinning `spender` —
//!   `Condition { arg_index: 1, operator: Eq, expected_value: Addr(known_dex)
//!   }` — means the account will only ever authorize an `approve` naming that
//!   one trusted address, no matter what the session key is told to do.
//! - **Amount floor scoping.** A DEX `swap(..., min_amount_out, ...)` call's
//!   `min_amount_out` is slippage protection — set it too low (or to zero) and
//!   the swap is exposed to a sandwich attack. Scoping it — `Condition {
//!   arg_index: N, operator: Gte, expected_value: I128(floor) }` — means the
//!   account refuses to authorize a swap whose stated minimum return falls
//!   below a floor fixed at install time, regardless of what a buggy or
//!   malicious caller tries to submit.

use soroban_sdk::{
    auth::{Context, ContractContext},
    contracterror, contractevent, contracttype, panic_with_error, Address, Env, Map, Symbol,
    TryIntoVal, Val, Vec,
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

pub fn emit_conditions_enforced(
    e: &Env,
    smart_account: Address,
    context_rule_id: u32,
    fn_name: Symbol,
) {
    ConditionsEnforced { smart_account, context_rule_id, fn_name }.publish(e);
}

pub fn emit_conditions_installed(e: &Env, smart_account: Address, context_rule_id: u32) {
    ConditionsInstalled { smart_account, context_rule_id }.publish(e);
}

pub fn emit_conditions_uninstalled(e: &Env, smart_account: Address, context_rule_id: u32) {
    ConditionsUninstalled { smart_account, context_rule_id }.publish(e);
}

pub fn get_conditions(
    e: &Env,
    context_rule_id: u32,
    smart_account: &Address,
) -> Map<Symbol, Vec<Condition>> {
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

/// Converts `actual_arg` to `T` and evaluates `operator` against `expected`.
/// Covers all six ordered numeric [`ExpectedValue`] variants — `PartialOrd`'s
/// supertrait bound on `PartialEq` gives us `Eq`/`Neq` for free alongside the
/// ordering comparisons. Returns `false` (rather than propagating an error)
/// when `actual_arg` isn't actually a `T` — a type mismatch fails the
/// condition instead of panicking.
fn compare_ord<T>(e: &Env, actual_arg: &Val, expected: &T, operator: &Operator) -> bool
where
    T: PartialOrd,
    Val: TryIntoVal<Env, T>,
{
    let Ok(actual) = actual_arg.try_into_val(e) else {
        return false;
    };
    let actual: T = actual;
    match operator {
        Operator::Eq => actual == *expected,
        Operator::Neq => actual != *expected,
        Operator::Gt => actual > *expected,
        Operator::Gte => actual >= *expected,
        Operator::Lt => actual < *expected,
        Operator::Lte => actual <= *expected,
    }
}

/// Same conversion/mismatch handling as [`compare_ord`], for the
/// [`ExpectedValue`] variants (`Sym`, `Addr`) that only support equality —
/// `install` already rejects any other operator paired with these, but
/// `Gt`/`Gte`/`Lt`/`Lte` still need a defined (rejecting) fallback here.
fn compare_eq<T>(e: &Env, actual_arg: &Val, expected: &T, operator: &Operator) -> bool
where
    T: PartialEq,
    Val: TryIntoVal<Env, T>,
{
    let Ok(actual) = actual_arg.try_into_val(e) else {
        return false;
    };
    let actual: T = actual;
    match operator {
        Operator::Eq => actual == *expected,
        Operator::Neq => actual != *expected,
        _ => false,
    }
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
                let actual_arg = args.get(cond.arg_index).unwrap_or_else(|| {
                    panic_with_error!(e, ParameterScopedError::ArgumentIndexOutOfBounds)
                });

                let passed = match &cond.expected_value {
                    ExpectedValue::U32(expected) => {
                        compare_ord(e, &actual_arg, expected, &cond.operator)
                    }
                    ExpectedValue::I32(expected) => {
                        compare_ord(e, &actual_arg, expected, &cond.operator)
                    }
                    ExpectedValue::U64(expected) => {
                        compare_ord(e, &actual_arg, expected, &cond.operator)
                    }
                    ExpectedValue::I64(expected) => {
                        compare_ord(e, &actual_arg, expected, &cond.operator)
                    }
                    ExpectedValue::U128(expected) => {
                        compare_ord(e, &actual_arg, expected, &cond.operator)
                    }
                    ExpectedValue::I128(expected) => {
                        compare_ord(e, &actual_arg, expected, &cond.operator)
                    }
                    ExpectedValue::Sym(expected) => {
                        compare_eq(e, &actual_arg, expected, &cond.operator)
                    }
                    ExpectedValue::Addr(expected) => {
                        compare_eq(e, &actual_arg, expected, &cond.operator)
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
                ExpectedValue::Sym(_) | ExpectedValue::Addr(_)
                    if cond.operator != Operator::Eq && cond.operator != Operator::Neq =>
                {
                    panic_with_error!(e, ParameterScopedError::InvalidConditions)
                }
                _ => {}
            }
        }
    }

    let key = ParameterScopedStorageKey::AccountContext(smart_account.clone(), context_rule.id);
    if e.storage().persistent().has(&key) {
        panic_with_error!(e, ParameterScopedError::AlreadyInstalled)
    }

    let data = ParameterScopedData { conditions: params.conditions.clone() };
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
