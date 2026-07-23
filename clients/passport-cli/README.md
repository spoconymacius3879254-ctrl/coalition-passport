# Passport CLI

This is the testable command-line client for the versioned generated IDL at
`idl/coalition_passport.json`. It supports offline instruction building,
read-only account inspection, and explicitly selected Devnet submission. The
checked-in IDL is the public integration interface and must be refreshed from
`target/idl/coalition_passport.json` whenever the program interface changes.

Every `build` command constructs and prints one unsigned instruction without
contacting RPC or reading a key file. A `send` command instead requires an
explicit `--signer-keypair` path and confirms the RPC genesis hash is Solana
Devnet before signing. There is no default wallet. Passport creation generates
its fresh mint signer only in memory and discards it after the one-of-one mint;
the on-chain program permanently revokes that mint authority.

`record-receipt` accepts either a 64-character nonzero opaque commitment, or a
receipt reference paired with a nonempty salt. The latter is SHA-256 domain
separated by this program ID and merchant PDA. Raw references and salts are
never printed; avoid predictable references and reuse of salts.

```sh
npm ci
npm run build
npm test
npm run audit

# Read-only derivation
npm run cli -- derive coalition --authority <PUBKEY>

# Offline review: no key read and no RPC call
npm run cli -- build initialize-coalition --signer <PUBKEY> \
  --max-receipt-units 500 --tiers 100,300,700

# Explicit Devnet submission
npm run cli -- send initialize-coalition --signer-keypair ./devnet.json \
  --max-receipt-units 500 --tiers 100,300,700
```

The dependency tree pins Anchor 1.0.2 and legacy Web3 v1 as required by the
official Anchor client. `uuid` is overridden to 11.1.1 because the legacy
Web3/Jayson range otherwise resolves to an audited vulnerable release; build,
tests, and an actual Devnet RPC version query verify compatibility.
