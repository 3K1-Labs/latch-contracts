# Deployments

Empty until the next deployment meant to actually persist — not retroactively populated for
testnet churn that's already obsolete. See `../docs/BUILD.md` for the current (currently empty) index.

When a real deployment happens:

1. Build with `stellar contract build --optimize`.
2. Copy the resulting `.wasm` into `deployments/artifacts/<contract>-<wasm-hash>/` before it can
   get swept away by a clean build — `target/` is gitignored and ephemeral, this folder isn't.
3. Add the corresponding entry to `../docs/BUILD.md`, pointing at the archived copy.

This mirrors `references/argent-contracts-starknet/deployments/`'s split: a hand-maintained index
(`../docs/BUILD.md`) plus physically preserved build artifacts (`artifacts/`), so a recorded hash can
always be checked against the actual binary it was computed from — not just trusted.
