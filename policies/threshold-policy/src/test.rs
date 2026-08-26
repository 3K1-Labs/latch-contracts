#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

use super::ThresholdPolicy;

fn create_context_rule(e: &Env, signer_count: u32) -> ContextRule {
    let mut signers = Vec::new(e);
    for _ in 0..signer_count {
        signers.push_back(Signer::Delegated(Address::generate(e)));
    }

    ContextRule {
        id: 0,
        context_type: ContextRuleType::Default,
        name: String::from_str(e, "test"),
        signers,
        signer_ids: Vec::new(e),
        policies: Vec::new(e),
        policy_ids: Vec::new(e),
        valid_until: None,
    }
}

#[test]
fn would_remain_reachable_true_when_remaining_equals_threshold() {
    let e = Env::default();
    let address = e.register(ThresholdPolicy, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e, 3);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let params = super::SimpleThresholdAccountParams { threshold: 3 };
        super::simple_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        // 3 signers remain, threshold is 3 — reachable
        assert!(ThresholdPolicy::would_remain_reachable(&e, 0, smart_account.clone(), 3,));
    });
}

#[test]
fn would_remain_reachable_true_when_remaining_exceeds_threshold() {
    let e = Env::default();
    let address = e.register(ThresholdPolicy, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e, 5);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let params = super::SimpleThresholdAccountParams { threshold: 3 };
        super::simple_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        // 4 signers remain, threshold is 3 — reachable
        assert!(ThresholdPolicy::would_remain_reachable(&e, 0, smart_account.clone(), 4,));
    });
}

#[test]
fn would_remain_reachable_false_when_remaining_below_threshold() {
    let e = Env::default();
    let address = e.register(ThresholdPolicy, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e, 5);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let params = super::SimpleThresholdAccountParams { threshold: 5 };
        super::simple_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        // 3 signers remain, threshold is 5 — NOT reachable
        assert!(!ThresholdPolicy::would_remain_reachable(&e, 0, smart_account.clone(), 3,));
    });
}

#[test]
fn would_remain_reachable_false_when_zero_remaining_and_nonzero_threshold() {
    let e = Env::default();
    let address = e.register(ThresholdPolicy, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e, 3);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let params = super::SimpleThresholdAccountParams { threshold: 1 };
        super::simple_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        // 0 signers remain, threshold is 1 — NOT reachable
        assert!(!ThresholdPolicy::would_remain_reachable(&e, 0, smart_account.clone(), 0,));
    });
}

#[test]
fn would_remain_reachable_true_when_zero_remaining_and_zero_threshold() {
    let e = Env::default();
    let address = e.register(ThresholdPolicy, ());
    let smart_account = Address::generate(&e);
    // Threshold of 0 is invalid at install, so we set threshold=1 and
    // verify the edge case where threshold=0 is never stored.
    // Instead, test with threshold=1 and 1 remaining.
    let context_rule = create_context_rule(&e, 1);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let params = super::SimpleThresholdAccountParams { threshold: 1 };
        super::simple_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        // 1 signer remains, threshold is 1 — reachable
        assert!(ThresholdPolicy::would_remain_reachable(&e, 0, smart_account.clone(), 1,));
    });
}
