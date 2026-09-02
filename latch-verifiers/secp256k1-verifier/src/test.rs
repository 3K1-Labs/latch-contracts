#![cfg(test)]

extern crate std;

use k256::{
    ecdsa::{RecoveryId, Signature, SigningKey},
    SecretKey,
};
use soroban_sdk::{
    testutils::storage::{Instance as _, Persistent as _, Temporary as _},
    Bytes, BytesN, Env, Vec,
};

use super::{Secp256k1Verifier, Secp256k1VerifierClient};

const TEST_PAYLOAD: [u8; 32] = [
    0x4b, 0xb7, 0xa8, 0xb9, 0x96, 0x09, 0xb0, 0xb8, 0xb1, 0xd5, 0x34, 0x69, 0x4b, 0xb1, 0xf3, 0x1f,
    0x12, 0x91, 0x38, 0xa2, 0xf2, 0xa1, 0x1f, 0x8e, 0x87, 0x02, 0xee, 0xdb, 0xb7, 0x92, 0x92, 0x2e,
];

fn test_signing_key() -> SigningKey {
    let secret_bytes: [u8; 32] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        26, 27, 28, 29, 30, 31, 32,
    ];
    let secret = SecretKey::from_slice(&secret_bytes).unwrap();
    SigningKey::from(&secret)
}

fn test_signing_key_2() -> SigningKey {
    let secret_bytes: [u8; 32] = [
        32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10,
        9, 8, 7, 6, 5, 4, 3, 2, 1,
    ];
    let secret = SecretKey::from_slice(&secret_bytes).unwrap();
    SigningKey::from(&secret)
}

fn public_key_bytes(signing_key: &SigningKey) -> [u8; 65] {
    let encoded = signing_key.verifying_key().to_encoded_point(false);
    let mut bytes = [0_u8; 65];
    bytes.copy_from_slice(encoded.as_bytes());
    bytes
}

fn sign_hash(signing_key: &SigningKey, hash: &[u8; 32]) -> [u8; 65] {
    let (signature, recovery_id) = signing_key.sign_prehash_recoverable(hash).unwrap();
    recoverable_signature_bytes(&signature, recovery_id)
}

fn recoverable_signature_bytes(signature: &Signature, recovery_id: RecoveryId) -> [u8; 65] {
    let mut bytes = [0_u8; 65];
    bytes[..64].copy_from_slice(&signature.to_bytes());
    bytes[64] = recovery_id.to_byte();
    bytes
}

fn high_s_signature(signing_key: &SigningKey, hash: &[u8; 32]) -> [u8; 65] {
    let (low_s_signature, recovery_id) = signing_key.sign_prehash_recoverable(hash).unwrap();
    let high_s = -low_s_signature.s();
    let high_s_signature =
        Signature::from_scalars(low_s_signature.r().to_bytes(), high_s.to_bytes()).unwrap();
    let high_s_recovery_id = RecoveryId::new(!recovery_id.is_y_odd(), recovery_id.is_x_reduced());
    recoverable_signature_bytes(&high_s_signature, high_s_recovery_id)
}

fn register_verifier(e: &Env) -> Secp256k1VerifierClient<'_> {
    let address = e.register(Secp256k1Verifier, ());
    Secp256k1VerifierClient::new(e, &address)
}

fn valid_inputs(e: &Env) -> (Bytes, BytesN<65>, BytesN<65>) {
    let signing_key = test_signing_key();
    (
        Bytes::from_array(e, &TEST_PAYLOAD),
        BytesN::from_array(e, &public_key_bytes(&signing_key)),
        BytesN::from_array(e, &sign_hash(&signing_key, &TEST_PAYLOAD)),
    )
}

#[test]
fn verify_valid_signature() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, signature) = valid_inputs(&e);

    assert!(client.verify(&hash, &public_key, &signature));
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn verify_rejects_short_hash() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (_, public_key, signature) = valid_inputs(&e);

    client.verify(&Bytes::from_array(&e, &[0_u8; 31]), &public_key, &signature);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn verify_rejects_long_hash() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (_, public_key, signature) = valid_inputs(&e);

    client.verify(&Bytes::from_array(&e, &[0_u8; 33]), &public_key, &signature);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn verify_rejects_wrong_digest() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (_, public_key, signature) = valid_inputs(&e);

    client.verify(&Bytes::from_array(&e, &[0xff_u8; 32]), &public_key, &signature);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn verify_rejects_wrong_key() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, _, signature) = valid_inputs(&e);
    let wrong_key = BytesN::from_array(&e, &public_key_bytes(&test_signing_key_2()));

    client.verify(&hash, &wrong_key, &signature);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn verify_rejects_wrong_recovery_id() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, signature) = valid_inputs(&e);
    let mut signature_bytes = signature.to_array();
    signature_bytes[64] ^= 1;

    client.verify(&hash, &public_key, &BytesN::from_array(&e, &signature_bytes));
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn verify_rejects_modified_r() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, signature) = valid_inputs(&e);
    let mut signature_bytes = signature.to_array();
    signature_bytes[0] ^= 1;

    client.verify(&hash, &public_key, &BytesN::from_array(&e, &signature_bytes));
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn verify_rejects_modified_s() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, signature) = valid_inputs(&e);
    let mut signature_bytes = signature.to_array();
    signature_bytes[63] ^= 1;

    client.verify(&hash, &public_key, &BytesN::from_array(&e, &signature_bytes));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn verify_rejects_disallowed_prefix() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, signature) = valid_inputs(&e);
    let mut key_bytes = public_key.to_array();
    key_bytes[0] = 0;

    client.verify(&hash, &BytesN::from_array(&e, &key_bytes), &signature);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn verify_rejects_compressed_point_prefix_in_65_byte_input() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, signature) = valid_inputs(&e);
    let mut key_bytes = public_key.to_array();
    key_bytes[0] = 0x02;

    client.verify(&hash, &BytesN::from_array(&e, &key_bytes), &signature);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn verify_rejects_invalid_curve_point() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, _, signature) = valid_inputs(&e);
    let mut key_bytes = [0_u8; 65];
    key_bytes[0] = 0x04;

    client.verify(&hash, &BytesN::from_array(&e, &key_bytes), &signature);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn verify_rejects_recovery_id_2() {
    assert_recovery_id_rejected(2);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn verify_rejects_ethereum_recovery_id_27() {
    assert_recovery_id_rejected(27);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn verify_rejects_ethereum_recovery_id_28() {
    assert_recovery_id_rejected(28);
}

fn assert_recovery_id_rejected(recovery_id: u8) {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, signature) = valid_inputs(&e);
    let mut signature_bytes = signature.to_array();
    signature_bytes[64] = recovery_id;

    client.verify(&hash, &public_key, &BytesN::from_array(&e, &signature_bytes));
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_zero_r() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, signature) = valid_inputs(&e);
    let mut signature_bytes = signature.to_array();
    signature_bytes[..32].fill(0);

    client.verify(&hash, &public_key, &BytesN::from_array(&e, &signature_bytes));
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_zero_s() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, signature) = valid_inputs(&e);
    let mut signature_bytes = signature.to_array();
    signature_bytes[32..64].fill(0);

    client.verify(&hash, &public_key, &BytesN::from_array(&e, &signature_bytes));
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_high_s_signature() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let public_key = BytesN::from_array(&e, &public_key_bytes(&signing_key));
    let signature = BytesN::from_array(&e, &high_s_signature(&signing_key, &TEST_PAYLOAD));

    // Soroban's native signature parser enforces low-S normalization before
    // recovery. This pins host behavior without duplicating curve arithmetic
    // in the contract.
    client.verify(&hash, &public_key, &signature);
}

#[test]
fn canonicalize_key_is_identity() {
    let e = Env::default();
    let client = register_verifier(&e);
    let key_bytes = public_key_bytes(&test_signing_key());
    let key = BytesN::from_array(&e, &key_bytes);

    let canonical = client.canonicalize_key(&key);

    assert_eq!(canonical, Bytes::from_array(&e, &key_bytes));
    assert_eq!(canonical.len(), 65);
}

#[test]
fn canonicalize_key_preserves_distinct_keys() {
    let e = Env::default();
    let client = register_verifier(&e);
    let key_a = BytesN::from_array(&e, &public_key_bytes(&test_signing_key()));
    let key_b = BytesN::from_array(&e, &public_key_bytes(&test_signing_key_2()));

    assert_ne!(client.canonicalize_key(&key_a), client.canonicalize_key(&key_b));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn canonicalize_key_rejects_disallowed_prefix() {
    let e = Env::default();
    let client = register_verifier(&e);
    let mut key_bytes = public_key_bytes(&test_signing_key());
    key_bytes[0] = 0x03;

    client.canonicalize_key(&BytesN::from_array(&e, &key_bytes));
}

#[test]
fn batch_canonicalize_key_preserves_length_and_order() {
    let e = Env::default();
    let client = register_verifier(&e);
    let key_a_bytes = public_key_bytes(&test_signing_key());
    let key_b_bytes = public_key_bytes(&test_signing_key_2());
    let key_a = BytesN::from_array(&e, &key_a_bytes);
    let key_b = BytesN::from_array(&e, &key_b_bytes);
    let keys = Vec::from_array(&e, [key_a, key_b]);

    let canonical = client.batch_canonicalize_key(&keys);

    assert_eq!(canonical.len(), 2);
    assert_eq!(canonical.get(0).unwrap(), Bytes::from_array(&e, &key_a_bytes));
    assert_eq!(canonical.get(1).unwrap(), Bytes::from_array(&e, &key_b_bytes));
}

#[test]
fn batch_canonicalize_key_single_matches_canonicalize_key() {
    let e = Env::default();
    let client = register_verifier(&e);
    let key = BytesN::from_array(&e, &public_key_bytes(&test_signing_key()));
    let keys = Vec::from_array(&e, [key.clone()]);

    assert_eq!(client.batch_canonicalize_key(&keys).get(0).unwrap(), client.canonicalize_key(&key));
}

#[test]
fn batch_canonicalize_key_accepts_empty_input() {
    let e = Env::default();
    let client = register_verifier(&e);

    assert!(client.batch_canonicalize_key(&Vec::new(&e)).is_empty());
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn batch_canonicalize_key_rejects_disallowed_prefix() {
    let e = Env::default();
    let client = register_verifier(&e);
    let valid_key = BytesN::from_array(&e, &public_key_bytes(&test_signing_key()));
    let mut invalid_key_bytes = public_key_bytes(&test_signing_key_2());
    invalid_key_bytes[0] = 0x02;
    let invalid_key = BytesN::from_array(&e, &invalid_key_bytes);

    client.batch_canonicalize_key(&Vec::from_array(&e, [valid_key, invalid_key]));
}

#[test]
fn verifier_calls_write_no_contract_storage() {
    let e = Env::default();
    let address = e.register(Secp256k1Verifier, ());
    let client = Secp256k1VerifierClient::new(&e, &address);
    let (hash, public_key, signature) = valid_inputs(&e);

    assert!(client.verify(&hash, &public_key, &signature));
    let verify_resources = e.cost_estimate().resources();
    assert_eq!(verify_resources.write_entries, 0);

    client.canonicalize_key(&public_key);
    assert_eq!(e.cost_estimate().resources().write_entries, 0);

    let keys = Vec::from_array(&e, [public_key]);
    client.batch_canonicalize_key(&keys);
    assert_eq!(e.cost_estimate().resources().write_entries, 0);

    e.as_contract(&address, || {
        assert!(e.storage().instance().all().is_empty());
        assert!(e.storage().persistent().all().is_empty());
        assert!(e.storage().temporary().all().is_empty());
    });

    std::println!("representative native verify resources: {verify_resources:#?}");
}
