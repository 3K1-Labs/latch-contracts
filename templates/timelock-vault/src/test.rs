#![cfg(test)]
extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger, MockAuth, MockAuthInvoke},
    token, Address, Env, IntoVal,
};

use crate::{TimelockVault, TimelockVaultClient};

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

/// The unlock point used by most tests — comfortably in the future.
const UNLOCK_LEDGER: u32 = 1_000;
/// Deposit amount used in most tests (10 tokens in 7-decimal stroops).
const DEPOSIT_AMOUNT: i128 = 10_000_000;

/// Registers the vault contract with a default owner and unlock ledger,
/// creates and funds a SAC token, and returns everything the caller needs.
fn setup_env<'a>() -> (Env, Address, TimelockVaultClient<'a>, Address, token::StellarAssetClient<'a>)
{
    let e = Env::default();
    e.mock_all_auths();

    // Start at ledger 100 so UNLOCK_LEDGER is in the future.
    e.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });

    let owner = Address::generate(&e);
    let vault_id = e.register(TimelockVault, (&owner, UNLOCK_LEDGER));
    let vault_client = TimelockVaultClient::new(&e, &vault_id);

    // Stand up a SAC token and mint some to the owner so we can deposit.
    let admin = Address::generate(&e);
    let token_id = e.register_stellar_asset_contract_v2(admin.clone());
    let sac_client = token::StellarAssetClient::new(&e, &token_id.address());
    sac_client.mint(&owner, &(DEPOSIT_AMOUNT * 10));

    (e, owner, vault_client, token_id.address(), sac_client)
}

/// Registers a *second* SAC token — used for multi-token tests.
fn register_second_token<'a>(
    e: &Env,
    mint_to: &Address,
    amount: i128,
) -> (Address, token::StellarAssetClient<'a>) {
    let admin = Address::generate(e);
    let token_id = e.register_stellar_asset_contract_v2(admin.clone());
    let sac_client = token::StellarAssetClient::new(e, &token_id.address());
    sac_client.mint(mint_to, &amount);
    (token_id.address(), sac_client)
}

// ────────────────────────────────────────────────────────────────────────────
// Constructor tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn constructor_stores_owner_and_unlock() {
    let (e, owner, client, _, _) = setup_env();

    assert_eq!(client.get_owner(), owner);
    assert_eq!(client.get_unlock_ledger(), UNLOCK_LEDGER);

    // Balance starts at zero — nothing deposited yet.
    let token_addr = e.register_stellar_asset_contract_v2(Address::generate(&e));
    assert_eq!(client.get_balance(&token_addr.address()), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // InvalidUnlockLedger
fn constructor_rejects_past_unlock() {
    let e = Env::default();
    e.mock_all_auths();

    e.ledger().with_mut(|li| {
        li.sequence_number = 500;
    });

    let owner = Address::generate(&e);
    // unlock_ledger = 100 < current sequence 500 → must reject.
    e.register(TimelockVault, (&owner, 100u32));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // InvalidUnlockLedger
fn constructor_rejects_equal_unlock() {
    let e = Env::default();
    e.mock_all_auths();

    e.ledger().with_mut(|li| {
        li.sequence_number = 500;
    });

    let owner = Address::generate(&e);
    // unlock_ledger == current sequence → must reject (not strictly in the
    // future).
    e.register(TimelockVault, (&owner, 500u32));
}

// ────────────────────────────────────────────────────────────────────────────
// Deposit tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn deposit_transfers_and_shows_balance() {
    let (_e, owner, client, token_addr, _sac) = setup_env();

    client.deposit(&owner, &token_addr, &DEPOSIT_AMOUNT);

    assert_eq!(client.get_balance(&token_addr), DEPOSIT_AMOUNT);
}

// ────────────────────────────────────────────────────────────────────────────
// Withdraw tests — time gate
// ────────────────────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #1)")] // StillLocked
fn withdraw_before_unlock_rejected() {
    let (e, owner, client, token_addr, _sac) = setup_env();

    client.deposit(&owner, &token_addr, &DEPOSIT_AMOUNT);

    // Current ledger is 100, unlock is 1000 — still locked.
    assert!(e.ledger().sequence() < UNLOCK_LEDGER);
    client.withdraw(&token_addr, &DEPOSIT_AMOUNT, &owner);
}

#[test]
fn withdraw_after_unlock_succeeds() {
    let (e, owner, client, token_addr, _sac) = setup_env();

    client.deposit(&owner, &token_addr, &DEPOSIT_AMOUNT);

    // Advance past the unlock point.
    e.ledger().with_mut(|li| {
        li.sequence_number = UNLOCK_LEDGER + 1;
    });

    client.withdraw(&token_addr, &DEPOSIT_AMOUNT, &owner);

    assert_eq!(client.get_balance(&token_addr), 0);
    // Owner received the funds back.
    let tok = token::TokenClient::new(&e, &token_addr);
    assert_eq!(tok.balance(&owner), DEPOSIT_AMOUNT * 10); // original mint restored
}

#[test]
fn withdraw_at_exact_unlock_ledger_succeeds() {
    let (e, owner, client, token_addr, _sac) = setup_env();

    client.deposit(&owner, &token_addr, &DEPOSIT_AMOUNT);

    // Advance to exactly the unlock ledger — boundary check.
    e.ledger().with_mut(|li| {
        li.sequence_number = UNLOCK_LEDGER;
    });

    client.withdraw(&token_addr, &DEPOSIT_AMOUNT, &owner);

    assert_eq!(client.get_balance(&token_addr), 0);
}

// ────────────────────────────────────────────────────────────────────────────
// Withdraw tests — owner gate
// ────────────────────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn withdraw_non_owner_rejected_before_unlock() {
    let (_e, owner, client, token_addr, _sac) = setup_env();

    client.deposit(&owner, &token_addr, &DEPOSIT_AMOUNT);

    let env = client.env.clone();
    let non_owner = Address::generate(&env);
    // Non-owner + before unlock → must fail (auth will fail first).
    client
        .mock_auths(&[MockAuth {
            address: &non_owner,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: "withdraw",
                args: (&token_addr, DEPOSIT_AMOUNT, &non_owner).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .withdraw(&token_addr, &DEPOSIT_AMOUNT, &non_owner);
}

#[test]
#[should_panic]
fn withdraw_non_owner_rejected_after_unlock() {
    let (e, owner, client, token_addr, _sac) = setup_env();

    client.deposit(&owner, &token_addr, &DEPOSIT_AMOUNT);

    // Advance past unlock — the time gate passes, but the auth gate must
    // still reject a non-owner.
    e.ledger().with_mut(|li| {
        li.sequence_number = UNLOCK_LEDGER + 1;
    });

    let non_owner = Address::generate(&e);
    client
        .mock_auths(&[MockAuth {
            address: &non_owner,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: "withdraw",
                args: (&token_addr, DEPOSIT_AMOUNT, &non_owner).into_val(&e),
                sub_invokes: &[],
            },
        }])
        .withdraw(&token_addr, &DEPOSIT_AMOUNT, &non_owner);
}

// ────────────────────────────────────────────────────────────────────────────
// Multi-token tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn works_with_multiple_tokens() {
    let (e, owner, client, token_a, _sac_a) = setup_env();
    let (token_b, _sac_b) = register_second_token(&e, &owner, DEPOSIT_AMOUNT * 10);

    // Deposit both tokens.
    client.deposit(&owner, &token_a, &DEPOSIT_AMOUNT);
    client.deposit(&owner, &token_b, &(DEPOSIT_AMOUNT * 2));

    assert_eq!(client.get_balance(&token_a), DEPOSIT_AMOUNT);
    assert_eq!(client.get_balance(&token_b), DEPOSIT_AMOUNT * 2);

    // Advance past unlock.
    e.ledger().with_mut(|li| {
        li.sequence_number = UNLOCK_LEDGER + 1;
    });

    // Withdraw each token separately.
    client.withdraw(&token_a, &DEPOSIT_AMOUNT, &owner);
    client.withdraw(&token_b, &(DEPOSIT_AMOUNT * 2), &owner);

    assert_eq!(client.get_balance(&token_a), 0);
    assert_eq!(client.get_balance(&token_b), 0);
}

// ────────────────────────────────────────────────────────────────────────────
// Event tests
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn deposit_emits_event() {
    let (e, owner, client, token_addr, _sac) = setup_env();

    client.deposit(&owner, &token_addr, &DEPOSIT_AMOUNT);

    // Filter events to only those emitted by the vault contract.
    let vault_events = e.events().all().filter_by_contract(&client.address);
    // The vault should have emitted at least one event (the Deposited event).
    assert!(!vault_events.events().is_empty(), "expected at least one vault event after deposit");
}

#[test]
fn withdraw_emits_event() {
    let (e, owner, client, token_addr, _sac) = setup_env();

    client.deposit(&owner, &token_addr, &DEPOSIT_AMOUNT);

    e.ledger().with_mut(|li| {
        li.sequence_number = UNLOCK_LEDGER + 1;
    });

    client.withdraw(&token_addr, &DEPOSIT_AMOUNT, &owner);

    // Filter events to only those emitted by the vault contract.
    let vault_events = e.events().all().filter_by_contract(&client.address);
    // The vault should have emitted at least one event (the Withdrawn event).
    assert!(
        !vault_events.events().is_empty(),
        "expected at least one vault event after withdrawal"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Partial withdrawal
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn partial_withdraw_leaves_remainder() {
    let (e, owner, client, token_addr, _sac) = setup_env();

    client.deposit(&owner, &token_addr, &DEPOSIT_AMOUNT);

    e.ledger().with_mut(|li| {
        li.sequence_number = UNLOCK_LEDGER + 1;
    });

    let half = DEPOSIT_AMOUNT / 2;
    client.withdraw(&token_addr, &half, &owner);

    assert_eq!(client.get_balance(&token_addr), DEPOSIT_AMOUNT - half);

    // Second withdrawal for the rest.
    client.withdraw(&token_addr, &(DEPOSIT_AMOUNT - half), &owner);
    assert_eq!(client.get_balance(&token_addr), 0);
}

// ────────────────────────────────────────────────────────────────────────────
// E2E deployment via smart account (#39)
// ────────────────────────────────────────────────────────────────────────────

#[test]
#[ignore] // TODO: unblock when #39 (CreateContract-gated entrypoint) lands.
fn e2e_deploy_via_smart_account() {
    // This test will deploy the timelock-vault WASM through the smart
    // account's yet-to-be-implemented `create_contract` entrypoint (#39),
    // then exercise deposit + time-gated withdrawal end-to-end.
    //
    // Outline:
    // 1. Register a LatchSmartAccount with a default signer.
    // 2. Upload the timelock-vault WASM to the environment.
    // 3. Call smart_account.create_contract(wasm_hash, salt, init_args).
    // 4. Verify the vault is owned by the smart account.
    // 5. Deposit, advance ledger, withdraw.
    //
    // Cannot be implemented until #39 provides the `create_contract`
    // entrypoint on `latch-smart-account`.
    todo!("Implement once #39 lands");
}
