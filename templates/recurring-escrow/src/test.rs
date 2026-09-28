#![cfg(test)]
extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger, MockAuth, MockAuthInvoke},
    token, Address, Env, IntoVal,
};

use crate::{RecurringEscrow, RecurringEscrowClient};

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

const AMOUNT_PER_PERIOD: i128 = 1_000_000;
const PERIOD_LEDGERS: u32 = 100;
const MINT_AMOUNT: i128 = 10_000_000;

fn setup_env<'a>() -> (Env, Address, Address, RecurringEscrowClient<'a>, Address, token::StellarAssetClient<'a>)
{
    let e = Env::default();
    e.mock_all_auths();

    e.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });

    let owner = Address::generate(&e);
    let payee = Address::generate(&e);

    let escrow_id = e.register(
        RecurringEscrow,
        (&owner, &payee, AMOUNT_PER_PERIOD, PERIOD_LEDGERS, Address::generate(&e)),
    );

    // Register a real SAC token and re-deploy with the real token address.
    let admin = Address::generate(&e);
    let token_id = e.register_stellar_asset_contract_v2(admin.clone());
    let sac_client = token::StellarAssetClient::new(&e, &token_id.address());
    sac_client.mint(&owner, &MINT_AMOUNT);

    // Re-deploy with the correct token address.
    let escrow_id = e.register(
        RecurringEscrow,
        (&owner, &payee, AMOUNT_PER_PERIOD, PERIOD_LEDGERS, &token_id.address()),
    );
    let client = RecurringEscrowClient::new(&e, &escrow_id);

    (e, owner, payee, client, token_id.address(), sac_client)
}

// ────────────────────────────────────────────────────────────────────────────
// Constructor tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn constructor_stores_config() {
    let (e, owner, payee, client, token_addr, _) = setup_env();

    assert_eq!(client.get_owner(), owner);
    assert_eq!(client.get_payee(), payee);
    assert_eq!(client.get_token(), token_addr);
    assert_eq!(client.get_amount_per_period(), AMOUNT_PER_PERIOD);
    assert_eq!(client.get_period_ledgers(), PERIOD_LEDGERS);
    assert_eq!(client.get_last_pull_ledger(), 0);
    assert!(!client.is_cancelled());
    assert_eq!(client.get_balance(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn constructor_rejects_zero_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let owner = Address::generate(&e);
    let payee = Address::generate(&e);
    let token = Address::generate(&e);
    e.register(RecurringEscrow, (&owner, &payee, 0i128, PERIOD_LEDGERS, &token));
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn constructor_rejects_negative_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let owner = Address::generate(&e);
    let payee = Address::generate(&e);
    let token = Address::generate(&e);
    e.register(RecurringEscrow, (&owner, &payee, -1i128, PERIOD_LEDGERS, &token));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn constructor_rejects_zero_period() {
    let e = Env::default();
    e.mock_all_auths();
    let owner = Address::generate(&e);
    let payee = Address::generate(&e);
    let token = Address::generate(&e);
    e.register(RecurringEscrow, (&owner, &payee, AMOUNT_PER_PERIOD, 0u32, &token));
}

// ────────────────────────────────────────────────────────────────────────────
// Pull tests — timing
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn first_pull_succeeds_immediately() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    // Fund the escrow.
    token::TokenClient::new(&e, &token_addr).transfer(&owner, &client.address, &MINT_AMOUNT);
    assert_eq!(client.get_balance(), MINT_AMOUNT);

    // First pull — should succeed with no period wait.
    client.pull(&payee);

    assert_eq!(client.get_balance(), MINT_AMOUNT - AMOUNT_PER_PERIOD);
    assert_eq!(client.get_last_pull_ledger(), 100);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn pull_before_period_elapsed_rejected() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    token::TokenClient::new(&e, &token_addr).transfer(&owner, &client.address, &MINT_AMOUNT);

    // First pull succeeds.
    client.pull(&payee);

    // Second pull immediately — should fail (only 0 ledgers have passed).
    client.pull(&payee);
}

#[test]
fn pull_after_period_succeeds() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    token::TokenClient::new(&e, &token_addr).transfer(&owner, &client.address, &MINT_AMOUNT);

    client.pull(&payee);

    // Advance by exactly PERIOD_LEDGERS.
    e.ledger().with_mut(|li| {
        li.sequence_number += PERIOD_LEDGERS;
    });

    client.pull(&payee);
    assert_eq!(client.get_balance(), MINT_AMOUNT - AMOUNT_PER_PERIOD * 2);
}

#[test]
fn pull_at_exact_boundary_succeeds() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    token::TokenClient::new(&e, &token_addr).transfer(&owner, &client.address, &MINT_AMOUNT);

    client.pull(&payee);

    // Advance to exactly last_pull + period_ledgers.
    e.ledger().with_mut(|li| {
        li.sequence_number = 100 + PERIOD_LEDGERS;
    });

    client.pull(&payee);
    assert_eq!(client.get_last_pull_ledger(), 100 + PERIOD_LEDGERS);
}

// ────────────────────────────────────────────────────────────────────────────
// Pull tests — balance gate
// ────────────────────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn pull_insufficient_balance_rejected() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    // Fund with less than one period's worth.
    token::TokenClient::new(&e, &token_addr)
        .transfer(&owner, &client.address, &(AMOUNT_PER_PERIOD - 1));

    client.pull(&payee);
}

#[test]
fn pull_exact_balance_succeeds() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    token::TokenClient::new(&e, &token_addr)
        .transfer(&owner, &client.address, &AMOUNT_PER_PERIOD);

    client.pull(&payee);
    assert_eq!(client.get_balance(), 0);
}

// ────────────────────────────────────────────────────────────────────────────
// Pull tests — auth gate
// ────────────────────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn pull_non_payee_rejected() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    token::TokenClient::new(&e, &token_addr).transfer(&owner, &client.address, &MINT_AMOUNT);

    let rando = Address::generate(&e);
    client
        .mock_auths(&[MockAuth {
            address: &rando,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: "pull",
                args: (&rando,).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .pull(&rando);
}

// ────────────────────────────────────────────────────────────────────────────
// Cancel tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn cancel_returns_balance_to_owner() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    token::TokenClient::new(&e, &token_addr).transfer(&owner, &client.address, &MINT_AMOUNT);

    let tok = token::TokenClient::new(&e, &token_addr);
    let owner_before = tok.balance(&owner);

    client.cancel(&owner);

    assert!(client.is_cancelled());
    assert_eq!(client.get_balance(), 0);
    assert_eq!(tok.balance(&owner), owner_before + MINT_AMOUNT);
}

#[test]
fn cancel_disables_future_pulls() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    token::TokenClient::new(&e, &token_addr).transfer(&owner, &client.address, &MINT_AMOUNT);

    client.cancel(&owner);

    // Any subsequent pull must fail with Cancelled.
    let res = client.try_pull(&payee);
    assert!(res.is_err());
}

#[test]
fn cancel_idempotent() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    token::TokenClient::new(&e, &token_addr).transfer(&owner, &client.address, &MINT_AMOUNT);

    client.cancel(&owner);
    // Second cancel should be a no-op.
    client.cancel(&owner);
    assert!(client.is_cancelled());
}

#[test]
fn cancel_with_zero_balance_succeeds() {
    let (_e, owner, _payee, client, _, _) = setup_env();

    // Cancel without ever funding — should work fine.
    client.cancel(&owner);
    assert!(client.is_cancelled());
}

// ────────────────────────────────────────────────────────────────────────────
// Event tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn pull_emits_event() {
    let (e, owner, payee, client, token_addr, _sac) = setup_env();

    token::TokenClient::new(&e, &token_addr).transfer(&owner, &client.address, &MINT_AMOUNT);

    client.pull(&payee);

    let vault_events = e.events().all().filter_by_contract(&client.address);
    assert!(!vault_events.events().is_empty());
}

#[test]
fn cancel_emits_event() {
    let (e, owner, _payee, client, token_addr, _sac) = setup_env();

    token::TokenClient::new(&e, &token_addr).transfer(&owner, &client.address, &MINT_AMOUNT);

    client.cancel(&owner);

    let vault_events = e.events().all().filter_by_contract(&client.address);
    assert!(!vault_events.events().is_empty());
}

// ────────────────────────────────────────────────────────────────────────────
// E2E deployment via smart account (#39)
// ────────────────────────────────────────────────────────────────────────────

mod recurring_escrow_wasm {
    soroban_sdk::contractimport!(file = "testdata/recurring_escrow.wasm");
}

#[test]
fn e2e_deploy_via_smart_account() {
    let e = Env::default();
    e.mock_all_auths();

    e.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });

    // 1. Register a LatchSmartAccount.
    let owner_signers = soroban_sdk::vec![
        &e,
        stellar_accounts::smart_account::Signer::Delegated(Address::generate(&e)),
    ];
    let policies = soroban_sdk::Map::<Address, soroban_sdk::Val>::new(&e);
    let account_id = e.register(smart_account::LatchSmartAccount, (owner_signers, policies));
    let smart_account_client = smart_account::LatchSmartAccountClient::new(&e, &account_id);

    // 2. Upload escrow WASM.
    let wasm_hash = e.deployer().upload_contract_wasm(recurring_escrow_wasm::WASM);

    // 3. Deploy via smart account.
    let salt = soroban_sdk::BytesN::from_array(&e, &[2u8; 32]);
    let payee = Address::generate(&e);
    let admin = Address::generate(&e);
    let token_id = e.register_stellar_asset_contract_v2(admin.clone());

    let init_args =
        (&account_id, &payee, AMOUNT_PER_PERIOD, PERIOD_LEDGERS, &token_id.address())
            .into_val(&e);

    let escrow_address =
        smart_account_client.deploy_contract(&wasm_hash, &salt, &init_args);
    let client = RecurringEscrowClient::new(&e, &escrow_address);

    // 4. Verify config.
    assert_eq!(client.get_owner(), account_id);
    assert_eq!(client.get_payee(), payee);

    // 5. Fund and pull.
    let sac_client = token::StellarAssetClient::new(&e, &token_id.address());
    sac_client.mint(&account_id, &MINT_AMOUNT);

    token::TokenClient::new(&e, &token_id.address())
        .transfer(&account_id, &escrow_address, &MINT_AMOUNT);

    client.pull(&payee);
    assert_eq!(client.get_balance(), MINT_AMOUNT - AMOUNT_PER_PERIOD);

    // Pull again too early — fails.
    let res = client.try_pull(&payee);
    assert!(res.is_err());

    // Advance and pull again.
    e.ledger().with_mut(|li| {
        li.sequence_number += PERIOD_LEDGERS;
    });
    client.pull(&payee);
    assert_eq!(client.get_balance(), MINT_AMOUNT - AMOUNT_PER_PERIOD * 2);
}
