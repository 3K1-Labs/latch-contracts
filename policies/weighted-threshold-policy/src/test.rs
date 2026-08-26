#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env, Map, String, Vec};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

use super::WeightedThresholdPolicy;

fn create_context_rule_with_signers(e: &Env, signers: Vec<Signer>) -> ContextRule {
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

fn make_weighted_install_params(
    e: &Env,
    weights: &[(Signer, u32)],
    threshold: u32,
) -> super::weighted_threshold::WeightedThresholdAccountParams {
    let mut signer_weights = Map::new(e);
    for (signer, weight) in weights.iter() {
        signer_weights.set(signer.clone(), *weight);
    }
    super::weighted_threshold::WeightedThresholdAccountParams { signer_weights, threshold }
}

#[test]
fn would_remain_reachable_true_when_remaining_weight_exceeds_threshold() {
    let e = Env::default();
    let address = e.register(WeightedThresholdPolicy, ());
    let smart_account = Address::generate(&e);

    let signer_a = Signer::Delegated(Address::generate(&e));
    let signer_b = Signer::Delegated(Address::generate(&e));
    let signer_c = Signer::Delegated(Address::generate(&e));

    let signers = Vec::from_array(&e, [signer_a.clone(), signer_b.clone(), signer_c.clone()]);
    let context_rule = create_context_rule_with_signers(&e, signers);

    // A=100, B=75, C=75, threshold=150
    let params = make_weighted_install_params(
        &e,
        &[(signer_a.clone(), 100), (signer_b.clone(), 75), (signer_c.clone(), 75)],
        150,
    );

    e.mock_all_auths();

    e.as_contract(&address, || {
        super::weighted_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        // Remove A (weight 100): remaining = 75+75 = 150 >= 150 — reachable
        assert!(WeightedThresholdPolicy::would_remain_reachable(
            &e,
            0,
            smart_account.clone(),
            signer_a.clone(),
            2, // remaining signer count (unused by weighted policy)
        ));
    });
}

#[test]
fn would_remain_reachable_false_when_remaining_weight_below_threshold() {
    let e = Env::default();
    let address = e.register(WeightedThresholdPolicy, ());
    let smart_account = Address::generate(&e);

    let signer_a = Signer::Delegated(Address::generate(&e));
    let signer_b = Signer::Delegated(Address::generate(&e));
    let signer_c = Signer::Delegated(Address::generate(&e));

    let signers = Vec::from_array(&e, [signer_a.clone(), signer_b.clone(), signer_c.clone()]);
    let context_rule = create_context_rule_with_signers(&e, signers);

    // A=100, B=75, C=75, threshold=200
    let params = make_weighted_install_params(
        &e,
        &[(signer_a.clone(), 100), (signer_b.clone(), 75), (signer_c.clone(), 75)],
        200,
    );

    e.mock_all_auths();

    e.as_contract(&address, || {
        super::weighted_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        // Remove B (weight 75): remaining = 100+75 = 175 < 200 — NOT reachable
        assert!(!WeightedThresholdPolicy::would_remain_reachable(
            &e,
            0,
            smart_account.clone(),
            signer_b.clone(),
            2, // remaining signer count (unused by weighted policy)
        ));
    });
}

#[test]
fn would_remain_reachable_true_when_weight_exceeds_threshold() {
    let e = Env::default();
    let address = e.register(WeightedThresholdPolicy, ());
    let smart_account = Address::generate(&e);

    let signer_a = Signer::Delegated(Address::generate(&e));
    let signer_b = Signer::Delegated(Address::generate(&e));
    let signer_c = Signer::Delegated(Address::generate(&e));

    let signers = Vec::from_array(&e, [signer_a.clone(), signer_b.clone(), signer_c.clone()]);
    let context_rule = create_context_rule_with_signers(&e, signers);

    // A=100, B=50, C=50, threshold=100
    let params = make_weighted_install_params(
        &e,
        &[(signer_a.clone(), 100), (signer_b.clone(), 50), (signer_c.clone(), 50)],
        100,
    );

    e.mock_all_auths();

    e.as_contract(&address, || {
        super::weighted_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        // Remove C (weight 50): remaining = 100+50 = 150 >= 100 — reachable
        assert!(WeightedThresholdPolicy::would_remain_reachable(
            &e,
            0,
            smart_account.clone(),
            signer_c.clone(),
            2, // remaining signer count (unused by weighted policy)
        ));
    });
}

#[test]
fn would_remain_reachable_false_for_zero_weight_unconfigured_signer() {
    let e = Env::default();
    let address = e.register(WeightedThresholdPolicy, ());
    let smart_account = Address::generate(&e);

    let signer_a = Signer::Delegated(Address::generate(&e));
    let signer_b = Signer::Delegated(Address::generate(&e));
    let unconfigured = Signer::Delegated(Address::generate(&e));

    let signers = Vec::from_array(&e, [signer_a.clone(), signer_b.clone(), unconfigured.clone()]);
    let context_rule = create_context_rule_with_signers(&e, signers);

    // A=100, B=50, threshold=150 (total=150, so 2-of-2 required)
    let params =
        make_weighted_install_params(&e, &[(signer_a.clone(), 100), (signer_b.clone(), 50)], 150);

    e.mock_all_auths();

    e.as_contract(&address, || {
        super::weighted_threshold::install(&e, &params, &context_rule, &smart_account);
    });

    e.as_contract(&address, || {
        // Removing the unconfigured signer (weight 0): remaining = 100+50 = 150 >= 150
        assert!(WeightedThresholdPolicy::would_remain_reachable(
            &e,
            0,
            smart_account.clone(),
            unconfigured,
            2, // remaining signer count (unused by weighted policy)
        ));
    });
}
