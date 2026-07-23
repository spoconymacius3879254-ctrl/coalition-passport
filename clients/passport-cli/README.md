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
npm run check-devnet

# Read-only derivation
npm run cli -- derive coalition --authority <PUBKEY>

# Offline review: no key read and no RPC call
npm run cli -- build initialize-coalition --signer <PUBKEY> \
  --max-receipt-units 500 --tiers 100,300,700

# Explicit Devnet submission
npm run cli -- send initialize-coalition --signer-keypair ./devnet.json \
  --max-receipt-units 500 --tiers 100,300,700
```

## Complete Devnet flow

The program must already be deployed. Replace public-key placeholders with the
addresses printed by `solana address --keypair <file>`; never paste private key
bytes into a command or chat.

```sh
# 1. Coalition admin initializes shared rules.
npm run cli -- send initialize-coalition \
  --signer-keypair <ADMIN_KEYPAIR_FILE> \
  --max-receipt-units 500 --tiers 100,300,700

# 2. Admin and merchant both consent to registration.
npm run cli -- send register-merchant \
  --signer-keypair <ADMIN_KEYPAIR_FILE> \
  --merchant-keypair <MERCHANT_KEYPAIR_FILE> \
  --earn-bps 1000 --daily-cap 500

# 3. Customer creates a unique soulbound Passport. The mint signer is generated
# in memory for this one transaction and is not retained.
npm run cli -- send create-passport \
  --signer-keypair <CUSTOMER_KEYPAIR_FILE> \
  --authority <ADMIN_PUBKEY>

# 4. Merchant records a private, salted receipt commitment.
npm run cli -- send record-receipt \
  --signer-keypair <MERCHANT_KEYPAIR_FILE> \
  --authority <ADMIN_PUBKEY> --customer <CUSTOMER_PUBKEY> \
  --nonce 1 --amount 120 --receipt-commitment <64_HEX_CHARACTERS>

# 5. Anyone can derive and inspect public state without a wallet.
npm run cli -- derive passport \
  --authority <ADMIN_PUBKEY> --customer <CUSTOMER_PUBKEY>
npm run cli -- derive balance \
  --authority <ADMIN_PUBKEY> --merchant-authority <MERCHANT_PUBKEY> \
  --customer <CUSTOMER_PUBKEY>
npm run cli -- show passport --address <PASSPORT_PDA>
npm run cli -- show balance --address <BALANCE_PDA>
npm run cli -- status --authority <ADMIN_PUBKEY> \
  --customer <CUSTOMER_PUBKEY> --merchant-authority <MERCHANT_PUBKEY>

# 6. Only the customer can redeem issuer-local credit.
npm run cli -- send redeem \
  --signer-keypair <CUSTOMER_KEYPAIR_FILE> \
  --authority <ADMIN_PUBKEY> --merchant-authority <MERCHANT_PUBKEY> \
  --units 10

# 7. Admin pause blocks new accrual; unpause resumes it. Redemption remains
# available while paused by on-chain design.
npm run cli -- send pause-coalition --signer-keypair <ADMIN_KEYPAIR_FILE>
npm run cli -- send unpause-coalition --signer-keypair <ADMIN_KEYPAIR_FILE>
```

Always prefer `--receipt-commitment <64_HEX_CHARS>` computed by the merchant
backend. The `--receipt-reference` plus `--receipt-salt` convenience exists for
synthetic demos only: command-line arguments can remain in shell history and
process listings and must never contain real customer or basket data.

The dependency tree pins Anchor 1.0.2 and legacy Web3 v1 as required by the
official Anchor client. `uuid` is overridden to 11.1.1 because the legacy
Web3/Jayson range otherwise resolves to an audited vulnerable release; build,
tests, and an actual Devnet RPC version query verify compatibility.
