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

    // The context rule has 3 signers. Removing any one leaves 2 < 3 → NOT
    // reachable. But threshold=3 with 3 signers → removing one → 2 remaining,
    // so let's use threshold=2 with 3 signers: removing one → 2 remaining ≥ 2 →
    // reachable.
    let e2 = Env::default();
    let address2 = e2.register(ThresholdPolicy, ());
    let smart_account2 = Address::generate(&e2);
    let context_rule2 = create_context_rule(&e2, 3);
    e2.mock_all_auths();
    e2.as_contract(&address2, || {
        let params = super::SimpleThresholdAccountParams { threshold: 2 };
        super::simple_threshold::install(&e2, &params, &context_rule2, &smart_account2);
    });

    let signer_to_remove = context_rule.signers.get_unchecked(0);
    e.as_contract(&address, || {
        // 3 signers in rule, threshold=3. Removing one → 2 remaining < 3 → NOT
        // reachable.
        assert!(!ThresholdPolicy::would_remain_reachable(
            &e,
            0,
            smart_account.clone(),
            signer_to_remove.clone(),
            2, // remaining_count after removing one from 3
        ));
    });

    let signer_to_remove2 = context_rule2.signers.get_unchecked(0);
    e2.as_contract(&address2, || {
        // 3 signers in rule, threshold=2. Removing one → 2 remaining ≥ 2 → reachable.
        assert!(ThresholdPolicy::would_remain_reachable(
            &e2,
            0,
            smart_account2.clone(),
            signer_to_remove2.clone(),
            2, // remaining_count after removing one from 3
        ));
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

    let signer_to_remove = context_rule.signers.get_unchecked(0);
    e.as_contract(&address, || {
        // 5 signers, threshold=3. Removing one → 4 remaining ≥ 3 → reachable.
        assert!(ThresholdPolicy::would_remain_reachable(
            &e,
            0,
            smart_account.clone(),
            signer_to_remove.clone(),
            4, // remaining_count after removing one from 5
        ));
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

    let signer_to_remove = context_rule.signers.get_unchecked(0);
    e.as_contract(&address, || {
        // 5 signers, threshold=5. Removing one → 4 remaining < 5 → NOT reachable.
        assert!(!ThresholdPolicy::would_remain_reachable(
            &e,
            0,
            smart_account.clone(),
            signer_to_remove.clone(),
            4, // remaining_count after removing one from 5
        ));
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
        let params = super::SimpleThresholdAccountParams { threshold: 3 };
        super::simple_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    let signer_to_remove = context_rule.signers.get_unchecked(0);
    e.as_contract(&address, || {
        // 3 signers, threshold=3. Removing one → 2 remaining < 3 → NOT reachable.
        assert!(!ThresholdPolicy::would_remain_reachable(
            &e,
            0,
            smart_account.clone(),
            signer_to_remove.clone(),
            2, // remaining_count after removing one from 3
        ));
    });
}

#[test]
fn would_remain_reachable_true_when_one_signer_and_threshold_one() {
    let e = Env::default();
    let address = e.register(ThresholdPolicy, ());
    let smart_account = Address::generate(&e);
    let context_rule = create_context_rule(&e, 1);

    e.mock_all_auths();

    e.as_contract(&address, || {
        let params = super::SimpleThresholdAccountParams { threshold: 1 };
        super::simple_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    let signer_to_remove = context_rule.signers.get_unchecked(0);
    e.as_contract(&address, || {
        // 1 signer, threshold=1. Removing the only signer → 0 remaining < 1 → NOT
        // reachable.
        assert!(!ThresholdPolicy::would_remain_reachable(
            &e,
            0,
            smart_account.clone(),
            signer_to_remove.clone(),
            0, // remaining_count after removing the only signer
        ));
    });
}
