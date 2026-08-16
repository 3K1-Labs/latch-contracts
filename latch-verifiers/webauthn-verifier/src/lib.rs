#![no_std]

use soroban_sdk::{contract, contractimpl, xdr::FromXdr, Bytes, BytesN, Env, Vec};
use stellar_accounts::verifiers::{
    utils::extract_from_bytes,
    webauthn::{self, WebAuthnSigData},
    Verifier,
};

#[contract]
pub struct WebAuthnVerifier;

#[contractimpl]
impl Verifier for WebAuthnVerifier {
    type KeyData = Bytes;
    type SigData = Bytes;

    /// Verify a WebAuthn authentication assertion against a Soroban auth
    /// payload hash.
    ///
    /// # Arguments
    ///
    /// * `hash` - The 32-byte Soroban auth payload hash. The client
    ///   base64url-encodes this and embeds it as the `challenge` field in
    ///   `clientDataJSON`.
    /// * `key_data` - Bytes containing a 65-byte uncompressed secp256r1 public
    ///   key (0x04 prefix + 32-byte X + 32-byte Y) followed by an optional
    ///   variable-length credential ID suffix. The credential ID is client-side
    ///   metadata and is stripped by `canonicalize_key`.
    /// * `sig_data` - XDR-encoded `WebAuthnSigData` struct containing:
    ///   - `signature`: 64-byte compact P-256 signature (r || s)
    ///   - `authenticator_data`: raw authenticator data bytes (≥37 bytes)
    ///   - `client_data`: raw clientDataJSON bytes
    ///
    /// # Returns
    ///
    /// `true` if the assertion is valid. Panics on any verification failure.
    fn verify(e: &Env, hash: Bytes, key_data: Bytes, sig_data: Bytes) -> bool {
        let sig_struct = WebAuthnSigData::from_xdr(e, &sig_data)
            .expect("sig_data must be a valid XDR-encoded WebAuthnSigData");

        let pub_key: BytesN<65> = extract_from_bytes(e, &key_data, 0..65)
            .expect("key_data must contain a 65-byte secp256r1 public key at offset 0");

        webauthn::verify(e, &hash, &pub_key, &sig_struct)
    }

    /// Returns the canonical 65-byte public key, stripping any credential ID
    /// suffix.
    ///
    /// Two registrations of the same P-256 key with different credential IDs
    /// must produce identical canonical output — this enables the smart
    /// account to detect and reject duplicate signers regardless of which
    /// device was used to register.
    ///
    /// Panics with `Error(Contract, #3119)` if `key_data` is shorter than 65
    /// bytes.
    fn canonicalize_key(e: &Env, key_data: Bytes) -> Bytes {
        webauthn::canonicalize_key(e, &key_data)
    }

    /// Canonicalizes a batch of WebAuthn keys, preserving input order.
    fn batch_canonicalize_key(e: &Env, key_data: Vec<Bytes>) -> Vec<Bytes> {
        webauthn::batch_canonicalize_key(e, &key_data)
    }
}

#[cfg(test)]
mod test;
