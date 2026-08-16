#![cfg(test)]

extern crate std;

use p256::{
    ecdsa::{signature::hazmat::PrehashSigner, Signature as P256Signature, SigningKey},
    SecretKey,
};
use soroban_sdk::{xdr::ToXdr, Bytes, BytesN, Env, Vec};
use stellar_accounts::verifiers::webauthn::{
    WebAuthnSigData, AUTH_DATA_FLAGS_BE, AUTH_DATA_FLAGS_BS, AUTH_DATA_FLAGS_UP, AUTH_DATA_FLAGS_UV,
};

use super::{WebAuthnVerifier, WebAuthnVerifierClient};

// ── test keypair ─────────────────────────────────────────────────────────────

/// Returns a deterministic P-256 keypair for testing.
/// Same seed used across all tests for reproducibility.
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

/// Extracts the 65-byte uncompressed public key from a signing key.
fn public_key_bytes(signing_key: &SigningKey) -> [u8; 65] {
    let point = signing_key.verifying_key().to_encoded_point(false);
    let bytes = point.as_bytes();
    let mut out = [0u8; 65];
    out.copy_from_slice(bytes);
    out
}

// ── test payload ─────────────────────────────────────────────────────────────

const TEST_PAYLOAD: [u8; 32] = [
    0x4b, 0xb7, 0xa8, 0xb9, 0x96, 0x09, 0xb0, 0xb8, 0xb1, 0xd5, 0x34, 0x69, 0x4b, 0xb1, 0xf3, 0x1f,
    0x12, 0x91, 0x38, 0xa2, 0xf2, 0xa1, 0x1f, 0x8e, 0x87, 0x02, 0xee, 0xdb, 0xb7, 0x92, 0x92, 0x2e,
];

// ── WebAuthn assertion builders ──────────────────────────────────────────────

/// Encodes a 32-byte payload as base64url (no padding).
/// Produces the `challenge` string that goes inside clientDataJSON.
fn base64url_encode(src: &[u8; 32]) -> [u8; 43] {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut dst = [0u8; 43];
    let mut di = 0;
    let mut si = 0;
    let n = (src.len() / 3) * 3;
    while si < n {
        let val = (src[si] as usize) << 16 | (src[si + 1] as usize) << 8 | (src[si + 2] as usize);
        dst[di] = ALPHABET[val >> 18 & 0x3F];
        dst[di + 1] = ALPHABET[val >> 12 & 0x3F];
        dst[di + 2] = ALPHABET[val >> 6 & 0x3F];
        dst[di + 3] = ALPHABET[val & 0x3F];
        si += 3;
        di += 4;
    }
    let remain = src.len() - si;
    if remain > 0 {
        let mut val = (src[si] as usize) << 16;
        if remain == 2 {
            val |= (src[si + 1] as usize) << 8;
        }
        dst[di] = ALPHABET[val >> 18 & 0x3F];
        dst[di + 1] = ALPHABET[val >> 12 & 0x3F];
        if remain == 2 {
            dst[di + 2] = ALPHABET[val >> 6 & 0x3F];
        }
    }
    dst
}

/// Builds minimal 37-byte authenticatorData with the given flags byte.
/// Layout: 32-byte rpIdHash (zeroed) | 1-byte flags | 4-byte counter (zeroed)
fn build_authenticator_data(flags: u8) -> std::vec::Vec<u8> {
    let mut data = std::vec![0u8; 37];
    data[32] = flags;
    data
}

/// Builds clientDataJSON with the given challenge string and type field.
fn build_client_data(challenge: &str, type_field: &str) -> std::vec::Vec<u8> {
    let json = std::format!(
        r#"{{"type":"{type_field}","challenge":"{challenge}","origin":"https://example.com","crossOrigin":false}}"#
    );
    json.into_bytes()
}

/// Signs a WebAuthn assertion over authenticatorData + clientDataJSON using a
/// P-256 key.
///
/// Step 19-20 of WebAuthn §7.2:
///   message = authenticatorData || SHA-256(clientData)
///   digest  = SHA-256(message)
///   sig     = ECDSA_P256(private_key, digest)
fn sign_assertion(
    e: &Env,
    signing_key: &SigningKey,
    authenticator_data: &[u8],
    client_data: &[u8],
) -> [u8; 64] {
    let client_data_hash = e.crypto().sha256(&Bytes::from_slice(e, client_data)).to_array();

    let mut message = std::vec::Vec::new();
    message.extend_from_slice(authenticator_data);
    message.extend_from_slice(&client_data_hash);

    let digest = e.crypto().sha256(&Bytes::from_slice(e, &message)).to_array();

    let signature: P256Signature = signing_key.sign_prehash(&digest).unwrap();
    // normalize_s ensures low-S form required by Stellar secp256r1_verify
    let normalized = signature.normalize_s().unwrap_or(signature);
    let sig_bytes = normalized.to_bytes();

    let mut out = [0u8; 64];
    out.copy_from_slice(&sig_bytes);
    out
}

/// Builds a complete, valid WebAuthn sig_data for the given payload and flags.
fn build_sig_data(
    e: &Env,
    signing_key: &SigningKey,
    payload: &[u8; 32],
    flags: u8,
    challenge_override: Option<&str>,
    type_override: Option<&str>,
) -> Bytes {
    let encoded_challenge = base64url_encode(payload);
    let challenge_str =
        challenge_override.unwrap_or_else(|| std::str::from_utf8(&encoded_challenge).unwrap());
    let type_str = type_override.unwrap_or("webauthn.get");

    let authenticator_data = build_authenticator_data(flags);
    let client_data = build_client_data(challenge_str, type_str);
    let signature = sign_assertion(e, signing_key, &authenticator_data, &client_data);

    let sig_struct = WebAuthnSigData {
        signature: BytesN::<64>::from_array(e, &signature),
        authenticator_data: Bytes::from_slice(e, &authenticator_data),
        client_data: Bytes::from_slice(e, &client_data),
    };

    sig_struct.to_xdr(e)
}

/// Standard valid flags: UP + UV + BE + BS all set.
fn valid_flags() -> u8 {
    AUTH_DATA_FLAGS_UP | AUTH_DATA_FLAGS_UV | AUTH_DATA_FLAGS_BE | AUTH_DATA_FLAGS_BS
}

fn register_verifier(e: &Env) -> WebAuthnVerifierClient<'_> {
    let addr = e.register(WebAuthnVerifier, ());
    WebAuthnVerifierClient::new(e, &addr)
}

// ── verify: happy path ───────────────────────────────────────────────────────

#[test]
fn verify_valid_webauthn_assertion() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);
    let sig_data = build_sig_data(&e, &signing_key, &TEST_PAYLOAD, valid_flags(), None, None);

    assert!(client.verify(&hash, &key_data, &sig_data));
}

#[test]
fn verify_valid_assertion_with_credential_id_suffix() {
    // key_data = pubkey (65 bytes) + credential_id (16 bytes)
    // The verifier must extract only the first 65 bytes and ignore the rest.
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);

    let mut key_data_with_suffix = std::vec::Vec::new();
    key_data_with_suffix.extend_from_slice(&pub_key_bytes);
    key_data_with_suffix.extend_from_slice(&[0xAB_u8; 16]); // 16-byte credential ID
    let key_data = Bytes::from_slice(&e, &key_data_with_suffix);

    let sig_data = build_sig_data(&e, &signing_key, &TEST_PAYLOAD, valid_flags(), None, None);

    assert!(client.verify(&hash, &key_data, &sig_data));
}

// ── verify: challenge failures ───────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #3114)")]
fn verify_rejects_wrong_challenge() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);

    // Sign over a different challenge
    let sig_data = build_sig_data(
        &e,
        &signing_key,
        &TEST_PAYLOAD,
        valid_flags(),
        Some("d3JvbmctY2hhbGxlbmdl"), // base64url("wrong-challenge")
        None,
    );

    client.verify(&hash, &key_data, &sig_data);
}

// ── verify: type field failures ──────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #3113)")]
fn verify_rejects_wrong_type_field() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);

    // "webauthn.create" is registration — not valid for authentication
    let sig_data = build_sig_data(
        &e,
        &signing_key,
        &TEST_PAYLOAD,
        valid_flags(),
        None,
        Some("webauthn.create"),
    );

    client.verify(&hash, &key_data, &sig_data);
}

// ── verify: authenticator data flag failures ─────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #3116)")]
fn verify_rejects_up_flag_not_set() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);

    // UV set but UP cleared
    let flags = AUTH_DATA_FLAGS_UV;
    let sig_data = build_sig_data(&e, &signing_key, &TEST_PAYLOAD, flags, None, None);

    client.verify(&hash, &key_data, &sig_data);
}

#[test]
#[should_panic(expected = "Error(Contract, #3117)")]
fn verify_rejects_uv_flag_not_set() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);

    // UP set but UV cleared
    let flags = AUTH_DATA_FLAGS_UP;
    let sig_data = build_sig_data(&e, &signing_key, &TEST_PAYLOAD, flags, None, None);

    client.verify(&hash, &key_data, &sig_data);
}

#[test]
#[should_panic(expected = "Error(Contract, #3118)")]
fn verify_rejects_invalid_backup_state() {
    // BS=1 without BE=1 is an invalid authenticator state per WebAuthn spec.
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);

    // UP + UV + BS set, but BE not set
    let flags = AUTH_DATA_FLAGS_UP | AUTH_DATA_FLAGS_UV | AUTH_DATA_FLAGS_BS;
    let sig_data = build_sig_data(&e, &signing_key, &TEST_PAYLOAD, flags, None, None);

    client.verify(&hash, &key_data, &sig_data);
}

// ── verify: structural failures ──────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #3115)")]
fn verify_rejects_authenticator_data_too_short() {
    // authenticatorData must be at least 37 bytes.
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);

    let encoded_challenge = base64url_encode(&TEST_PAYLOAD);
    let challenge_str = std::str::from_utf8(&encoded_challenge).unwrap();

    let short_auth_data = std::vec![0u8; 35]; // 35 bytes — too short
    let client_data = build_client_data(challenge_str, "webauthn.get");
    let signature = sign_assertion(&e, &signing_key, &short_auth_data, &client_data);

    let sig_struct = WebAuthnSigData {
        signature: BytesN::<64>::from_array(&e, &signature),
        authenticator_data: Bytes::from_slice(&e, &short_auth_data),
        client_data: Bytes::from_slice(&e, &client_data),
    };
    let sig_data = sig_struct.to_xdr(&e);

    client.verify(&hash, &key_data, &sig_data);
}

#[test]
#[should_panic(expected = "Error(Contract, #3111)")]
fn verify_rejects_client_data_too_long() {
    // clientDataJSON must be ≤ 1024 bytes.
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);

    let encoded_challenge = base64url_encode(&TEST_PAYLOAD);
    let challenge_str = std::str::from_utf8(&encoded_challenge).unwrap();

    // Pad the origin to push total JSON length over 1024
    let long_origin = "x".repeat(1100);
    let long_client_data = std::format!(
        r#"{{"type":"webauthn.get","challenge":"{challenge_str}","origin":"{long_origin}","crossOrigin":false}}"#
    ).into_bytes();

    let auth_data = build_authenticator_data(valid_flags());
    let signature = sign_assertion(&e, &signing_key, &auth_data, &long_client_data);

    let sig_struct = WebAuthnSigData {
        signature: BytesN::<64>::from_array(&e, &signature),
        authenticator_data: Bytes::from_slice(&e, &auth_data),
        client_data: Bytes::from_slice(&e, &long_client_data),
    };
    let sig_data = sig_struct.to_xdr(&e);

    client.verify(&hash, &key_data, &sig_data);
}

#[test]
#[should_panic(expected = "Error(Contract, #3112)")]
fn verify_rejects_malformed_client_data_json() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);

    // Truncated JSON — missing closing brace
    let bad_client_data = br#"{"type":"webauthn.get","challenge":"test""#.to_vec();
    let auth_data = build_authenticator_data(valid_flags());
    let signature = sign_assertion(&e, &signing_key, &auth_data, &bad_client_data);

    let sig_struct = WebAuthnSigData {
        signature: BytesN::<64>::from_array(&e, &signature),
        authenticator_data: Bytes::from_slice(&e, &auth_data),
        client_data: Bytes::from_slice(&e, &bad_client_data),
    };
    let sig_data = sig_struct.to_xdr(&e);

    client.verify(&hash, &key_data, &sig_data);
}

// ── verify: cryptographic failures ───────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_corrupted_signature() {
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);

    let encoded_challenge = base64url_encode(&TEST_PAYLOAD);
    let challenge_str = std::str::from_utf8(&encoded_challenge).unwrap();

    let auth_data = build_authenticator_data(valid_flags());
    let client_data = build_client_data(challenge_str, "webauthn.get");
    let mut signature = sign_assertion(&e, &signing_key, &auth_data, &client_data);
    signature[0] = signature[0].wrapping_add(1); // corrupt

    let sig_struct = WebAuthnSigData {
        signature: BytesN::<64>::from_array(&e, &signature),
        authenticator_data: Bytes::from_slice(&e, &auth_data),
        client_data: Bytes::from_slice(&e, &client_data),
    };
    let sig_data = sig_struct.to_xdr(&e);

    client.verify(&hash, &key_data, &sig_data);
}

#[test]
#[should_panic(expected = "Error(Crypto, InvalidInput)")]
fn verify_rejects_wrong_key() {
    // Sign with key_1, present key_2's public key — must fail.
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let wrong_signing_key = test_signing_key_2();
    let wrong_pub_key_bytes = public_key_bytes(&wrong_signing_key);

    let hash = Bytes::from_array(&e, &TEST_PAYLOAD);
    let wrong_key_data = Bytes::from_slice(&e, &wrong_pub_key_bytes);

    let sig_data = build_sig_data(&e, &signing_key, &TEST_PAYLOAD, valid_flags(), None, None);

    client.verify(&hash, &wrong_key_data, &sig_data);
}

#[test]
#[should_panic(expected = "Error(Contract, #3114)")]
fn verify_rejects_wrong_payload() {
    // Assertion signed over TEST_PAYLOAD, presented with a different hash.
    // The challenge in clientDataJSON encodes TEST_PAYLOAD — when the verifier
    // reconstructs the expected challenge from `different_hash`, they won't match.
    // Challenge validation (step 4) fires before the crypto check (step 10).
    let e = Env::default();
    let client = register_verifier(&e);
    let signing_key = test_signing_key();
    let pub_key_bytes = public_key_bytes(&signing_key);

    let different_hash = Bytes::from_array(&e, &[0xFFu8; 32]);
    let key_data = Bytes::from_slice(&e, &pub_key_bytes);

    // sig_data was built for TEST_PAYLOAD — challenge won't match different_hash
    let sig_data = build_sig_data(&e, &signing_key, &TEST_PAYLOAD, valid_flags(), None, None);

    client.verify(&different_hash, &key_data, &sig_data);
}

// ── canonicalize_key tests
// ────────────────────────────────────────────────────

#[test]
fn canonicalize_key_strips_credential_id_suffix() {
    let e = Env::default();
    let client = register_verifier(&e);

    let pub_key = [0x04_u8; 65];
    let mut key_with_suffix = std::vec::Vec::new();
    key_with_suffix.extend_from_slice(&pub_key);
    key_with_suffix.extend_from_slice(&[0xBE_u8; 20]); // 20-byte credential ID

    let key_data = Bytes::from_slice(&e, &key_with_suffix);
    let canonical = client.canonicalize_key(&key_data);

    assert_eq!(canonical, Bytes::from_slice(&e, &pub_key));
    assert_eq!(canonical.len(), 65);
}

#[test]
fn canonicalize_key_same_pubkey_different_credential_id_produces_same_output() {
    let e = Env::default();
    let client = register_verifier(&e);

    let pub_key = [0x04_u8; 65];

    let mut key_a = std::vec::Vec::new();
    key_a.extend_from_slice(&pub_key);
    key_a.extend_from_slice(&[0x11_u8; 16]);

    let mut key_b = std::vec::Vec::new();
    key_b.extend_from_slice(&pub_key);
    key_b.extend_from_slice(&[0x22_u8; 32]);

    let canonical_a = client.canonicalize_key(&Bytes::from_slice(&e, &key_a));
    let canonical_b = client.canonicalize_key(&Bytes::from_slice(&e, &key_b));

    assert_eq!(canonical_a, canonical_b);
}

#[test]
fn canonicalize_key_exact_65_bytes_passes() {
    let e = Env::default();
    let client = register_verifier(&e);

    let pub_key = [0x04_u8; 65];
    let key_data = Bytes::from_slice(&e, &pub_key);
    let canonical = client.canonicalize_key(&key_data);

    assert_eq!(canonical, key_data);
}

#[test]
#[should_panic(expected = "Error(Contract, #3119)")]
fn canonicalize_key_rejects_short_input() {
    let e = Env::default();
    let client = register_verifier(&e);

    let short_key = Bytes::from_slice(&e, &[0x04_u8; 64]); // 64 bytes — one short
    client.canonicalize_key(&short_key);
}

// ── batch_canonicalize_key tests
// ──────────────────────────────────────────────

#[test]
fn batch_canonicalize_key_preserves_order() {
    let e = Env::default();
    let client = register_verifier(&e);

    let pub1 = [0x04_u8; 65];
    let pub2 = [0x05_u8; 65];
    let pub3 = [0x06_u8; 65];

    let mut k1 = std::vec::Vec::new();
    k1.extend_from_slice(&pub1);
    k1.extend_from_slice(&[0xAA_u8; 10]);

    let mut k2 = std::vec::Vec::new();
    k2.extend_from_slice(&pub2);
    k2.extend_from_slice(&[0xBB_u8; 20]);

    // k3 has no credential ID suffix
    let k3 = pub3.to_vec();

    let keys = Vec::from_array(
        &e,
        [Bytes::from_slice(&e, &k1), Bytes::from_slice(&e, &k2), Bytes::from_slice(&e, &k3)],
    );

    let canonical = client.batch_canonicalize_key(&keys);

    assert_eq!(canonical.len(), 3);
    assert_eq!(canonical.get(0).unwrap(), Bytes::from_slice(&e, &pub1));
    assert_eq!(canonical.get(1).unwrap(), Bytes::from_slice(&e, &pub2));
    assert_eq!(canonical.get(2).unwrap(), Bytes::from_slice(&e, &pub3));
}

#[test]
fn batch_canonicalize_key_single_matches_canonicalize_key() {
    let e = Env::default();
    let client = register_verifier(&e);

    let pub_key = [0x04_u8; 65];
    let mut key_with_suffix = std::vec::Vec::new();
    key_with_suffix.extend_from_slice(&pub_key);
    key_with_suffix.extend_from_slice(&[0xCC_u8; 8]);

    let key_data = Bytes::from_slice(&e, &key_with_suffix);
    let keys = Vec::from_array(&e, [key_data.clone()]);

    let batch_result = client.batch_canonicalize_key(&keys);
    let single_result = client.canonicalize_key(&key_data);

    assert_eq!(batch_result.get(0).unwrap(), single_result);
}
