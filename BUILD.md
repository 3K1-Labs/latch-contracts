# Build Artifacts

## Smart Account

| Field | Value |
|---|---|
| WASM file | `latch-smart-account/target/wasm32v1-none/release/smart_account.wasm` |
| WASM hash | `62c66784860e7a55ca90d59d34a9a90d1ad744042976908c62e61f5d0b0c1aed` |
| WASM size | 38,743 bytes (optimized from 43,240) |
| Built with | `stellar contract build --optimize` |
| Network | testnet |
| Uploaded by | `GBL4FMN3MPLPA2IS7T2K5VAGGVT4WJWJ24YXYFAHIFOGGCVEM6WVVAQA` (franky) |
| Upload tx | `9e2f38e88bf9a190e39c02b338fb5799755007b393fcbdb39f16a2235cd19220` |

### Exported Functions (16)

```
__check_auth
__constructor
add_context_rule
add_policy
add_signer
batch_add_signer
execute
get_context_rule
get_context_rules_count
get_policy_id
get_signer_id
remove_context_rule
remove_policy
remove_signer
update_context_rule_name
update_context_rule_valid_until
```

---

## Ed25519 Phantom Verifier

| Field | Value |
|---|---|
| WASM file | `latch-verifiers/ed25519-phantom-verifier/target/wasm32v1-none/release/ed25519_phantom_verifier.wasm` |
| WASM hash | `9246cfdd8c34e4453bd787195b61e286401727539675676ef28cfeae0320c561` |
| WASM size | 2,593 bytes (optimized from 2,984) |
| Built with | `stellar contract build --optimize` |
| Network | testnet |
| Deployed by | `GBL4FMN3MPLPA2IS7T2K5VAGGVT4WJWJ24YXYFAHIFOGGCVEM6WVVAQA` (franky) |
| Contract address | `CAD6GFOCK2ISL7TA6QAZFY4QICS2AWSETXIKIACNSCPGXGOK7WOIME4U` |
| Upload tx | `e6d8e4012708d72dec09df54c39ee0cbb366fd14703ba939a1c6d57dc402e422` |
| Deploy tx | `81b790d29a58cd2d660181bfc43e76c1689ec71d04eddea25f663169ba121e22` |

### Exported Functions (3)

```
verify
canonicalize_key
batch_canonicalize_key
```

---

## WebAuthn Verifier

| Field | Value |
|---|---|
| WASM file | `latch-verifiers/webauthn-verifier/target/wasm32v1-none/release/webauthn_verifier.wasm` |
| WASM hash | `16ff1c9f4070007f6a296b2e234a8e0d3bceaf08ab0e6cdf03fead656519fb78` |
| WASM size | 11,096 bytes (optimized from 12,949) |
| Built with | `stellar contract build --optimize` |
| Network | testnet |
| Deployed by | `GBL4FMN3MPLPA2IS7T2K5VAGGVT4WJWJ24YXYFAHIFOGGCVEM6WVVAQA` (franky) |
| Contract address | `CDBBGLSWWHWK52REY7GK5HWAQGAJJ4GP5O75LOM3F4INN6W4KT6DPBVY` |
| Upload tx | `cb31f24e556942b1e5a390babcf81bd7707eb56365b4f56a6ab88d81a88efd9f` |
| Deploy tx | `3939cf837692cd4bac65be6482980c3a93365ee09062501b97cae75d059ab806` |

### Exported Functions (3)

```
verify
canonicalize_key
batch_canonicalize_key
```

---

## Secp256k1 Verifier (stub)

> **Placeholder only, and no longer wired into the factory.** `verify` panics with `NotImplemented`. `Secp256k1` was removed from the factory's `SignerKind` — this deployment is unused by any current factory instance.

| Field | Value |
|---|---|
| WASM file | `latch-verifiers/secp256k1-verifier/target/wasm32v1-none/release/secp256k1_verifier.wasm` |
| WASM hash | `3be1eeeee47d93a458e3cac725a415ead07fd5742ff2bff1f2d62d1cd27b534e` |
| WASM size | 840 bytes (optimized from 922) |
| Built with | `stellar contract build --optimize` |
| Network | testnet |
| Deployed by | `GBL4FMN3MPLPA2IS7T2K5VAGGVT4WJWJ24YXYFAHIFOGGCVEM6WVVAQA` (franky) |
| Contract address | `CCWT7H2WDUMTQDOBWHTGLMB3H23B34L2RDRWVOD3PQZOR2MHZHIRSWKB` |
| Upload tx | `7685ecc87a1d11bc390d4a508f7f5df02affb5bc62842511a7a1600414571130` |
| Deploy tx | `486f8f0ca6a86191e18b8ae961778b46b016605076b78898e232a1bf7a2afea5` |

### Exported Functions (3)

```
verify
canonicalize_key
batch_canonicalize_key
```

---

## Threshold Policy

| Field | Value |
|---|---|
| WASM file | `latch-threshold-policy/target/wasm32v1-none/release/threshold_policy.wasm` |
| WASM hash | `5c3d8acccfd37b03b75aaa183c5ea4d5b615c907dfc84c5db148f899484c3d47` |
| WASM size | 8,672 bytes (optimized from 9,526) |
| Built with | `stellar contract build --optimize` |
| Network | testnet |
| Deployed by | `GBL4FMN3MPLPA2IS7T2K5VAGGVT4WJWJ24YXYFAHIFOGGCVEM6WVVAQA` (franky) |
| Contract address | `CAILIN6YJ5A73VPVHF35XAOESBNBLXOV7I7VZHYI2Q24EZTSQJ2UTFIL` |
| Upload tx | `2d8e9ce41b6f282b0b97e22f8fb59290330d5fd561b3454218e796f0f83ea871` |
| Deploy tx | `ef1fa9ac1c57dc068f31360be09530356ec3029192a2e8296ad38fbe1f37578d` |

### Exported Functions (5)

```
enforce
install
uninstall
get_threshold
set_threshold
```

---

## Account Factory

> **Stale.** This deployment predates the removal of `Secp256k1` from the factory's
> `SignerKind`/`FactoryConfig`/constructor (5 args → 4 args, secp256k1 verifier dropped). The wasm
> hash and constructor args below no longer match current source. Redeploy before relying on this
> record.

| Field | Value |
|---|---|
| WASM file | `account-factory/target/wasm32v1-none/release/factory_contract.wasm` |
| WASM hash | `56cc40058ff623fedf62b94dfa29380d3cd218860da8439d4c00de0017a68856` |
| WASM size | 8,148 bytes (optimized from 9,500) |
| Built with | `stellar contract build --optimize` |
| Network | testnet |
| Deployed by | `GBL4FMN3MPLPA2IS7T2K5VAGGVT4WJWJ24YXYFAHIFOGGCVEM6WVVAQA` (franky) |
| Contract address | `CABA6ERCFZRZXZV5EYQCFB5AQMHMAIEOB5TCN7JWHMK3HWUZJCJNUZRR` |
| Upload tx | `74abb8d7cae12b07a2400db14706c63d4a00074c09f1be0a8e3b02d7efc9cc6d` |
| Deploy tx | `a2d99f1c79f0d36471c758e5c03afbb744313ae005f00f2d815e01734de5d900` |

### Constructor Arguments

| Argument | Value |
|---|---|
| `smart_account_wasm_hash` | `62c66784860e7a55ca90d59d34a9a90d1ad744042976908c62e61f5d0b0c1aed` |
| `ed25519_verifier` | `CAD6GFOCK2ISL7TA6QAZFY4QICS2AWSETXIKIACNSCPGXGOK7WOIME4U` |
| `secp256k1_verifier` | `CCWT7H2WDUMTQDOBWHTGLMB3H23B34L2RDRWVOD3PQZOR2MHZHIRSWKB` (stub) |
| `webauthn_verifier` | `CDBBGLSWWHWK52REY7GK5HWAQGAJJ4GP5O75LOM3F4INN6W4KT6DPBVY` |
| `threshold_policy` | `CAILIN6YJ5A73VPVHF35XAOESBNBLXOV7I7VZHYI2Q24EZTSQJ2UTFIL` |

### Exported Functions (5)

```
__constructor
create_account
get_account_address
get_threshold_policy
get_verifier
```
