#![cfg(test)]

extern crate std;

use p256::{
    ecdsa::{signature::hazmat::PrehashSigner, Signature as P256Signature, SigningKey},
    SecretKey,
};
use soroban_sdk::{Bytes, Env, Symbol, Vec};

use super::{P256Verifier, P256VerifierClient};

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
    let point = signing_key.verifying_key().to_encoded_point(false);
    let bytes = point.as_bytes();
    let mut out = [0u8; 65];
    out.copy_from_slice(bytes);
    out
}

fn sign_hash(signing_key: &SigningKey, hash: &[u8; 32]) -> [u8; 64] {
    let signature: P256Signature = signing_key.sign_prehash(hash).unwrap();
    let normalized = signature.normalize_s().unwrap_or(signature);
    let mut out = [0u8; 64];
    out.copy_from_slice(&normalized.to_bytes());
    out
}

fn sign_hash_high_s(signing_key: &SigningKey, hash: &[u8; 32]) -> [u8; 64] {
    let signature: P256Signature = signing_key.sign_prehash(hash).unwrap();
    let s_high = -signature.s();
    let high_sig = P256Signature::from_scalars(signature.r().to_bytes(), s_high.to_bytes()).unwrap();
    let mut out = [0u8; 64];
    out.copy_from_slice(&high_sig.to_bytes());
    out
}

const TEST_PAYLOAD: [u8; 32] = [
    0x4b, 0xb7, 0xa8, 0xb9, 0x96, 0x09, 0xb0, 0xb8, 0xb1, 0xd5, 0x34, 0x69, 0x4b, 0xb1, 0xf3, 0x1f,
    0x12, 0x91, 0x38, 0xa2, 0xf2, 0xa1, 0x1f, 0x8e, 0x87, 0x02, 0xee, 0xdb, 0xb7, 0x92, 0x92, 0x2e,
];

fn register_verifier(e: &Env) -> P256VerifierClient<'_> {
    let addr = e.register(P256Verifier, ());
    P256VerifierClient::new(e, &addr)
}

#[test]
fn verify_valid_signature() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key = soroban_sdk::BytesN::<65>::from_array(&e, &public_key_bytes(&signing_key));
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let sig = soroban_sdk::BytesN::<64>::from_array(&e, &sign_hash(&signing_key, &TEST_PAYLOAD));

    assert!(client.verify(&hash, &pub_key, &sig));
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_wrong_digest() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key = soroban_sdk::BytesN::<65>::from_array(&e, &public_key_bytes(&signing_key));
    let wrong_hash = Bytes::from_array(&e, &[0xffu8; 32]);
    let sig = soroban_sdk::BytesN::<64>::from_array(&e, &sign_hash(&signing_key, &TEST_PAYLOAD));

    client.verify(&wrong_hash, &pub_key, &sig);
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_wrong_key() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let other_key = test_signing_key_2();
    let wrong_key = soroban_sdk::BytesN::<65>::from_array(&e, &public_key_bytes(&other_key));
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let sig = soroban_sdk::BytesN::<64>::from_array(&e, &sign_hash(&signing_key, &TEST_PAYLOAD));

    client.verify(&hash, &wrong_key, &sig);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn verify_rejects_malformed_key_prefix() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let sig = soroban_sdk::BytesN::<64>::from_array(&e, &sign_hash(&signing_key, &TEST_PAYLOAD));
    let mut malformed = public_key_bytes(&signing_key);
    malformed[0] = 0x02;
    let malformed_key = soroban_sdk::BytesN::<65>::from_array(&e, &malformed);

    client.verify(&hash, &malformed_key, &sig);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn verify_rejects_non_uncompressed_key_encoding() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let sig = soroban_sdk::BytesN::<64>::from_array(&e, &sign_hash(&signing_key, &TEST_PAYLOAD));

    let mut compressed = [0u8; 65];
    compressed[0] = 0x02;
    compressed[1..].copy_from_slice(&public_key_bytes(&signing_key)[1..]);
    let key = soroban_sdk::BytesN::<65>::from_array(&e, &compressed);

    client.verify(&hash, &key, &sig);
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_modified_r_or_s() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key = soroban_sdk::BytesN::<65>::from_array(&e, &public_key_bytes(&signing_key));
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let mut sig_bytes = sign_hash(&signing_key, &TEST_PAYLOAD);
    sig_bytes[63] ^= 0x01;
    let sig = soroban_sdk::BytesN::<64>::from_array(&e, &sig_bytes);

    client.verify(&hash, &pub_key, &sig);
}

#[test]
fn verify_accepts_high_s_signature() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key = soroban_sdk::BytesN::<65>::from_array(&e, &public_key_bytes(&signing_key));
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let sig = soroban_sdk::BytesN::<64>::from_array(&e, &sign_hash_high_s(&signing_key, &TEST_PAYLOAD));

    assert!(client.verify(&hash, &pub_key, &sig));
}

#[test]
fn canonicalize_key_is_identity() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let key = soroban_sdk::BytesN::<65>::from_array(&e, &public_key_bytes(&signing_key));

    let canonical = client.canonicalize_key(&key);
    assert_eq!(canonical, Bytes::from_slice(&e, &public_key_bytes(&signing_key)));
}

#[test]
fn batch_canonicalize_key_preserves_order() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key_1 = test_signing_key();
    let signing_key_2 = test_signing_key_2();
    let key_1 = soroban_sdk::BytesN::<65>::from_array(&e, &public_key_bytes(&signing_key_1));
    let key_2 = soroban_sdk::BytesN::<65>::from_array(&e, &public_key_bytes(&signing_key_2));
    let keys = Vec::from_array(&e, [key_1.clone(), key_2.clone()]);

    let canonical = client.batch_canonicalize_key(&keys);
    assert_eq!(canonical.get(0).unwrap(), Bytes::from_slice(&e, &public_key_bytes(&signing_key_1)));
    assert_eq!(canonical.get(1).unwrap(), Bytes::from_slice(&e, &public_key_bytes(&signing_key_2)));
}

