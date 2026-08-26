#![cfg(test)]

extern crate std;

use k256::ecdsa::{signature::hazmat::PrehashSigner, RecoveryId, SigningKey};
use soroban_sdk::{Bytes, BytesN, Env, Vec};

use super::{Secp256k1Verifier, Secp256k1VerifierClient};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Deterministic test keypair #1.
fn test_signing_key_1() -> SigningKey {
    // 32 non-zero bytes chosen to avoid the all-zero scalar (invalid).
    let secret = [
        0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x7a, 0x8b, 0x9c, 0xad, 0xbe, 0xcf, 0xda, 0xeb,
        0xfc, 0x01, 0x12, 0x23, 0x34, 0x45, 0x56, 0x67, 0x78, 0x89, 0x9a, 0xab, 0xbc, 0xcd,
        0xde, 0xef, 0xf0, 0x11,
    ];
    SigningKey::from_bytes(&secret.into()).expect("valid secret scalar")
}

/// Deterministic test keypair #2 — distinct from #1.
fn test_signing_key_2() -> SigningKey {
    let secret = [
        0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10, 0xef, 0xcd, 0xab, 0x89, 0x67, 0x45,
        0x23, 0x01, 0xf0, 0xe1, 0xd2, 0xc3, 0xb4, 0xa5, 0x96, 0x87, 0x78, 0x69, 0x5a, 0x4b,
        0x3c, 0x2d, 0x1e, 0x0f,
    ];
    SigningKey::from_bytes(&secret.into()).expect("valid secret scalar")
}

const TEST_PAYLOAD: [u8; 32] = [
    0x4b, 0xb7, 0xa8, 0xb9, 0x96, 0x09, 0xb0, 0xb8, 0xb1, 0xd5, 0x34, 0x69, 0x4b, 0xb1, 0xf3,
    0x1f, 0x12, 0x91, 0x38, 0xa2, 0xf2, 0xa1, 0x1f, 0x8e, 0x87, 0x02, 0xee, 0xdb, 0xb7, 0x92,
    0x92, 0x2e,
];

/// Returns the 65-byte uncompressed public key bytes for a signing key.
fn uncompressed_pubkey(sk: &SigningKey) -> [u8; 65] {
    let ep = sk.verifying_key().to_encoded_point(false); // false = uncompressed
    ep.as_bytes()
        .try_into()
        .expect("uncompressed pubkey is 65 bytes")
}

/// Signs `payload` with `sk` over the prehash (raw bytes) and returns a
/// 65-byte `sig_data`: r‖s‖recovery_id.
fn sign(sk: &SigningKey, payload: &[u8; 32]) -> [u8; 65] {
    let (sig, recid): (k256::ecdsa::Signature, RecoveryId) =
        sk.sign_prehash(payload).expect("signing should not fail");
    let sig_bytes: [u8; 64] = sig.to_bytes().into();
    let mut out = [0u8; 65];
    out[..64].copy_from_slice(&sig_bytes);
    out[64] = recid.to_byte();
    out
}

fn register_verifier(e: &Env) -> Secp256k1VerifierClient<'_> {
    let addr = e.register(Secp256k1Verifier, ());
    Secp256k1VerifierClient::new(e, &addr)
}

// ── verify: happy path ───────────────────────────────────────────────────────

#[test]
fn verify_valid_signature() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk = test_signing_key_1();
    let pub_key_bytes = uncompressed_pubkey(&sk);
    let pub_key = BytesN::<65>::from_array(&e, &pub_key_bytes);
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let sig_bytes = sign(&sk, &TEST_PAYLOAD);
    let sig = BytesN::<65>::from_array(&e, &sig_bytes);

    // Must succeed regardless of which recovery_id k256 produced.
    assert!(client.verify(&hash, &pub_key, &sig));
}

// ── verify: wrong recovery_id ────────────────────────────────────────────────

/// Flipping the recovery_id makes recover() return the *other* candidate key,
/// which is a different curve point than the registered public key.
#[test]
#[should_panic]
fn verify_rejects_wrong_recovery_id() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk = test_signing_key_1();
    let pub_key_bytes = uncompressed_pubkey(&sk);
    let pub_key = BytesN::<65>::from_array(&e, &pub_key_bytes);
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);

    let mut sig_bytes = sign(&sk, &TEST_PAYLOAD);
    // Flip the recovery_id bit (0→1 or 1→0).
    sig_bytes[64] = 1 - sig_bytes[64];
    let sig = BytesN::<65>::from_array(&e, &sig_bytes);

    // The recovered key won't match key_data — must panic.
    client.verify(&hash, &pub_key, &sig);
}

// ── verify: wrong key ────────────────────────────────────────────────────────

/// Signature is valid for sk1 but key_data points to sk2.
#[test]
#[should_panic]
fn verify_rejects_wrong_key() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk1 = test_signing_key_1();
    let sk2 = test_signing_key_2();

    // Sign with sk1, but supply sk2's public key as key_data.
    let wrong_pub_key = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&sk2));
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let sig_bytes = sign(&sk1, &TEST_PAYLOAD);
    let sig = BytesN::<65>::from_array(&e, &sig_bytes);

    // Recovered key is sk1's key, which != sk2's key — must panic.
    client.verify(&hash, &wrong_pub_key, &sig);
}

// ── verify: wrong hash ───────────────────────────────────────────────────────

/// Signature was made over TEST_PAYLOAD but a different hash is presented.
#[test]
#[should_panic]
fn verify_rejects_wrong_hash() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk = test_signing_key_1();
    let pub_key = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&sk));

    let sig_bytes = sign(&sk, &TEST_PAYLOAD);
    let sig = BytesN::<65>::from_array(&e, &sig_bytes);

    // Pass a different hash — the recovered key from (different_hash, sig)
    // will not be our public key.
    let different_hash = Bytes::from_array(&e, &[0xffu8; 32]);

    client.verify(&different_hash, &pub_key, &sig);
}

// ── verify: corrupted signature ──────────────────────────────────────────────

/// Corrupting `r` — either the host panics on the invalid ECDSA data, or
/// recovery produces the wrong key. Either way the call must not succeed.
#[test]
#[should_panic]
fn verify_rejects_corrupted_r_bytes() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk = test_signing_key_1();
    let pub_key = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&sk));
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);

    let mut sig_bytes = sign(&sk, &TEST_PAYLOAD);
    sig_bytes[0] = sig_bytes[0].wrapping_add(1); // corrupt r
    let sig = BytesN::<65>::from_array(&e, &sig_bytes);

    client.verify(&hash, &pub_key, &sig);
}

/// Corrupting `s`.
#[test]
#[should_panic]
fn verify_rejects_corrupted_s_bytes() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk = test_signing_key_1();
    let pub_key = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&sk));
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);

    let mut sig_bytes = sign(&sk, &TEST_PAYLOAD);
    sig_bytes[32] = sig_bytes[32].wrapping_add(1); // corrupt s
    let sig = BytesN::<65>::from_array(&e, &sig_bytes);

    client.verify(&hash, &pub_key, &sig);
}

// ── verify: malformed key_data ───────────────────────────────────────────────

/// key_data starts with `0x02` (compressed-key prefix) instead of `0x04`.
#[test]
#[should_panic]
fn verify_rejects_key_with_wrong_prefix() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk = test_signing_key_1();
    let mut pub_key_bytes = uncompressed_pubkey(&sk);
    pub_key_bytes[0] = 0x02; // wrong prefix
    let pub_key = BytesN::<65>::from_array(&e, &pub_key_bytes);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let sig_bytes = sign(&sk, &TEST_PAYLOAD);
    let sig = BytesN::<65>::from_array(&e, &sig_bytes);

    client.verify(&hash, &pub_key, &sig);
}

// ── verify: invalid hash length ──────────────────────────────────────────────

#[test]
#[should_panic]
fn verify_rejects_hash_shorter_than_32_bytes() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk = test_signing_key_1();
    let pub_key = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&sk));
    let sig_bytes = sign(&sk, &TEST_PAYLOAD);
    let sig = BytesN::<65>::from_array(&e, &sig_bytes);

    let short_hash = Bytes::from_array(&e, &[0xabu8; 16]);
    client.verify(&short_hash, &pub_key, &sig);
}

#[test]
#[should_panic]
fn verify_rejects_hash_longer_than_32_bytes() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk = test_signing_key_1();
    let pub_key = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&sk));
    let sig_bytes = sign(&sk, &TEST_PAYLOAD);
    let sig = BytesN::<65>::from_array(&e, &sig_bytes);

    let long_hash = Bytes::from_array(&e, &[0xabu8; 64]);
    client.verify(&long_hash, &pub_key, &sig);
}

// ── verify: invalid recovery_id ──────────────────────────────────────────────

/// `recovery_id` must be 0 or 1. Value 2 must be rejected before calling into
/// the host.
#[test]
#[should_panic]
fn verify_rejects_invalid_recovery_id() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk = test_signing_key_1();
    let pub_key = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&sk));
    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);

    let mut sig_bytes = sign(&sk, &TEST_PAYLOAD);
    sig_bytes[64] = 2; // recovery_id must be 0 or 1
    let sig = BytesN::<65>::from_array(&e, &sig_bytes);

    client.verify(&hash, &pub_key, &sig);
}

// ── canonicalize_key tests ───────────────────────────────────────────────────

/// canonicalize_key is a pass-through for an already-uncompressed key.
#[test]
fn canonicalize_key_is_identity() {
    let e = Env::default();
    let client = register_verifier(&e);

    let sk = test_signing_key_1();
    let pub_key_bytes = uncompressed_pubkey(&sk);
    let pub_key = BytesN::<65>::from_array(&e, &pub_key_bytes);

    let canonical = client.canonicalize_key(&pub_key);

    assert_eq!(canonical, Bytes::from_array(&e, &pub_key_bytes));
    assert_eq!(canonical.len(), 65);
}

#[test]
fn canonicalize_key_distinct_keys_produce_distinct_output() {
    let e = Env::default();
    let client = register_verifier(&e);

    let key_a = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&test_signing_key_1()));
    let key_b = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&test_signing_key_2()));

    assert_ne!(client.canonicalize_key(&key_a), client.canonicalize_key(&key_b));
}

// ── batch_canonicalize_key tests ─────────────────────────────────────────────

/// Single-element batch result must match the scalar canonicalize_key result.
#[test]
fn batch_canonicalize_key_single_matches_canonicalize_key() {
    let e = Env::default();
    let client = register_verifier(&e);

    let key = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&test_signing_key_1()));
    let keys = Vec::from_array(&e, [key.clone()]);

    let batch_result = client.batch_canonicalize_key(&keys);
    let single_result = client.canonicalize_key(&key);

    assert_eq!(batch_result.len(), 1);
    assert_eq!(batch_result.get(0).unwrap(), single_result);
}

/// Input order must be preserved in the output.
#[test]
fn batch_canonicalize_key_preserves_order() {
    let e = Env::default();
    let client = register_verifier(&e);

    let key1 = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&test_signing_key_1()));
    let key2 = BytesN::<65>::from_array(&e, &uncompressed_pubkey(&test_signing_key_2()));

    let keys = Vec::from_array(&e, [key1.clone(), key2.clone()]);
    let canonical = client.batch_canonicalize_key(&keys);

    assert_eq!(canonical.len(), 2);
    assert_eq!(
        canonical.get(0).unwrap(),
        Bytes::from_array(&e, &uncompressed_pubkey(&test_signing_key_1()))
    );
    assert_eq!(
        canonical.get(1).unwrap(),
        Bytes::from_array(&e, &uncompressed_pubkey(&test_signing_key_2()))
    );
}

#[test]
fn batch_canonicalize_key_empty_input() {
    let e = Env::default();
    let client = register_verifier(&e);

    let keys: Vec<BytesN<65>> = Vec::new(&e);
    let result = client.batch_canonicalize_key(&keys);

    assert_eq!(result.len(), 0);
}
