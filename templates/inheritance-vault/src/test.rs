#![cfg(test)]
extern crate std;

// Identity-sensitive tests ("rejected unless the designated party signs")
// build the authorization tree by hand with `mock_auths`, the same
// convention used elsewhere in this workspace (e.g. `fee-forwarder`) —
// `mock_all_auths()` approves every `require_auth()` call regardless of
// identity, so it can't express "someone other than `owner`/`beneficiary`
// tried this."
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal, MuxedAddress, String, Vec,
};
use stellar_tokens::fungible::{Base, FungibleToken};

use super::{InheritanceVault, InheritanceVaultClient, VaultData};

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn __constructor(e: &Env, to: Address) {
        Base::set_metadata(e, 7, String::from_str(e, "Mock Token"), String::from_str(e, "MOCK"));
        Base::mint(e, &to, 1_000_000_000);
    }
}

#[contractimpl(contracttrait)]
impl FungibleToken for MockToken {
    type ContractType = Base;
}

const DAY_IN_LEDGERS: u32 = 17280;

struct Setup<'a> {
    vault: InheritanceVaultClient<'a>,
    token: MockTokenClient<'a>,
    owner: Address,
    beneficiary: Address,
    inactivity_period_ledgers: u32,
}

/// Deploys a vault at a known, non-zero starting ledger — `Env::default()`
/// starts at sequence `0`, and starting there would make some
/// "before/after threshold" arithmetic in tests degenerate.
fn setup(e: &Env) -> Setup<'_> {
    e.ledger().set_sequence_number(1_000);

    let owner = Address::generate(e);
    let beneficiary = Address::generate(e);
    let inactivity_period_ledgers = 30 * DAY_IN_LEDGERS;

    let vault_id = e.register(
        InheritanceVault,
        (owner.clone(), beneficiary.clone(), inactivity_period_ledgers),
    );
    let token_id = e.register(MockToken, (owner.clone(),));

    Setup {
        vault: InheritanceVaultClient::new(e, &vault_id),
        token: MockTokenClient::new(e, &token_id),
        owner,
        beneficiary,
        inactivity_period_ledgers,
    }
}

fn fund_vault(s: &Setup, e: &Env, amount: i128) {
    s.token
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.token.address,
                fn_name: "transfer",
                args: (s.owner.clone(), s.vault.address.clone(), amount).into_val(e),
                sub_invokes: &[],
            },
        }])
        .transfer(&s.owner, &s.vault.address, &amount);
}

fn advance_past_threshold(s: &Setup, e: &Env) {
    let data = s.vault.get_vault_data();
    e.ledger().set_sequence_number(data.last_active_ledger + s.inactivity_period_ledgers);
}

// ── constructor ──────────────────────────────────────────────────────────

#[test]
fn constructor_seeds_state_from_deployment_ledger() {
    let e = Env::default();
    let s = setup(&e);

    let data = s.vault.get_vault_data();
    assert_eq!(
        data,
        VaultData {
            owner: s.owner.clone(),
            beneficiary: s.beneficiary.clone(),
            inactivity_period_ledgers: s.inactivity_period_ledgers,
            last_active_ledger: e.ledger().sequence(),
        }
    );
    assert!(!s.vault.is_claimable());
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")] // InvalidBeneficiary
fn constructor_rejects_beneficiary_equal_to_owner() {
    let e = Env::default();
    let owner = Address::generate(&e);

    e.register(InheritanceVault, (owner.clone(), owner, 100u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // InvalidInactivityPeriod
fn constructor_rejects_zero_inactivity_period() {
    let e = Env::default();
    let owner = Address::generate(&e);
    let beneficiary = Address::generate(&e);

    e.register(InheritanceVault, (owner, beneficiary, 0u32));
}

// ── check_in ─────────────────────────────────────────────────────────────

#[test]
fn check_in_resets_last_active_ledger() {
    let e = Env::default();
    let s = setup(&e);

    let deployed_at = s.vault.get_vault_data().last_active_ledger;
    e.ledger().set_sequence_number(deployed_at + DAY_IN_LEDGERS);

    s.vault
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "check_in",
                args: Vec::new(&e),
                sub_invokes: &[],
            },
        }])
        .check_in();

    assert_eq!(s.vault.get_vault_data().last_active_ledger, e.ledger().sequence());
    assert!(!s.vault.is_claimable());
}

#[test]
#[should_panic]
fn check_in_rejects_non_owner_auth() {
    let e = Env::default();
    let s = setup(&e);

    // `beneficiary` signs instead of `owner` — must not satisfy
    // `owner.require_auth()`.
    s.vault
        .mock_auths(&[MockAuth {
            address: &s.beneficiary,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "check_in",
                args: Vec::new(&e),
                sub_invokes: &[],
            },
        }])
        .check_in();
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")] // AlreadyClaimable
fn check_in_rejects_once_claimable() {
    let e = Env::default();
    let s = setup(&e);
    advance_past_threshold(&s, &e);

    // Even with valid `owner` authorization, a check-in submitted after the
    // threshold must not be able to reset the clock and cancel a pending
    // claim — see the module-level "no owner override" warning.
    s.vault
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "check_in",
                args: Vec::new(&e),
                sub_invokes: &[],
            },
        }])
        .check_in();
}

// ── claim ────────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #4)")] // NotYetClaimable
fn claim_rejects_before_threshold() {
    let e = Env::default();
    let s = setup(&e);
    fund_vault(&s, &e, 1_000);

    e.ledger().set_sequence_number(e.ledger().sequence() + s.inactivity_period_ledgers - 1);

    s.vault
        .mock_auths(&[MockAuth {
            address: &s.beneficiary,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "claim",
                args: (s.token.address.clone(), s.beneficiary.clone()).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .claim(&s.token.address, &s.beneficiary);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")] // NotYetClaimable
fn check_in_resets_clock_blocks_claim_shortly_after() {
    let e = Env::default();
    let s = setup(&e);
    fund_vault(&s, &e, 1_000);

    let deployed_at = s.vault.get_vault_data().last_active_ledger;

    // Check in just before the *original* deadline — still within the
    // window, so it succeeds and resets `last_active_ledger` to here.
    e.ledger().set_sequence_number(deployed_at + s.inactivity_period_ledgers - 1);
    s.vault
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "check_in",
                args: Vec::new(&e),
                sub_invokes: &[],
            },
        }])
        .check_in();

    // Advance only a little further — but far enough past the *original*
    // deployment ledger that judging claimability from deployment time
    // alone (instead of the reset `last_active_ledger`) would incorrectly
    // say this is claimable.
    e.ledger().set_sequence_number(e.ledger().sequence() + 10);
    assert!(e.ledger().sequence() > deployed_at + s.inactivity_period_ledgers);

    s.vault
        .mock_auths(&[MockAuth {
            address: &s.beneficiary,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "claim",
                args: (s.token.address.clone(), s.beneficiary.clone()).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .claim(&s.token.address, &s.beneficiary);
}

#[test]
fn claim_succeeds_after_threshold() {
    let e = Env::default();
    let s = setup(&e);
    fund_vault(&s, &e, 1_000);
    advance_past_threshold(&s, &e);

    let recipient = Address::generate(&e);
    let amount = s
        .vault
        .mock_auths(&[MockAuth {
            address: &s.beneficiary,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "claim",
                args: (s.token.address.clone(), recipient.clone()).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .claim(&s.token.address, &recipient);

    assert_eq!(amount, 1_000);
    assert_eq!(s.token.balance(&recipient), 1_000);
    assert_eq!(s.token.balance(&s.vault.address), 0);
}

#[test]
#[should_panic]
fn claim_rejects_non_beneficiary_auth() {
    let e = Env::default();
    let s = setup(&e);
    fund_vault(&s, &e, 1_000);
    advance_past_threshold(&s, &e);

    // `owner` signs instead of `beneficiary`.
    s.vault
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "claim",
                args: (s.token.address.clone(), s.owner.clone()).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .claim(&s.token.address, &s.owner);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")] // NoFundsToClaim
fn claim_rejects_when_vault_holds_no_balance() {
    let e = Env::default();
    let s = setup(&e);
    advance_past_threshold(&s, &e);

    s.vault
        .mock_auths(&[MockAuth {
            address: &s.beneficiary,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "claim",
                args: (s.token.address.clone(), s.beneficiary.clone()).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .claim(&s.token.address, &s.beneficiary);
}

// ── update_beneficiary ──────────────────────────────────────────────────

#[test]
fn update_beneficiary_succeeds_before_threshold() {
    let e = Env::default();
    let s = setup(&e);
    let new_beneficiary = Address::generate(&e);

    s.vault
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "update_beneficiary",
                args: (new_beneficiary.clone(),).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .update_beneficiary(&new_beneficiary);

    assert_eq!(s.vault.get_vault_data().beneficiary, new_beneficiary);
}

#[test]
#[should_panic]
fn update_beneficiary_rejects_non_owner_auth() {
    let e = Env::default();
    let s = setup(&e);
    let new_beneficiary = Address::generate(&e);

    s.vault
        .mock_auths(&[MockAuth {
            address: &s.beneficiary,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "update_beneficiary",
                args: (new_beneficiary.clone(),).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .update_beneficiary(&new_beneficiary);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")] // InvalidBeneficiary
fn update_beneficiary_rejects_owner_as_new_beneficiary() {
    let e = Env::default();
    let s = setup(&e);

    s.vault
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "update_beneficiary",
                args: (s.owner.clone(),).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .update_beneficiary(&s.owner);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")] // AlreadyClaimable
fn update_beneficiary_rejects_once_claimable() {
    let e = Env::default();
    let s = setup(&e);
    advance_past_threshold(&s, &e);

    let new_beneficiary = Address::generate(&e);
    s.vault
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "update_beneficiary",
                args: (new_beneficiary.clone(),).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .update_beneficiary(&new_beneficiary);
}

// ── extend_inactivity_period ────────────────────────────────────────────

#[test]
fn extend_inactivity_period_succeeds_before_threshold() {
    let e = Env::default();
    let s = setup(&e);

    let old_threshold_ledger =
        s.vault.get_vault_data().last_active_ledger + s.inactivity_period_ledgers;
    let new_period = s.inactivity_period_ledgers + DAY_IN_LEDGERS * 30;

    s.vault
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "extend_inactivity_period",
                args: (new_period,).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .extend_inactivity_period(&new_period);

    assert_eq!(s.vault.get_vault_data().inactivity_period_ledgers, new_period);

    // Past the *old* threshold, but the extended period pushes the real
    // threshold further out.
    e.ledger().set_sequence_number(old_threshold_ledger + 1);
    assert!(!s.vault.is_claimable());
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // InvalidInactivityPeriod
fn extend_inactivity_period_rejects_zero() {
    let e = Env::default();
    let s = setup(&e);

    s.vault
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "extend_inactivity_period",
                args: (0u32,).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .extend_inactivity_period(&0u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")] // AlreadyClaimable
fn extend_inactivity_period_rejects_once_claimable() {
    let e = Env::default();
    let s = setup(&e);
    advance_past_threshold(&s, &e);

    let new_period = s.inactivity_period_ledgers + DAY_IN_LEDGERS;
    s.vault
        .mock_auths(&[MockAuth {
            address: &s.owner,
            invoke: &MockAuthInvoke {
                contract: &s.vault.address,
                fn_name: "extend_inactivity_period",
                args: (new_period,).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .extend_inactivity_period(&new_period);
}

// ── is_claimable boundary ───────────────────────────────────────────────

#[test]
fn is_claimable_true_exactly_at_threshold() {
    let e = Env::default();
    let s = setup(&e);

    let data = s.vault.get_vault_data();
    e.ledger().set_sequence_number(data.last_active_ledger + data.inactivity_period_ledgers);

    assert!(s.vault.is_claimable());
}

#[test]
fn is_claimable_false_one_ledger_before_threshold() {
    let e = Env::default();
    let s = setup(&e);

    let data = s.vault.get_vault_data();
    e.ledger().set_sequence_number(data.last_active_ledger + data.inactivity_period_ledgers - 1);

    assert!(!s.vault.is_claimable());
}
