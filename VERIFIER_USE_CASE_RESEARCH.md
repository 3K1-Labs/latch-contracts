# Smart-Account Verifier Use-Case Research

This note explores which signing and verification methods are genuinely useful for Latch smart accounts. The goal is not to implement verifiers merely because the cryptographic schemes exist, but to connect each verifier to a concrete wallet capability and contributor-ready scope.

## Executive conclusion

The most useful next verifier is a **raw secp256r1 (P-256) verifier for browser and native-mobile session keys**.

The durable account owner can remain a WebAuthn/passkey signer. That passkey authorizes creation of a separate, ephemeral P-256 session signer. The session signer is then constrained by:

- a `CallContract` context rule targeting one dApp;
- the session policy's function allowlist;
- an on-chain `valid_until` ledger;
- optional spending or other policies; and
- early removal through `remove_context_rule`.

This separation avoids requiring a passkey ceremony for every session action and avoids treating a synchronized passkey as an ephemeral credential.

## Passkey management and deletion

Users can inspect and delete passkeys stored in Google Password Manager or Chrome. A passkey may instead be stored in iCloud Keychain, Windows Hello, a hardware security key, or another credential manager.

However, a website cannot reliably delete a private passkey from the user's authenticator or password manager. It can revoke the credential by ceasing to accept its public key. Physical deletion remains a user/device operation.

For Latch, this is not a blocker. A session credential does not need to be physically erased before it becomes harmless. Authorization ends when its context rule expires or is removed on-chain. Local deletion is additional security hygiene rather than the authoritative security boundary.

References:

- [Manage passkeys in Chrome](https://support.google.com/chrome/answer/13168025?hl=en-GB)
- [Google passkey user journeys](https://developers.google.com/identity/passkeys/ux/user-journeys)
- [WebAuthn specification](https://www.w3.org/TR/webauthn-1/)
- [`stellar-accounts` 0.7.2 documentation](https://docs.rs/crate/stellar-accounts/0.7.2)

## Recommended session-key architecture

```text
Durable WebAuthn/passkey owner
              |
              | one user-approved authorization
              v
Create an expiring context rule
              |
              +-- target: one dApp contract
              +-- methods: session-policy allowlist
              +-- signer: ephemeral raw P-256 public key
              +-- valid_until: expiry ledger
              |
              v
dApp signs permitted actions during the session
```

### Browser flow

1. Generate a raw P-256 key using Web Crypto.
2. Make the private `CryptoKey` non-extractable.
3. Store the `CryptoKey` in IndexedDB.
4. Add the public key as `Signer::External(raw_p256_verifier, public_key)` on a new context rule.
5. Use the session key to sign Soroban authorization payloads without another WebAuthn ceremony.
6. Delete the IndexedDB entry on logout and request early on-chain revocation when practical.
7. Treat `valid_until` as the hard boundary if local cleanup or revocation submission fails.

Web Crypto permits `CryptoKey` objects to be serialized into IndexedDB without exporting the underlying key material. See [MDN's Web Crypto documentation](https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto).

A non-extractable key still does not protect against malicious JavaScript running in the same origin: such code can request signatures while it is active. On-chain restrictions on target, function, duration, and value therefore remain essential.

### Native-mobile flow

Raw P-256 also maps naturally to mobile hardware. Apple's Secure Enclave supports hardware-protected P-256 signing keys whose private material does not leave the enclave. See [Apple's Secure Enclave documentation](https://developer.apple.com/documentation/Security/protecting-keys-with-the-secure-enclave).

Equivalent Android work should evaluate hardware-backed Android Keystore P-256 keys during the client spike.

## Why raw P-256 is different from WebAuthn

| WebAuthn verifier | Raw P-256 verifier |
|---|---|
| Validates a complete WebAuthn assertion | Validates an ECDSA signature |
| Includes RP ID, authenticator flags, and client-data JSON | Signs the Soroban authorization digest directly |
| Normally has a user-presence or user-verification ceremony | Can sign silently according to local key policy |
| Best suited to durable owner authentication | Best suited to app, device, and session keys |

Soroban already provides native secp256r1 verification, so the verifier can be a narrow adapter rather than a custom cryptographic implementation. See the [Soroban SDK crypto API](https://docs.rs/soroban-sdk/latest/soroban_sdk/crypto/struct.Crypto.html).

The factory does not initially need to support raw P-256. The factory controls initial account signers, while a session signer can be installed later through an account context rule using the verifier's deployed address. Factory integration should be considered separately if raw P-256 becomes a supported initial owner type.

## Evaluation of other verifier families

### Secp256k1: EVM-wallet interoperability

The concrete use case is controlling or co-signing a Latch account with an existing MetaMask, Ethereum hardware-wallet, or other EVM key.

This is useful interoperability infrastructure, but it is not blocking the smart-account core. The existing issue should be re-scoped around an end-to-end wallet experiment that settles:

- personal-sign versus typed-data signing;
- domain separation and cross-chain replay protection;
- low-S and other malleability requirements;
- public-key recovery versus storing a complete public key; and
- actual injected-wallet UX.

### ZK proofs: prove an authority, not merely a signature

A generic "ZK signer" is under-specified. Every proof-backed authorization must say what fact gives the prover authority over the requested account action.

Promising Latch-specific uses include:

- private email-guardian recovery;
- anonymous membership in an approved organization or guardian set;
- proof of eligibility such as KYC status, age, residency, or employment without revealing the underlying identity; and
- proof of control of an approved Web2 identity or JWT.

The strongest wallet use case is private email-guardian recovery. A guardian could authorize a new recovery key through a DKIM-authenticated email command without publishing the guardian's email address.

The expected flow is:

1. A recovery client obtains email or identity evidence.
2. It generates a proof off-chain.
3. Public inputs bind the proof to the Latch account, network, intended action, new public key, nonce, and expiry.
4. The proof is supplied as signer `sig_data`.
5. A Soroban verifier validates it.
6. A narrowly scoped recovery rule authorizes only the recovery transition.

Recovery requires more than a verifier crate. The recovery rule must be installed while the account is healthy so it can rotate owner signers when the normal owner key is unavailable. Guardian thresholds, delay, cancellation, expiry, and replay protection belong in the recovery design.

Stellar has credible primitives for an experiment: BLS12-381 host functions are Final under CAP-0059, protocol 25 added BN254 and Poseidon functionality, and Stellar's examples include Groth16, BN254, and BLS demonstrations. The examples are educational and unaudited, so this should start as a design and benchmarking spike.

References:

- [ZK Email recovery](https://github.com/zkemail/email-recovery)
- [Stellar CAP status](https://github.com/stellar/stellar-protocol/blob/master/core/README.md)
- [Soroban examples](https://github.com/stellar/soroban-examples)

### BLS: large-group aggregation

BLS is potentially useful for:

- institutional treasury committees;
- validator or operator committees;
- large guardian sets; and
- organizations that need one compact authorization from many participants.

For two or three wallet signers, the existing threshold policy is simpler. BLS becomes compelling only when aggregation materially reduces payload and verification overhead. Any research must include proof of possession, rogue-key-attack protection, signer-set commitments, aggregation semantics, and benchmarks against the existing threshold policy.

### RSA: require a named integration first

Potential uses are legacy institutional HSMs, enterprise PKI, and direct DKIM verification. RSA keys and signatures are large, and Soroban does not expose an ordinary high-level RSA verification host function comparable to Ed25519 or P-256.

For email recovery, a ZK-email construction is likely preferable to processing full RSA/DKIM material on-chain. For institutional custody, first identify a real custodian or HSM that cannot provide P-256 or Ed25519. A standalone RSA implementation issue should not be opened without that evidence.

### Email is a flow, not a standalone curve

Email authentication may combine:

- DKIM using RSA or Ed25519;
- a ZK circuit protecting addresses and content;
- a replay-preventing nullifier;
- recovery threshold, delay, and cancellation policy; and
- a proof verifier.

Therefore email, RSA, and ZK should not automatically become three independent implementation issues. First define the complete recovery architecture and let that design reveal the actual components.

## Recommended contributor roadmap

### 1. Raw P-256 session-key initiative

Create a parent issue for the end-to-end capability, followed by independently reviewable child issues:

- verifier contract and adversarial tests;
- browser key lifecycle proof of concept;
- end-to-end context-rule/session integration test;
- security and client-lifecycle documentation; and
- resource and WASM-size benchmarks.

The parent is an initiative rather than a one-PR Wave task. Each child should carry a clear complexity recommendation and acceptance criteria.

### 2. Re-scope secp256k1 as an interoperability spike

Use a research issue or discussion before implementation. Its output should be a chosen signing envelope and a working injected-wallet prototype.

### 3. Private email-guardian recovery discussion

Open a design discussion before implementation issues. Resolve authority, initialization, recovery/cancellation state transitions, public inputs, privacy goals, proving stack, and expected costs.

### 4. BLS institutional-authorization research

Begin with a benchmark/design discussion. Compare the current threshold policy against BLS at realistic signer counts before accepting a verifier implementation.

### 5. Defer standalone RSA and generic email-verifier issues

Only create them when a concrete integration or accepted recovery design requires them.

## Drips Wave issue-design guidance

Contributor issues should be independently mergeable and small enough for one contributor to complete within a Wave. Parent initiatives and open-ended architecture discussions should not themselves be enrolled as Wave implementation tasks.

Recommended interpretation of the Drips levels:

| Complexity | Points | Appropriate Latch scope |
|---|---:|---|
| Trivial | 100 | Focused docs, typo, small test assertion, or bounded cleanup |
| Medium | 150 | Narrow verifier adapter, standard test expansion, client proof of concept with settled architecture |
| High | 200 | Security-sensitive integration, new cryptographic verifier with extensive negative tests, or cross-contract lifecycle feature |

Each Wave issue should include:

- user-facing motivation;
- exact in-scope and out-of-scope work;
- prerequisites and dependencies;
- objective acceptance criteria;
- required positive and adversarial tests;
- relevant repository paths and upstream references;
- security considerations;
- expected deliverables; and
- a recommended Drips complexity with justification.

Do not label an unresolved design discussion as a contributor implementation task. Convert an accepted design into smaller implementation issues, then add only those child issues to the Wave.
