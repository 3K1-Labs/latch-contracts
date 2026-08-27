#![cfg(test)]
extern crate std;

use soroban_sdk::{
    auth::{Context, ContractContext},
    testutils::{Address as _, BytesN as _},
    vec, Address, BytesN, Env, IntoVal, Symbol, Val, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

use crate::{
    RecipientAccountParams, RecipientAllowlistPolicy, RecipientAllowlistPolicyClient,
    MAX_ALLOWED_RECIPIENTS,
};

fn create_context_rule(e: &Env) -> ContextRule {
    let signer = Address::generate(e);
    let mut signers = Vec::new(e);
    signers.push_back(Signer::Delegated(signer));

    ContextRule {
        id: 1,
        context_type: ContextRuleType::CallContract(Address::generate(e)),
        name: soroban_sdk::String::from_str(e, "session"),
        signers,
        signer_ids: Vec::new(e),
        policies: Vec::new(e),
        policy_ids: Vec::new(e),
        valid_until: None,
    }
}

fn setup_env<'a>() -> (Env, Address, RecipientAllowlistPolicyClient<'a>) {
    let e = Env::default();
    e.mock_all_auths();

    let smart_account = Address::generate(&e);
    let contract_id = e.register(RecipientAllowlistPolicy, ());
    let client = RecipientAllowlistPolicyClient::new(&e, &contract_id);

    (e, smart_account, client)
}

#[test]
fn test_install_and_get() {
    let (e, smart_account, client) = setup_env();

    let rule = create_context_rule(&e);

    let recipient1 = Address::generate(&e);
    let recipient2 = Address::generate(&e);
    let allowed_recipients = vec![&e, recipient1.clone(), recipient2.clone()];

    client.install(
        &RecipientAccountParams { allowed_recipients: allowed_recipients.clone() },
        &rule,
        &smart_account,
    );

    assert_eq!(client.get_allowed_recipients(&rule.id, &smart_account), allowed_recipients);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // OnlyCallContractAllowed
fn test_install_rejects_non_call_contract() {
    let (e, smart_account, client) = setup_env();

    let mut rule = create_context_rule(&e);
    rule.context_type = ContextRuleType::Default;

    let allowed_recipients = vec![&e, Address::generate(&e)];

    client.install(&RecipientAccountParams { allowed_recipients }, &rule, &smart_account);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")] // InvalidAllowedRecipients
fn test_install_rejects_empty_allowlist() {
    let (e, smart_account, client) = setup_env();

    let rule = create_context_rule(&e);

    client.install(&RecipientAccountParams { allowed_recipients: vec![&e] }, &rule, &smart_account);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")] // InvalidAllowedRecipients
fn test_install_rejects_too_large_allowlist() {
    let (e, smart_account, client) = setup_env();

    let rule = create_context_rule(&e);

    let mut allowed_recipients = vec![&e];
    for _ in 0..(MAX_ALLOWED_RECIPIENTS + 1) {
        allowed_recipients.push_back(Address::generate(&e));
    }

    client.install(&RecipientAccountParams { allowed_recipients }, &rule, &smart_account);
}

#[test]
fn test_enforce_accepts_allowlisted() {
    let (e, smart_account, client) = setup_env();

    let contract_target = Address::generate(&e);
    let mut rule = create_context_rule(&e);
    rule.context_type = ContextRuleType::CallContract(contract_target.clone());

    let recipient = Address::generate(&e);
    let allowed_recipients = vec![&e, recipient.clone()];

    client.install(&RecipientAccountParams { allowed_recipients }, &rule, &smart_account);

    let args: Vec<Val> =
        vec![&e, smart_account.into_val(&e), recipient.into_val(&e), 100i128.into_val(&e)];
    let context = Context::Contract(ContractContext {
        contract: contract_target,
        fn_name: Symbol::new(&e, "transfer"),
        args,
    });

    client.enforce(&context, &rule.signers, &rule, &smart_account);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")] // RecipientNotAllowed
fn test_enforce_rejects_non_allowlisted() {
    let (e, smart_account, client) = setup_env();

    let contract_target = Address::generate(&e);
    let mut rule = create_context_rule(&e);
    rule.context_type = ContextRuleType::CallContract(contract_target.clone());

    let allowed_recipients = vec![&e, Address::generate(&e)];
    client.install(&RecipientAccountParams { allowed_recipients }, &rule, &smart_account);

    let unapproved_recipient = Address::generate(&e);
    let args: Vec<Val> = vec![
        &e,
        smart_account.into_val(&e),
        unapproved_recipient.into_val(&e),
        100i128.into_val(&e),
    ];
    let context = Context::Contract(ContractContext {
        contract: contract_target,
        fn_name: Symbol::new(&e, "transfer"),
        args,
    });

    client.enforce(&context, &rule.signers, &rule, &smart_account);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")] // RecipientNotAllowed
fn test_enforce_rejects_non_transfer() {
    let (e, smart_account, client) = setup_env();

    let contract_target = Address::generate(&e);
    let mut rule = create_context_rule(&e);
    rule.context_type = ContextRuleType::CallContract(contract_target.clone());

    let recipient = Address::generate(&e);
    let allowed_recipients = vec![&e, recipient.clone()];
    client.install(&RecipientAccountParams { allowed_recipients }, &rule, &smart_account);

    let args: Vec<Val> =
        vec![&e, smart_account.into_val(&e), recipient.into_val(&e), 100i128.into_val(&e)];
    let context = Context::Contract(ContractContext {
        contract: contract_target,
        fn_name: Symbol::new(&e, "mint"),
        args,
    });

    client.enforce(&context, &rule.signers, &rule, &smart_account);
}

#[test]
fn test_uninstall_clears_state() {
    let (e, smart_account, client) = setup_env();

    let rule = create_context_rule(&e);

    let allowed_recipients = vec![&e, Address::generate(&e)];
    client.install(&RecipientAccountParams { allowed_recipients }, &rule, &smart_account);

    client.uninstall(&rule, &smart_account);

    let res = client.try_get_allowed_recipients(&rule.id, &smart_account);
    assert!(res.is_err());
}
