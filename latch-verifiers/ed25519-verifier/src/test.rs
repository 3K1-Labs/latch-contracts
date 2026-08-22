#![cfg(test)]

extern crate std;

use ed25519_dalek::{Signer as DalekSigner, SigningKey};
use soroban_sdk::{Bytes, BytesN, Env, Vec};

use super::{Ed25519Verifier, Ed25519VerifierClient};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Generates a deterministic keypair from a fixed seed for testing.
fn test_keypair() -> SigningKey {
    let secret: [u8; 32] = [
        157, 97, 177, 157, 239, 253, 90, 96, 186, 132, 74, 244, 146, 236, 44, 196, 68, 73, 197,
        105, 123, 50, 105, 25, 112, 59, 172, 3, 28, 174, 127, 96,
    ];
    SigningKey::from_bytes(&secret)
}

fn test_keypair_2() -> SigningKey {
    let secret: [u8; 32] = [
        200, 100, 150, 200, 240, 250, 95, 100, 190, 140, 80, 250, 150, 240, 50, 200, 70, 80, 200,
        110, 130, 55, 110, 30, 115, 65, 175, 10, 35, 180, 130, 100,
    ];
    SigningKey::from_bytes(&secret)
}

const TEST_PAYLOAD: [u8; 32] = [
    0x4b, 0xb7, 0xa8, 0xb9, 0x96, 0x09, 0xb0, 0xb8, 0xb1, 0xd5, 0x34, 0x69, 0x4b, 0xb1, 0xf3, 0x1f,
    0x12, 0x91, 0x38, 0xa2, 0xf2, 0xa1, 0x1f, 0x8e, 0x87, 0x02, 0xee, 0xdb, 0xb7, 0x92, 0x92, 0x2e,
];

fn test_payload(e: &Env) -> Bytes {
    Bytes::from_array(e, &TEST_PAYLOAD)
}

fn register_verifier(e: &Env) -> Ed25519VerifierClient<'_> {
    let addr = e.register(Ed25519Verifier, ());
    Ed25519VerifierClient::new(e, &addr)
}

// ── verify tests ─────────────────────────────────────────────────────────────

#[test]
fn verify_valid_signature() {
    let e = Env::default();
    let client = register_verifier(&e);

    let keypair = test_keypair();
    let pub_key = BytesN::<32>::from_array(&e, keypair.verifying_key().as_bytes());

    let hash = test_payload(&e);
    let sig_bytes = keypair.sign(&TEST_PAYLOAD).to_bytes();
    let sig = BytesN::<64>::from_array(&e, &sig_bytes);

    assert!(client.verify(&hash, &pub_key, &sig));
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_corrupted_signature() {
    let e = Env::default();
    let client = register_verifier(&e);

    let keypair = test_keypair();
    let pub_key = BytesN::<32>::from_array(&e, keypair.verifying_key().as_bytes());

    let hash = test_payload(&e);
    let mut sig_bytes = keypair.sign(&TEST_PAYLOAD).to_bytes();
    sig_bytes[0] = sig_bytes[0].wrapping_add(1); // corrupt
    let sig = BytesN::<64>::from_array(&e, &sig_bytes);

    client.verify(&hash, &pub_key, &sig);
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_wrong_key() {
    let e = Env::default();
    let client = register_verifier(&e);

    let keypair = test_keypair();
    let keypair2 = test_keypair_2();

    // Sign with keypair, verify with keypair2's public key
    let wrong_pub_key = BytesN::<32>::from_array(&e, keypair2.verifying_key().as_bytes());

    let hash = test_payload(&e);
    let sig_bytes = keypair.sign(&TEST_PAYLOAD).to_bytes();
    let sig = BytesN::<64>::from_array(&e, &sig_bytes);

    client.verify(&hash, &wrong_pub_key, &sig);
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_wrong_payload() {
    let e = Env::default();
    let client = register_verifier(&e);

    let keypair = test_keypair();
    let pub_key = BytesN::<32>::from_array(&e, keypair.verifying_key().as_bytes());

    // Sign one hash, verify against a different hash
    let hash_b = Bytes::from_array(&e, &[0xffu8; 32]);

    let sig_bytes = keypair.sign(&TEST_PAYLOAD).to_bytes();
    let sig = BytesN::<64>::from_array(&e, &sig_bytes);

    // Pass hash_b but signature was over TEST_PAYLOAD
    client.verify(&hash_b, &pub_key, &sig);
}

// ── canonicalize_key tests ──────────────────────────────────────────────────

#[test]
fn canonicalize_key_is_identity() {
    let e = Env::default();
    let client = register_verifier(&e);

    let key_bytes = [42u8; 32];
    let key = BytesN::<32>::from_array(&e, &key_bytes);

    let canonical = client.canonicalize_key(&key);

    assert_eq!(canonical, Bytes::from_array(&e, &key_bytes));
    assert_eq!(canonical.len(), 32);
}

#[test]
fn canonicalize_key_distinct_keys_produce_distinct_output() {
    let e = Env::default();
    let client = register_verifier(&e);

    let key_a = BytesN::<32>::from_array(&e, &[1u8; 32]);
    let key_b = BytesN::<32>::from_array(&e, &[2u8; 32]);

    assert_ne!(client.canonicalize_key(&key_a), client.canonicalize_key(&key_b));
}

// ── batch_canonicalize_key tests ────────────────────────────────────────────

#[test]
fn batch_canonicalize_key_preserves_order() {
    let e = Env::default();
    let client = register_verifier(&e);

    let key1 = BytesN::<32>::from_array(&e, &[1u8; 32]);
    let key2 = BytesN::<32>::from_array(&e, &[2u8; 32]);
    let key3 = BytesN::<32>::from_array(&e, &[3u8; 32]);

    let keys = Vec::from_array(&e, [key1.clone(), key2.clone(), key3.clone()]);
    let canonical = client.batch_canonicalize_key(&keys);

    assert_eq!(canonical.len(), 3);
    assert_eq!(canonical.get(0).unwrap(), Bytes::from_array(&e, &[1u8; 32]));
    assert_eq!(canonical.get(1).unwrap(), Bytes::from_array(&e, &[2u8; 32]));
    assert_eq!(canonical.get(2).unwrap(), Bytes::from_array(&e, &[3u8; 32]));
}

#[test]
fn batch_canonicalize_key_single_matches_canonicalize_key() {
    let e = Env::default();
    let client = register_verifier(&e);

    let key = BytesN::<32>::from_array(&e, &[7u8; 32]);
    let keys = Vec::from_array(&e, [key.clone()]);

    let batch_result = client.batch_canonicalize_key(&keys);
    let single_result = client.canonicalize_key(&key);

    assert_eq!(batch_result.get(0).unwrap(), single_result);
}
