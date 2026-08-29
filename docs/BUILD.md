# Build Artifacts

Deployment records for contracts actually live on a network right now. Testnet churns fast during
active development — an entry here is deleted and replaced the moment its contract is redeployed,
not preserved as history. Permanent, verifiable deployment records (matching the
`deployments/artifacts/` discipline) start at the next deployment meant to actually persist, and
definitely by mainnet — not retroactively applied to iteration that's already obsolete.

No contract has a current, verified deployment recorded here as of this reset. Add an entry per
contract, in this shape, once (re)deployed:

```
## <Contract Name>

| Field | Value |
|---|---|
| WASM hash | |
| WASM size | |
| Built with | `stellar contract build --optimize` |
| Network | |
| Deployed by | |
| Contract address | |
| Upload tx | |
| Deploy tx | |
| Archived artifact | `deployments/artifacts/<contract>-<hash>/` |

### Exported Functions (N)

\`\`\`
...
\`\`\`
```
