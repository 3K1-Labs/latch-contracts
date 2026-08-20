# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in any contract in this repository,
please report it privately by email to **superfranky@3k1labs.com** rather than
opening a public issue. Include:

- A description of the vulnerability and its potential impact.
- Steps to reproduce, or a proof of concept if you have one.
- The affected contract(s) and, if known, the affected function(s).

We'll acknowledge your report as soon as we can and keep you updated as we
investigate and fix the issue. Please give us a reasonable amount of time to
address the report before any public disclosure.

## Scope

This policy covers the smart contracts in this repository:
`latch-smart-account`, `account-factory`, `latch-verifiers/*`, and the
`policies/*` crates (`threshold-policy`, `weighted-threshold-policy`,
`session-policy`, `spending-limit-policy`).

Vulnerabilities in upstream dependencies (e.g. OpenZeppelin's
[stellar-contracts](https://github.com/OpenZeppelin/stellar-contracts), or the
Soroban SDK / host itself) should be reported directly to their respective
maintainers, not to us — though we'd still appreciate a heads-up if it affects
how we use them.

## Status

This project has not yet undergone an external security audit. Treat it as
early-stage software: do not deploy it to hold real value without your own
independent review. This will be updated once an audit has taken place.
