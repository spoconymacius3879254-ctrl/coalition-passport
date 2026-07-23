# Superteam submission draft

## Project

**Coalition Passport — portable neighbourhood reputation without pooled merchant
liability**

Repository: <https://github.com/spoconymacius3879254-ctrl/coalition-passport>

Program ID: `2A2227YnW1PEr6FrMLxZrjm8B3P3fHWQjjqM8tDNhxg6`

## Pitch

Traditional coalition loyalty either traps points in separate apps or makes
every merchant accept a shared liability. Coalition Passport separates the two
things customers and merchants actually need: portable reputation and local
credit.

Each customer owns a one-of-one Token-2022 `NonTransferable` Passport. A
registered merchant signs an opaque receipt commitment into its own isolated
balance PDA. That receipt earns issuer-local credit plus a bounded coalition
streak. Any partner dApp can read the Passport tier to gate a perk, but no
partner receives authority to spend or inflate another merchant's balance.

## Solana-specific innovation

- Token-2022 enforces soulbound transfer behavior at the token-program layer.
- Atomic CPIs create the mint and canonical ATA, mint exactly one credential,
  and permanently revoke mint authority.
- Deterministic PDAs expose a stable public composability surface while binding
  every mutation to coalition, customer, and merchant identities. Consumers
  combine the Coalition threshold schedule with Passport streak state to derive
  the current tier; the CLI `status` command is the tested reference consumer.
- Merchant-signed accrual removes a customer signature from each checkout;
  customers sign only Passport creation and redemption.

## Security and correctness evidence

- 20 Rust tests, including a real-SBF LiteSVM end-to-end flow.
- 7 CLI tests, including offline encoding of all seven public instructions.
- Strict Clippy, Rust formatting, TypeScript build, and zero-vulnerability
  production npm audit.
- Clean-room public CI:
  <https://github.com/spoconymacius3879254-ctrl/coalition-passport/actions/runs/29971763713>
- Trusted Clock-derived cap epochs, monotonic receipt nonces, checked pre-write
  arithmetic, typed errors, canonical Token-2022 ATA constraints, and
  redemption that remains available during pause.

## Public demo

The versioned Anchor IDL and compiled CLI are in `clients/passport-cli`. The CLI
supports wallet-free PDA derivation, combined tier/status inspection, offline
instruction review, and explicitly signed Devnet execution. It has no default
wallet and refuses to send if the RPC genesis hash is not Solana Devnet.

## Devnet links

Pending faucet funding. Before submission, replace this section with Explorer
links for deployment, coalition initialization, merchant registration, Passport
creation, receipt accrual, redemption, and final decoded state.

## Honest tradeoffs

The chain cannot prove a physical sale; registered merchants remain trusted
issuers. Caps, nonces, status, and pause bound that trust. Receipt commitments
hide raw data but are not zero-knowledge proofs, so low-entropy references need
unique private salts. Credits deliberately cannot move between merchants: the
design sacrifices speculation and pooled liquidity to preserve consumer clarity
and merchant solvency. Production admin authority should use a multisig and
timelock.
