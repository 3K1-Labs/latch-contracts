extern crate std;

use soroban_sdk::{
    auth::{Context, ContractContext},
    contract, symbol_short,
    testutils::Address as _,
    vec, Address, Env, IntoVal, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

use crate::allowlist::*;

#[contract]
struct MockContract;

fn create_context_rule(e: &Env) -> ContextRule {
    let signer = Address::generate(e);
    let mut signers = Vec::new(e);
    signers.push_back(Signer::Delegated(signer));

    ContextRule {
        id: 1,
        context_type: ContextRuleType::CallContract(Address::generate(e)),
        name: soroban_sdk::String::from_str(e, "recipient-allowlist"),
        signers,
        signer_ids: Vec::new(e),
        policies: Vec::new(e),
        policy_ids: Vec::new(e),
        valid_until: None,
    }
}

fn transfer_context(e: &Env, from: &Address, to: &Address, amount: i128) -> Context {
    let mut args = Vec::new(e);
    args.push_back(from.into_val(e));
    args.push_back(to.into_val(e));
    args.push_back(amount.into_val(e));
    Context::Contract(ContractContext {
        contract: Address::generate(e),
        fn_name: symbol_short!("transfer"),
        args,
    })
}

#[test]
fn install_success() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let recipient = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let context_rule = create_context_rule(&e);
        let params = RecipientAllowlistAccountParams {
            allowed_recipients: vec![&e, recipient.clone()],
        };

        install(&e, &params, &context_rule, &smart_account);

        let allowed = get_allowed_recipients(&e, context_rule.id, &smart_account);
        assert_eq!(allowed.len(), 1);
        assert!(allowed.contains(&recipient));
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
        let params = RecipientAllowlistAccountParams {
            allowed_recipients: vec![&e, Address::generate(&e)],
        };
        install(&e, &params, &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn install_rejects_empty_recipients() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let context_rule = create_context_rule(&e);
        let params = RecipientAllowlistAccountParams {
            allowed_recipients: Vec::new(&e),
        };
        install(&e, &params, &context_rule, &smart_account);
    });
}

#[test]
fn enforce_accepts_allowlisted_recipient() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let from = Address::generate(&e);
    let to = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let context_rule = create_context_rule(&e);
        let params = RecipientAllowlistAccountParams {
            allowed_recipients: vec![&e, to.clone()],
        };
        install(&e, &params, &context_rule, &smart_account);

        let mut signers = Vec::new(&e);
        signers.push_back(Signer::Delegated(Address::generate(&e)));

        let ctx = transfer_context(&e, &from, &to, 100);
        enforce(&e, &ctx, &signers, &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn enforce_rejects_non_allowlisted_recipient() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let from = Address::generate(&e);
    let allowed = Address::generate(&e);
    let other = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let context_rule = create_context_rule(&e);
        let params = RecipientAllowlistAccountParams {
            allowed_recipients: vec![&e, allowed],
        };
        install(&e, &params, &context_rule, &smart_account);

        let mut signers = Vec::new(&e);
        signers.push_back(Signer::Delegated(Address::generate(&e)));

        let ctx = transfer_context(&e, &from, &other, 100);
        enforce(&e, &ctx, &signers, &context_rule, &smart_account);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn enforce_rejects_non_transfer_call() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let allowed = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let context_rule = create_context_rule(&e);
        let params = RecipientAllowlistAccountParams {
            allowed_recipients: vec![&e, allowed],
        };
        install(&e, &params, &context_rule, &smart_account);

        let mut signers = Vec::new(&e);
        signers.push_back(Signer::Delegated(Address::generate(&e)));

        let ctx = Context::Contract(ContractContext {
            contract: Address::generate(&e),
            fn_name: symbol_short!("approve"),
            args: Vec::new(&e),
        });
        enforce(&e, &ctx, &signers, &context_rule, &smart_account);
    });
}

#[test]
fn uninstall_clears_state() {
    let e = Env::default();
    let address = e.register(MockContract, ());
    let smart_account = Address::generate(&e);
    let recipient = Address::generate(&e);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let context_rule = create_context_rule(&e);
        let params = RecipientAllowlistAccountParams {
            allowed_recipients: vec![&e, recipient],
        };
        install(&e, &params, &context_rule, &smart_account);
        uninstall(&e, &context_rule, &smart_account);

        // Re-install should succeed after uninstall cleared storage.
        install(&e, &params, &context_rule, &smart_account);
        let allowed = get_allowed_recipients(&e, context_rule.id, &smart_account);
        assert_eq!(allowed.len(), 1);
    });
}
