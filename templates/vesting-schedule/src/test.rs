#![cfg(test)]

extern crate std;

use smart_account::{LatchSmartAccount, LatchSmartAccountClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    vec, Address, Env, Map, Val,
};
use stellar_accounts::smart_account::Signer;

use super::{compute_vested, ScheduleData, VestingScheduleContract, VestingScheduleContractClient};

#[contract]
struct MockTokenContract;

#[contractimpl]
impl MockTokenContract {
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let balance_from = Self::balance(e.clone(), from.clone());
        if balance_from < amount {
            panic!("insufficient balance");
        }
        e.storage().persistent().set(&from, &(balance_from - amount));
        let balance_to = Self::balance(e.clone(), to.clone());
        e.storage().persistent().set(&to, &(balance_to + amount));
    }

    pub fn mint(e: Env, to: Address, amount: i128) {
        let balance = Self::balance(e.clone(), to.clone());
        e.storage().persistent().set(&to, &(balance + amount));
    }

    pub fn balance(e: Env, account: Address) -> i128 {
        e.storage().persistent().get(&account).unwrap_or(0)
    }
}

struct TestFixture<'a> {
    env: Env,
    owner: Address,
    token: Address,
    token_client: MockTokenContractClient<'a>,
    vesting_id: Address,
    client: VestingScheduleContractClient<'a>,
}

fn create_fixture<'a>(
    start_ledger: u32,
    cliff_ledger: Option<u32>,
    end_ledger: u32,
    total_amount: i128,
) -> TestFixture<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let token_id = env.register(MockTokenContract, ());
    let token_client = MockTokenContractClient::new(&env, &token_id);

    let vesting_id = env.register(
        VestingScheduleContract,
        (owner.clone(), token_id.clone(), total_amount, start_ledger, cliff_ledger, end_ledger),
    );
    let client = VestingScheduleContractClient::new(&env, &vesting_id);

    // Fund the vesting contract with the total tokens to vest
    token_client.mint(&vesting_id, &total_amount);

    TestFixture { env, owner, token: token_id, token_client, vesting_id, client }
}

// ################## ACCEPTANCE CRITERIA TESTS ##################

#[test]
fn test_claim_before_start_ledger_releases_zero() {
    let fixture = create_fixture(100, None, 200, 1000);
    fixture.env.ledger().set_sequence_number(50);

    let beneficiary = Address::generate(&fixture.env);
    let released = fixture.client.claim(&beneficiary);

    assert_eq!(released, 0);
    assert_eq!(fixture.token_client.balance(&beneficiary), 0);
    assert_eq!(fixture.token_client.balance(&fixture.vesting_id), 1000);
    assert_eq!(fixture.client.claimed_amount(), 0);
    assert_eq!(fixture.client.claimable_amount(), 0);
    assert_eq!(fixture.client.vested_amount(), 0);
}

#[test]
fn test_claim_before_cliff_ledger_releases_zero() {
    let fixture = create_fixture(100, Some(150), 200, 1000);
    fixture.env.ledger().set_sequence_number(140);

    let beneficiary = Address::generate(&fixture.env);
    let released = fixture.client.claim(&beneficiary);

    assert_eq!(released, 0);
    assert_eq!(fixture.token_client.balance(&beneficiary), 0);
    assert_eq!(fixture.token_client.balance(&fixture.vesting_id), 1000);
    assert_eq!(fixture.client.claimed_amount(), 0);
    assert_eq!(fixture.client.claimable_amount(), 0);
    assert_eq!(fixture.client.vested_amount(), 0);
}

#[test]
fn test_claim_at_cliff_ledger_releases_vested_amount() {
    // 100 to 200 (duration 100), cliff at 150 (50% elapsed)
    let fixture = create_fixture(100, Some(150), 200, 1000);
    fixture.env.ledger().set_sequence_number(150);

    let beneficiary = Address::generate(&fixture.env);

    assert_eq!(fixture.client.vested_amount(), 500);
    assert_eq!(fixture.client.claimable_amount(), 500);

    let released = fixture.client.claim(&beneficiary);
    assert_eq!(released, 500);
    assert_eq!(fixture.token_client.balance(&beneficiary), 500);
    assert_eq!(fixture.token_client.balance(&fixture.vesting_id), 500);
    assert_eq!(fixture.client.claimed_amount(), 500);
    assert_eq!(fixture.client.claimable_amount(), 0);
}

#[test]
fn test_claim_at_various_points_along_linear_range() {
    // 100 to 200 with total 1000 stroops (10 stroops per ledger)
    let fixture = create_fixture(100, None, 200, 1000);
    let beneficiary = Address::generate(&fixture.env);

    // At ledger 125: 25% vested (250 stroops)
    fixture.env.ledger().set_sequence_number(125);
    assert_eq!(fixture.client.vested_amount(), 250);
    assert_eq!(fixture.client.claimable_amount(), 250);
    let released1 = fixture.client.claim(&beneficiary);
    assert_eq!(released1, 250);
    assert_eq!(fixture.token_client.balance(&beneficiary), 250);

    // At ledger 150: 50% vested (500 stroops total, 250 incremental)
    fixture.env.ledger().set_sequence_number(150);
    assert_eq!(fixture.client.vested_amount(), 500);
    assert_eq!(fixture.client.claimable_amount(), 250);
    let released2 = fixture.client.claim(&beneficiary);
    assert_eq!(released2, 250);
    assert_eq!(fixture.token_client.balance(&beneficiary), 500);

    // At ledger 175: 75% vested (750 stroops total, 250 incremental)
    fixture.env.ledger().set_sequence_number(175);
    assert_eq!(fixture.client.vested_amount(), 750);
    assert_eq!(fixture.client.claimable_amount(), 250);
    let released3 = fixture.client.claim(&beneficiary);
    assert_eq!(released3, 250);
    assert_eq!(fixture.token_client.balance(&beneficiary), 750);
}

#[test]
fn test_claim_at_and_after_end_ledger_releases_exact_remaining_balance() {
    let fixture = create_fixture(100, None, 200, 1000);
    let beneficiary = Address::generate(&fixture.env);

    // Claim partway through at ledger 140 (400 stroops)
    fixture.env.ledger().set_sequence_number(140);
    let released1 = fixture.client.claim(&beneficiary);
    assert_eq!(released1, 400);

    // Claim exactly at end_ledger (200) -> releases remaining 600
    fixture.env.ledger().set_sequence_number(200);
    assert_eq!(fixture.client.vested_amount(), 1000);
    assert_eq!(fixture.client.claimable_amount(), 600);
    let released2 = fixture.client.claim(&beneficiary);
    assert_eq!(released2, 600);
    assert_eq!(fixture.token_client.balance(&beneficiary), 1000);
    assert_eq!(fixture.token_client.balance(&fixture.vesting_id), 0);

    // Claim after end_ledger at ledger 300 -> releases 0
    fixture.env.ledger().set_sequence_number(300);
    assert_eq!(fixture.client.vested_amount(), 1000);
    assert_eq!(fixture.client.claimable_amount(), 0);
    let released3 = fixture.client.claim(&beneficiary);
    assert_eq!(released3, 0);
    assert_eq!(fixture.token_client.balance(&beneficiary), 1000);
    assert_eq!(fixture.token_client.balance(&fixture.vesting_id), 0);
}

#[test]
fn test_repeated_claim_calls_never_double_release() {
    let fixture = create_fixture(100, None, 200, 1000);
    let beneficiary = Address::generate(&fixture.env);

    fixture.env.ledger().set_sequence_number(150);

    // First claim at ledger 150
    let first = fixture.client.claim(&beneficiary);
    assert_eq!(first, 500);

    // Immediate second claim at same ledger 150
    let second = fixture.client.claim(&beneficiary);
    assert_eq!(second, 0);

    // Immediate third claim at same ledger 150
    let third = fixture.client.claim(&beneficiary);
    assert_eq!(third, 0);

    assert_eq!(fixture.token_client.balance(&beneficiary), 500);
    assert_eq!(fixture.client.claimed_amount(), 500);
}

#[test]
fn test_claim_from_non_owner_is_rejected() {
    let env = Env::default();
    // Do NOT mock all auths unconditionally for this test
    let owner = Address::generate(&env);
    let token_id = env.register(MockTokenContract, ());
    let token_client = MockTokenContractClient::new(&env, &token_id);

    let vesting_id = env.register(
        VestingScheduleContract,
        (owner.clone(), token_id.clone(), 1000i128, 100u32, None::<u32>, 200u32),
    );
    let client = VestingScheduleContractClient::new(&env, &vesting_id);
    token_client.mint(&vesting_id, &1000);

    env.ledger().set_sequence_number(150);

    let beneficiary = Address::generate(&env);

    // Calling claim without mocking or providing owner auth fails auth check
    let res = client.try_claim(&beneficiary);
    assert!(res.is_err());
}

// ################## ROUNDING & PRECISION TESTS ##################

#[test]
fn test_rounding_precision_no_dust_left_behind() {
    // 100 stroops over 3 ledgers: (ledger 0 -> 3)
    // Ledger 1: 100 * 1 / 3 = 33
    // Ledger 2: 100 * 2 / 3 = 66
    // Ledger 3: 100 * 3 / 3 = 100
    let fixture = create_fixture(0, None, 3, 100);
    let beneficiary = Address::generate(&fixture.env);

    fixture.env.ledger().set_sequence_number(1);
    let rel1 = fixture.client.claim(&beneficiary);
    assert_eq!(rel1, 33);

    fixture.env.ledger().set_sequence_number(2);
    let rel2 = fixture.client.claim(&beneficiary);
    assert_eq!(rel2, 33); // 66 - 33 = 33

    fixture.env.ledger().set_sequence_number(3);
    let rel3 = fixture.client.claim(&beneficiary);
    assert_eq!(rel3, 34); // 100 - 66 = 34 (remaining balance exactly released)

    assert_eq!(rel1 + rel2 + rel3, 100);
    assert_eq!(fixture.token_client.balance(&beneficiary), 100);
    assert_eq!(fixture.token_client.balance(&fixture.vesting_id), 0);
}

#[test]
fn test_rounding_precision_prime_intervals() {
    // 10,000 stroops over 7 ledgers (start: 10, end: 17)
    let fixture = create_fixture(10, None, 17, 10_000);
    let beneficiary = Address::generate(&fixture.env);

    let mut total_released = 0i128;
    for l in 11..=17 {
        fixture.env.ledger().set_sequence_number(l);
        let rel = fixture.client.claim(&beneficiary);
        total_released += rel;
    }

    assert_eq!(total_released, 10_000);
    assert_eq!(fixture.token_client.balance(&beneficiary), 10_000);
    assert_eq!(fixture.token_client.balance(&fixture.vesting_id), 0);
}

#[test]
fn test_pure_math_wide_multiplication_large_values() {
    // Test large i128 values close to i128::MAX
    let large_amount = 100_000_000_000_000_000_000_000_000_000_000_000i128; // 10^35
    let start = 1_000_000u32;
    let end = 2_000_000u32; // duration = 1_000_000

    // Midpoint: exactly 50%
    let vested_mid = compute_vested(large_amount, start, None, end, 1_500_000);
    assert_eq!(vested_mid, 50_000_000_000_000_000_000_000_000_000_000_000i128);

    // Max i128 / 2
    let max_val = i128::MAX;
    let vested_half = compute_vested(max_val, 0, None, 2, 1);
    assert_eq!(vested_half, max_val / 2);

    // Boundary at end
    let vested_end = compute_vested(max_val, 0, None, 100, 100);
    assert_eq!(vested_end, max_val);
}

// ################## CONSTRUCTOR VALIDATIONS ##################

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_constructor_rejects_zero_amount() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let token = Address::generate(&env);

    env.register(VestingScheduleContract, (owner, token, 0i128, 100u32, None::<u32>, 200u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_constructor_rejects_negative_amount() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let token = Address::generate(&env);

    env.register(VestingScheduleContract, (owner, token, -500i128, 100u32, None::<u32>, 200u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_constructor_rejects_start_equal_end() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let token = Address::generate(&env);

    env.register(VestingScheduleContract, (owner, token, 1000i128, 100u32, None::<u32>, 100u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_constructor_rejects_start_greater_than_end() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let token = Address::generate(&env);

    env.register(VestingScheduleContract, (owner, token, 1000i128, 200u32, None::<u32>, 100u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_constructor_rejects_cliff_before_start() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let token = Address::generate(&env);

    env.register(VestingScheduleContract, (owner, token, 1000i128, 100u32, Some(90u32), 200u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_constructor_rejects_cliff_after_end() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let token = Address::generate(&env);

    env.register(VestingScheduleContract, (owner, token, 1000i128, 100u32, Some(210u32), 200u32));
}

// Note: The AlreadyInitialized (error #1) guard is exercised at the Rust level in
// `__constructor` directly. In the Soroban test framework, constructors are invoked
// exactly once during `env.register()` and cannot be re-invoked via the generated
// client — so there is no test-framework path to exercise this branch at the contract
// level. The guard exists for forward-compatibility with any future re-initialization
// vector and is verified by code inspection.

// ################## QUERY METHODS ##################

#[test]
fn test_getters_and_queries() {
    let fixture = create_fixture(100, Some(150), 200, 1000);
    assert_eq!(fixture.client.owner(), fixture.owner);
    assert_eq!(fixture.client.token(), fixture.token);

    let schedule = fixture.client.get_schedule();
    assert_eq!(
        schedule,
        ScheduleData {
            owner: fixture.owner,
            token: fixture.token,
            total_amount: 1000,
            start_ledger: 100,
            cliff_ledger: Some(150),
            end_ledger: 200,
            claimed_amount: 0,
        }
    );
}

// ################## END-TO-END INTEGRATION WITH SMART ACCOUNT ##################

#[test]
fn test_end_to_end_deployment_and_claim_via_smart_account() {
    let env = Env::default();
    env.mock_all_auths();

    // 1. Deploy and initialize real LatchSmartAccount
    let signers = vec![&env, Signer::Delegated(Address::generate(&env))];
    let policies: Map<Address, Val> = Map::new(&env);
    let account_id = env.register(LatchSmartAccount, (signers.clone(), policies.clone()));
    let _account_client = LatchSmartAccountClient::new(&env, &account_id);

    // 2. Deploy MockToken
    let token_id = env.register(MockTokenContract, ());
    let token_client = MockTokenContractClient::new(&env, &token_id);

    // 3. Deploy VestingSchedule with LatchSmartAccount as the owner
    let total_amount = 5_000i128;
    let start_ledger = 100u32;
    let end_ledger = 600u32;
    let vesting_id = env.register(
        VestingScheduleContract,
        (account_id.clone(), token_id.clone(), total_amount, start_ledger, None::<u32>, end_ledger),
    );
    let vesting_client = VestingScheduleContractClient::new(&env, &vesting_id);

    // 4. Fund the vesting schedule contract
    token_client.mint(&vesting_id, &total_amount);
    assert_eq!(token_client.balance(&vesting_id), 5_000);

    // 5. Advance ledger to 350 (50% through: (350 - 100) / (600 - 100) = 250 / 500 = 50%)
    env.ledger().set_sequence_number(350);

    let beneficiary = Address::generate(&env);

    // 6. Claim vested funds for beneficiary
    let released = vesting_client.claim(&beneficiary);
    assert_eq!(released, 2_500);
    assert_eq!(token_client.balance(&beneficiary), 2_500);
    assert_eq!(token_client.balance(&vesting_id), 2_500);

    // 7. Advance ledger past end to 700 and claim remaining
    env.ledger().set_sequence_number(700);
    let released_final = vesting_client.claim(&beneficiary);
    assert_eq!(released_final, 2_500);
    assert_eq!(token_client.balance(&beneficiary), 5_000);
    assert_eq!(token_client.balance(&vesting_id), 0);
}
