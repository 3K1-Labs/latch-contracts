#![cfg(test)]

extern crate std;

use p256::{
    ecdsa::{signature::hazmat::PrehashSigner, Signature as P256Signature, SigningKey},
    SecretKey,
};
use soroban_sdk::{
    testutils::storage::{Instance as _, Persistent as _, Temporary as _},
    Bytes, BytesN, Env, Vec,
};

use super::{P256Verifier, P256VerifierClient};

const TEST_PAYLOAD: [u8; 32] = [
    0x4b, 0xb7, 0xa8, 0xb9, 0x96, 0x09, 0xb0, 0xb8, 0xb1, 0xd5, 0x34, 0x69, 0x4b, 0xb1, 0xf3, 0x1f,
    0x12, 0x91, 0x38, 0xa2, 0xf2, 0xa1, 0x1f, 0x8e, 0x87, 0x02, 0xee, 0xdb, 0xb7, 0x92, 0x92, 0x2e,
];

fn test_signing_key() -> SigningKey {
    let secret_bytes: [u8; 32] = [
        33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55,
        56, 57, 58, 59, 60, 61, 62, 63, 64,
    ];
    let secret = SecretKey::from_slice(&secret_bytes).unwrap();
    SigningKey::from(&secret)
}

fn test_signing_key_2() -> SigningKey {
    let secret_bytes: [u8; 32] = [
        64, 63, 62, 61, 60, 59, 58, 57, 56, 55, 54, 53, 52, 51, 50, 49, 48, 47, 46, 45, 44, 43, 42,
        41, 40, 39, 38, 37, 36, 35, 34, 33,
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

fn sign_hash(signing_key: &SigningKey, hash: &[u8; 32]) -> [u8; 64] {
    let signature: P256Signature = signing_key.sign_prehash(hash).unwrap();
    let low_s = signature.normalize_s().unwrap_or(signature);
    low_s.to_bytes().into()
}

fn sign_hash_high_s(signing_key: &SigningKey, hash: &[u8; 32]) -> [u8; 64] {
    let signature: P256Signature = signing_key.sign_prehash(hash).unwrap();
    let low_s = signature.normalize_s().unwrap_or(signature);
    let high_s = -low_s.s();
    P256Signature::from_scalars(low_s.r().to_bytes(), high_s.to_bytes()).unwrap().to_bytes().into()
}

fn register_verifier(e: &Env) -> P256VerifierClient<'_> {
    let address = e.register(P256Verifier, ());
    P256VerifierClient::new(e, &address)
}

fn valid_inputs(e: &Env) -> (Bytes, BytesN<65>, BytesN<64>) {
    let signing_key = test_signing_key();
    (
        Bytes::from_array(e, &TEST_PAYLOAD),
        BytesN::from_array(e, &public_key_bytes(&signing_key)),
        BytesN::from_array(e, &sign_hash(&signing_key, &TEST_PAYLOAD)),
    )
}

#[test]
fn verify_valid_low_s_signature() {
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
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_wrong_digest() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (_, public_key, signature) = valid_inputs(&e);

    client.verify(&Bytes::from_array(&e, &[0xff_u8; 32]), &public_key, &signature);
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_wrong_key() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, _, signature) = valid_inputs(&e);
    let wrong_key = BytesN::from_array(&e, &public_key_bytes(&test_signing_key_2()));

    client.verify(&hash, &wrong_key, &signature);
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_modified_r() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, _) = valid_inputs(&e);
    let mut signature = sign_hash(&test_signing_key(), &TEST_PAYLOAD);
    signature[0] ^= 1;

    client.verify(&hash, &public_key, &BytesN::from_array(&e, &signature));
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_modified_s() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, _) = valid_inputs(&e);
    let mut signature = sign_hash(&test_signing_key(), &TEST_PAYLOAD);
    signature[63] ^= 1;

    client.verify(&hash, &public_key, &BytesN::from_array(&e, &signature));
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
    // The fixed ABI requires 65 bytes, so this represents a disallowed 0x02
    // prefix rather than a literal 33-byte compressed SEC1 point.
    key_bytes[0] = 0x02;

    client.verify(&hash, &BytesN::from_array(&e, &key_bytes), &signature);
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_invalid_curve_point() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, _, signature) = valid_inputs(&e);
    let mut key_bytes = [0_u8; 65];
    key_bytes[0] = 0x04;

    client.verify(&hash, &BytesN::from_array(&e, &key_bytes), &signature);
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_zero_r() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, _) = valid_inputs(&e);
    let mut signature = sign_hash(&test_signing_key(), &TEST_PAYLOAD);
    signature[..32].fill(0);

    client.verify(&hash, &public_key, &BytesN::from_array(&e, &signature));
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_zero_s() {
    let e = Env::default();
    let client = register_verifier(&e);
    let (hash, public_key, _) = valid_inputs(&e);
    let mut signature = sign_hash(&test_signing_key(), &TEST_PAYLOAD);
    signature[32..].fill(0);

    client.verify(&hash, &public_key, &BytesN::from_array(&e, &signature));
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_high_s_signature() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let public_key = BytesN::from_array(&e, &public_key_bytes(&signing_key));
    let high_s_signature = BytesN::from_array(&e, &sign_hash_high_s(&signing_key, &TEST_PAYLOAD));

    client.verify(&hash, &public_key, &high_s_signature);
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
fn verifier_calls_write_no_contract_storage() {
    let e = Env::default();
    let address = e.register(P256Verifier, ());
    let client = P256VerifierClient::new(&e, &address);
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
