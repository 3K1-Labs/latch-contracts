# Vesting Schedule Contract Template

A "personal contract" template that implements linear and cliff-based token vesting for Latch smart accounts.

## Features

- **Personal Contract Model**: Deployed per schedule, owned by the beneficiary's smart account.
- **Single-Token**: Fixed SEP-41 token at construction.
- **Flexible Timing**: Start ledger, optional cliff ledger, and completion ledger.
- **Exact Math**: Wide integer arithmetic prevents overflow and eliminates residual dust.
- **Owner Auth**: Only the configured owner smart account can claim vested tokens.

## Specification

See [`docs/vesting-schedule-spec.md`](../../docs/vesting-schedule-spec.md) for full architectural specification, mathematical formulas, error codes, and events.
