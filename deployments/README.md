# Deployments

Empty until the next deployment meant to actually persist — not retroactively populated for
testnet churn that's already obsolete. See `../docs/BUILD.md` for the current (currently empty) index.

When a real deployment happens:

1. Build with `stellar contract build --optimize`.
2. Copy the resulting `.wasm` into `deployments/artifacts/<contract>-<wasm-hash>/` before it can
   get swept away by a clean build — `target/` is gitignored and ephemeral, this folder isn't.
3. Add the corresponding entry to `../docs/BUILD.md`, pointing at the archived copy.

## Layout

- `artifacts/<contract>-<wasm-hash>/` — testnet binaries indexed in `../docs/BUILD.md` (stellar-cli
  27.1.0 builds).
- `artifacts/mainnet/<contract>-<wasm-hash>/` — mainnet test deployment binaries (stellar-cli 28.0.0
  builds of `main` at `a366bd0`; contract code identical to `audit-v1`). Their addresses and
  transactions are tracked in the team's deployment guide, not in this public repo. CLI 28 builds
  hash differently from the 27.1.0 builds of the same source, so the two sets don't match by hash.

This mirrors `references/argent-contracts-starknet/deployments/`'s split: a hand-maintained index
(`../docs/BUILD.md`) plus physically preserved build artifacts (`artifacts/`), so a recorded hash can
always be checked against the actual binary it was computed from — not just trusted.
