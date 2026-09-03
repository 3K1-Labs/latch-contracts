# Build Artifacts

Deployment records for contracts actually live on a network right now. Testnet churns fast during
active development — an entry here is deleted and replaced the moment its contract is redeployed,
not preserved as history. Every entry points at a physically archived copy of the exact binary
under `deployments/artifacts/<contract>-<wasm-hash>/`, so a recorded hash can always be checked
against the bytes it was computed from (see `deployments/README.md`).

## Testnet — 2026-09-03, source commit `ace9dbd` (contract code identical to tag `audit-v1`)

| Field | Value |
|---|---|
| Network | Test SDF Network ; September 2015 |
| Built with | `stellar contract build` (stellar-cli 27.1.0; optimizes by default) |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` (`latch-testnet-deployer`, a throwaway testnet identity) |
| Explorer | `https://stellar.expert/explorer/testnet/contract/<address>` |

**Fee Forwarder configuration:** `admin` = `manager` = deployer (testnet only; mainnet custody is
[#84](https://github.com/3K1-Labs/latch-contracts/issues/84)); `executor` =
`GBLDLFA2Y3RXGL3LZPFTZYDCAE5OZRUVDLBRWAZGA7ZRWT7BGSA7IHMT` (`latch-testnet-executor`, the key latch-relayer#37 will hold).
Fee token enabled: testnet USDC SAC `CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA`
(`USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5`).

**Deployment order used:** wasm uploads (account, templates) → 4 verifiers → 7 policies →
factory (constructed with the account wasm hash + ed25519/webauthn verifiers + threshold policy)
→ fee forwarder → `enable_fee_token`.

Quick reference:

| Contract | Address / hash |
|---|---|
| Latch Smart Account | `85f4e1d77f4f9efab5e8b3effb6d10719951d2355337ba459bc9593fb36fae18` |
| Timelock Vault template | `07e04c1a0fdd0a1a9f2d99742608df835f4bb7813a3879be5f0168f7ae041f34` |
| Vesting Schedule template | `7c6bda5c5bd3ed11296ab5a39477b6f727c317a85966c2de4476d05bfce55b67` |
| Ed25519 Verifier | `CAETENVJ67XXUMQQCKSU774S62TD6C4LDOYGJ3EY6XSTBLXNQZMZI6Q7` |
| P-256 Verifier | `CCOYRAC7JV263TGRCK6TZAGQ3WJU4WGZ7YLUW47I2HHKARX7QMQDU7TG` |
| secp256k1 Verifier | `CDB66JIJH34HBOJ6V2OJ5IT3POXE2PPY6UNHFJNILVUFAMCLOIJZB57O` |
| WebAuthn Verifier | `CANZCUBUIR5U5C5O5H3XAYI73C6HIBISMABUU4UGGHI2YDCCDS7G6KD5` |
| Threshold Policy | `CDQKM6NIZQT5AE5BU4XFEJ3NEHDN2A5PWC5PAXKVVS57GOM2W7Q4HSDK` |
| Weighted Threshold Policy | `CB3BKHAUKTZWLGFEE6YRKPDULFIFGHW4F7A4NQOAOOLSYVEF7KCQEOFJ` |
| Session Policy | `CCDXI5DHJRX5ZS6HFT7ORZAK6FF3X4ADNCMQ363EJQF5UC5ESOS75J2O` |
| Spending Limit Policy | `CDK44EDWIUM6JTSAHPAMBFNMICGQLTBGUD3LAP7NF47Q6VBVUSPSCJIL` |
| Multi-Token Spending Limit Policy | `CABX7KCI2S6QH3SG6TBFG3XUKNBWIBPDVRXSECJSRRZQQUYXNVVKO44L` |
| Parameter-Scoped Policy | `CDFAKXVOEQII5TRPV43UHUKSPPMCWHKBQIQPFBL6IGXD273BN3HIFE6S` |
| Recipient Allowlist Policy | `CDDEJH4YG2ANAGSNXINGW3K3JQM5D7QHA46VSQHTX6PPMHNZGL3HQWO4` |
| Account Factory | `CDMBQXX3F6WHUNFPJN6CWR4U6EAMJTOWVPTJFUQ32J3EHQXVVQLN2H4I` |
| Fee Forwarder | `CB6KFDFN7CXIOSBOEXABOX6KPRK4X6VMPSDWCGYP564JXGN2LIH2QPWI` |

---

## Latch Smart Account (wasm upload only — instances are deployed by the factory)

| Field | Value |
|---|---|
| WASM hash | `85f4e1d77f4f9efab5e8b3effb6d10719951d2355337ba459bc9593fb36fae18` |
| WASM size | 44644 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Upload tx | `0d42ff8badfc4ae1d6d916d4f7b11bc13cf0497bd7b44f15e058787320ab94df` |
| Archived artifact | `deployments/artifacts/smart_account-85f4e1d77f4f9efab5e8b3effb6d10719951d2355337ba459bc9593fb36fae18/` |

### Exported Functions (20)

```
__check_auth
__constructor
add_context_rule
add_policy
add_signer
batch_add_signer
deploy_contract
execute
get_context_rule
get_context_rules_count
get_deployed_contract
get_deployed_contract_count
get_policy_id
get_signer_id
remove_context_rule
remove_policy
remove_signer
update_context_rule_name
update_context_rule_valid_until
upgrade
```

## Timelock Vault template (wasm upload only — users deploy their own instances)

| Field | Value |
|---|---|
| WASM hash | `07e04c1a0fdd0a1a9f2d99742608df835f4bb7813a3879be5f0168f7ae041f34` |
| WASM size | 5309 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Upload tx | `a7fb8e35b15ebabaf410404582bf82d5f1466dcf0cf2ee56865d95329c8c05d9` |
| Archived artifact | `deployments/artifacts/timelock_vault-07e04c1a0fdd0a1a9f2d99742608df835f4bb7813a3879be5f0168f7ae041f34/` |

### Exported Functions (6)

```
__constructor
deposit
get_balance
get_owner
get_unlock_ledger
withdraw
```

## Vesting Schedule template (wasm upload only — users deploy their own instances)

| Field | Value |
|---|---|
| WASM hash | `7c6bda5c5bd3ed11296ab5a39477b6f727c317a85966c2de4476d05bfce55b67` |
| WASM size | 9099 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Upload tx | `f4e2f906a22a90b307be2c3b8008f67b5a1ef7618de893c40432c09a3aa89bd8` |
| Archived artifact | `deployments/artifacts/vesting_schedule-7c6bda5c5bd3ed11296ab5a39477b6f727c317a85966c2de4476d05bfce55b67/` |

### Exported Functions (8)

```
__constructor
claim
claimable_amount
claimed_amount
get_schedule
owner
token
vested_amount
```

## Ed25519 Verifier

| Field | Value |
|---|---|
| WASM hash | `7ec21a870fd7aa6458fff6105fb67db526b54187424eb7687c778be9c5e97ade` |
| WASM size | 1656 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CAETENVJ67XXUMQQCKSU774S62TD6C4LDOYGJ3EY6XSTBLXNQZMZI6Q7` |
| Upload tx | `aedccc56958c8d79dd893b4dec9b1031473816d22c3a60271561c15b6953ae36` |
| Deploy tx | `601dadda861da7424953fdf9b280b3d46c55d6a4cc60c0e6eb4ee22080924773` |
| Archived artifact | `deployments/artifacts/ed25519_verifier-7ec21a870fd7aa6458fff6105fb67db526b54187424eb7687c778be9c5e97ade/` |

### Exported Functions (3)

```
batch_canonicalize_key
canonicalize_key
verify
```

## P-256 Verifier

| Field | Value |
|---|---|
| WASM hash | `9ab31b9466e9c88cfe091e111b7eef17653c884ac6c73afb6744ca525b64abb1` |
| WASM size | 2701 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CCOYRAC7JV263TGRCK6TZAGQ3WJU4WGZ7YLUW47I2HHKARX7QMQDU7TG` |
| Upload tx | `cf1b9e0da3e6988a847d26d1b5c236fe0a60c894e86b9b4180aca075724e19ba` |
| Deploy tx | `050e4ab397f36b5852c5b1e37053003aa5608210c7218f89705c2e477613a258` |
| Archived artifact | `deployments/artifacts/p256_verifier-9ab31b9466e9c88cfe091e111b7eef17653c884ac6c73afb6744ca525b64abb1/` |

### Exported Functions (3)

```
batch_canonicalize_key
canonicalize_key
verify
```

## secp256k1 Verifier

| Field | Value |
|---|---|
| WASM hash | `9b65ede149796ec9bcc4669ed7d8e11e8675c734f0026f73b829e3c7ae1d1478` |
| WASM size | 4658 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CDB66JIJH34HBOJ6V2OJ5IT3POXE2PPY6UNHFJNILVUFAMCLOIJZB57O` |
| Upload tx | `af9adcf8162a58dee0a9a1edf264d42ee69822bdc9e94948307f91314ef55e02` |
| Deploy tx | `cf0a2a9cdaf06dbaa8f204d8224c96253261267addadc543acc14cb2d621b9f4` |
| Archived artifact | `deployments/artifacts/secp256k1_verifier-9b65ede149796ec9bcc4669ed7d8e11e8675c734f0026f73b829e3c7ae1d1478/` |

### Exported Functions (3)

```
batch_canonicalize_key
canonicalize_key
verify
```

## WebAuthn Verifier

| Field | Value |
|---|---|
| WASM hash | `dc4a5393eb111f5d89848fdbc4c79ef914df1a70d819d62d6e1f4168f9f8f6c8` |
| WASM size | 12267 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CANZCUBUIR5U5C5O5H3XAYI73C6HIBISMABUU4UGGHI2YDCCDS7G6KD5` |
| Upload tx | `45e416548d85169b879e7a20caf3de5887f466cd490f89a5bcdd63d3e8f451f9` |
| Deploy tx | `af933b7cd39ae5f72d70249b70297f73fcfe396c7d195278a67c4e744adba0ba` |
| Archived artifact | `deployments/artifacts/webauthn_verifier-dc4a5393eb111f5d89848fdbc4c79ef914df1a70d819d62d6e1f4168f9f8f6c8/` |

### Exported Functions (3)

```
batch_canonicalize_key
canonicalize_key
verify
```

## Threshold Policy

| Field | Value |
|---|---|
| WASM hash | `668a18b9613a01fdb53cf7f56c3613cf9e011eddb43e909a15b21a3155868f24` |
| WASM size | 11203 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CDQKM6NIZQT5AE5BU4XFEJ3NEHDN2A5PWC5PAXKVVS57GOM2W7Q4HSDK` |
| Upload tx | `09f42a44ec3125da05f759134972991a8b5dcd8cd1a604eafc076c1bc7b9cdf1` |
| Deploy tx | `52227e668834e8234283bee2a7347405f20665296ad3e81748bf2d79294c007d` |
| Archived artifact | `deployments/artifacts/threshold_policy-668a18b9613a01fdb53cf7f56c3613cf9e011eddb43e909a15b21a3155868f24/` |

### Exported Functions (5)

```
enforce
get_threshold
install
set_threshold
uninstall
```

## Weighted Threshold Policy

| Field | Value |
|---|---|
| WASM hash | `7b967b02248534442fb9da2ebbb844310429df1449d4463e288a989ac883a676` |
| WASM size | 14422 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CB3BKHAUKTZWLGFEE6YRKPDULFIFGHW4F7A4NQOAOOLSYVEF7KCQEOFJ` |
| Upload tx | `dbaffdb0ed72dc59319477918a1bdb1cfa0782fcd83f587d718e5bb99bef2a93` |
| Deploy tx | `9cacecaf675750196090b8bfd54fe4ff9acf992b15fef2e8cfff17e8283e59e3` |
| Archived artifact | `deployments/artifacts/weighted_threshold_policy-7b967b02248534442fb9da2ebbb844310429df1449d4463e288a989ac883a676/` |

### Exported Functions (7)

```
enforce
get_signer_weights
get_threshold
install
set_signer_weight
set_threshold
uninstall
```

## Session Policy

| Field | Value |
|---|---|
| WASM hash | `a3c840f2e181424abca2153ca61ea48ce35f7ecd162acd88b325e2e5b3dd8132` |
| WASM size | 10165 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CCDXI5DHJRX5ZS6HFT7ORZAK6FF3X4ADNCMQ363EJQF5UC5ESOS75J2O` |
| Upload tx | `5e7c4ed3cb53a1e2c83e1ce27a5b8e5b523d2c7d3c9ed71724295ef04b7ee340` |
| Deploy tx | `6f53d664ec82f31914f156320d105de1ca2061d2cb80402cae91f9a55da3e96e` |
| Archived artifact | `deployments/artifacts/session_policy-a3c840f2e181424abca2153ca61ea48ce35f7ecd162acd88b325e2e5b3dd8132/` |

### Exported Functions (4)

```
enforce
get_allowed_fns
install
uninstall
```

## Spending Limit Policy

| Field | Value |
|---|---|
| WASM hash | `dfd3d2e85678f5263bb79367c66b619891656cac674fdef6209bacd4efe4b66e` |
| WASM size | 14174 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CDK44EDWIUM6JTSAHPAMBFNMICGQLTBGUD3LAP7NF47Q6VBVUSPSCJIL` |
| Upload tx | `3d8efe18b396f7782bcdefefe71a0d8fa1d173d2bf8324feaf62979a69cea48e` |
| Deploy tx | `4fc799908b87465f1f3c5c28683f607b952383e5f16e5178fb2e50512d158582` |
| Archived artifact | `deployments/artifacts/spending_limit_policy-dfd3d2e85678f5263bb79367c66b619891656cac674fdef6209bacd4efe4b66e/` |

### Exported Functions (5)

```
enforce
get_spending_limit_data
install
set_spending_limit
uninstall
```

## Multi-Token Spending Limit Policy

| Field | Value |
|---|---|
| WASM hash | `6c481faba9a62bb12521f3fc1617e1d059538eed55ec5461473e426c407a71f2` |
| WASM size | 17982 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CABX7KCI2S6QH3SG6TBFG3XUKNBWIBPDVRXSECJSRRZQQUYXNVVKO44L` |
| Upload tx | `c810b277f1c7ddc05c65851e4017038bf18160e352f330f2ce5087b2fe56381e` |
| Deploy tx | `c06ed710725147a44c00bbf269ce1308257364db2f85a63fd02572d4628aaf80` |
| Archived artifact | `deployments/artifacts/multi_token_spending_limit_policy-6c481faba9a62bb12521f3fc1617e1d059538eed55ec5461473e426c407a71f2/` |

### Exported Functions (4)

```
enforce
get_policy_data
install
uninstall
```

## Parameter-Scoped Policy

| Field | Value |
|---|---|
| WASM hash | `315f3b856da197163b7ec8e7d6ed5488f4f9576eba4b120b229b9a810443a03e` |
| WASM size | 14125 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CDFAKXVOEQII5TRPV43UHUKSPPMCWHKBQIQPFBL6IGXD273BN3HIFE6S` |
| Upload tx | `129bd3413fd4b8ef1f20f820263b383083ae5760cd4a98c8aab28bc65560aa80` |
| Deploy tx | `4b382cb593f1e8b0f279c3e2e55690db25d97ce725ce748294eb8ba58785f11d` |
| Archived artifact | `deployments/artifacts/parameter_scoped_policy-315f3b856da197163b7ec8e7d6ed5488f4f9576eba4b120b229b9a810443a03e/` |

### Exported Functions (4)

```
enforce
get_conditions
install
uninstall
```

## Recipient Allowlist Policy

| Field | Value |
|---|---|
| WASM hash | `c93acfe6b7a250df24d3de7f84d4a15b3c6cf5aaef99b802649e27565cc158fc` |
| WASM size | 14636 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CDDEJH4YG2ANAGSNXINGW3K3JQM5D7QHA46VSQHTX6PPMHNZGL3HQWO4` |
| Upload tx | `d4a172737be89ed3e71548befa560ae1dce16b0bd5e61a2b11f1540f16280d94` |
| Deploy tx | `00f8e5e57853b105d5c16e5ffaeb61d74ff12970e30bb1951729e10fe872242f` |
| Archived artifact | `deployments/artifacts/recipient_allowlist_policy-c93acfe6b7a250df24d3de7f84d4a15b3c6cf5aaef99b802649e27565cc158fc/` |

### Exported Functions (5)

```
enforce
get_allowed_recipients
install
set_allowed_recipients
uninstall
```

## Account Factory

| Field | Value |
|---|---|
| WASM hash | `26c48c109685eab95eade21f146003f2003632aff16fd93fc2c14b0dc62c716c` |
| WASM size | 8379 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CDMBQXX3F6WHUNFPJN6CWR4U6EAMJTOWVPTJFUQ32J3EHQXVVQLN2H4I` |
| Upload tx | `ff97d62958799bbe3c1cbd2148f74ff37ee50782f773d09f4a14735f8c9be325` |
| Deploy tx | `ba8cafc51c0bc01be0efb851fd2fa5e304ea83e192e19bdc3f5a0192349b0a52` |
| Constructor | `smart_account_wasm_hash=85f4e1d77f4f9efab5e8b3effb6d10719951d2355337ba459bc9593fb36fae18 ed25519_verifier=CAETENVJ67XXUMQQCKSU774S62TD6C4LDOYGJ3EY6XSTBLXNQZMZI6Q7 webauthn_verifier=CANZCUBUIR5U5C5O5H3XAYI73C6HIBISMABUU4UGGHI2YDCCDS7G6KD5 threshold_policy=CDQKM6NIZQT5AE5BU4XFEJ3NEHDN2A5PWC5PAXKVVS57GOM2W7Q4HSDK` |
| Archived artifact | `deployments/artifacts/factory_contract-26c48c109685eab95eade21f146003f2003632aff16fd93fc2c14b0dc62c716c/` |

### Exported Functions (5)

```
__constructor
create_account
get_account_address
get_threshold_policy
get_verifier
```

## Fee Forwarder

| Field | Value |
|---|---|
| WASM hash | `d6fd2e3d7d0dab4dfc35275e5d0b07e01b756966b6650f94a876bb557793f776` |
| WASM size | 21761 bytes |
| Built with | `stellar contract build` |
| Network | testnet |
| Deployed by | `GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K` |
| Contract address | `CB6KFDFN7CXIOSBOEXABOX6KPRK4X6VMPSDWCGYP564JXGN2LIH2QPWI` |
| Upload tx | `82e259384f9a44b9dac2a3b05f54ea7957ea87d49ea04c74551541dc89295035` |
| Deploy tx | `918d24927e8105dd9b97a28405decea265b66e826047d0f31bbafc3c3a899717` |
| Constructor | `admin=GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K manager=GCOWPY2RHD6Y764NAEIB2SMJITDJLAJ7X4WVAQWQQKQTPRNRQKMT2W5K executors=[GBLDLFA2Y3RXGL3LZPFTZYDCAE5OZRUVDLBRWAZGA7ZRWT7BGSA7IHMT]` |
| Archived artifact | `deployments/artifacts/fee_forwarder-d6fd2e3d7d0dab4dfc35275e5d0b07e01b756966b6650f94a876bb557793f776/` |

### Exported Functions (18)

```
__constructor
accept_admin_transfer
disable_fee_token
enable_fee_token
forward
get_admin
get_existing_roles
get_role_admin
get_role_member
get_role_member_count
grant_role
has_role
renounce_admin
renounce_role
revoke_role
set_role_admin
sweep_tokens
transfer_admin_role
```

