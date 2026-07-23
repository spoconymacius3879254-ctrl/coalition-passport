# Coalition Passport

[![CI](https://github.com/spoconymacius3879254-ctrl/coalition-passport/actions/workflows/ci.yml/badge.svg)](https://github.com/spoconymacius3879254-ctrl/coalition-passport/actions/workflows/ci.yml)

Coalition Passport is a Solana loyalty primitive that makes customer reputation
portable across local businesses without turning merchant points into a
speculative token or forcing one merchant to assume another merchant's
liability.

It is being built for Superteam Poland's On-Chain Loyalty Rewards System
Challenge. The Rust/Anchor program, Token-2022 integration, real-SBF tests, and
TypeScript CLI are implemented locally. Devnet deployment is pending test-SOL
faucet funding; no mainnet deployment or real funds are involved.

## What is new

A customer holds a one-of-one, non-transferable Token-2022 Passport. Each
merchant records authenticated receipt commitments into a separate balance PDA:

- merchant-local credits can be redeemed only with their issuer;
- bounded streak points aggregate into a coalition-wide reputation tier; and
- another dApp can combine the Passport streak with Coalition thresholds to
  derive a tier and gate a perk without receiving authority over balances.

This separates **portable reputation** from **local financial liability**. A
coffee shop can recognize a trusted neighbourhood regular while never accepting
or subsidizing a bookstore's credits. There is no points swap, price oracle,
transfer market, or DeFi dependency.

## Why Solana

- **Token-2022 NonTransferable:** soulbound behavior is enforced by the token
  program, not a UI convention. Supply is exactly one, decimals are zero, and
  mint authority is permanently revoked in the creation transaction.
- **PDAs:** deterministic coalition, merchant, Passport, and balance addresses
  make the state independently discoverable and prevent account substitution.
- **Atomic CPIs:** Passport creation allocates and initializes a Token-2022
  mint, creates its canonical ATA, mints once, and revokes authority atomically.
- **Low fees and throughput:** a merchant signs receipt accrual without making
  the customer approve every visit; the customer signs only creation and
  redemption.

## Architecture

```text
customer signer ── creates ──> Passport PDA ── controls ──> 1-of-1 Token-2022 NFT
                                    │
merchant signer ── receipt ─────────┼──> MerchantBalance PDA (issuer-local credit)
                                    └──> streak + tier (portable reputation)

customer signer ── redeem ─────────────> decrements one MerchantBalance
partner dApp ── reads Passport + Coalition ──> derives tier and gates perk
```

| Account | PDA seeds | Role |
| --- | --- | --- |
| `Coalition` | `coalition`, authority | Rules, tier thresholds, receipt cap, pause state |
| `Merchant` | `merchant`, coalition, merchant authority | Authenticated issuer and earn policy |
| `Passport` | `passport`, coalition, customer | Customer identity keys, visits, streak, tier inputs |
| `MerchantBalance` | `balance`, Passport, Merchant | Isolated earned/redeemed units, daily cap state, nonce |

The public `Coalition`, `Passport`, and `MerchantBalance` layouts are the
composability API. A partner program can constrain the Coalition and Passport
PDAs to this program, derive the tier from `streak_points` and the active
threshold schedule, and gate its own instruction without gaining write
authority. The CLI `status` command demonstrates that exact consumer logic.

## On-chain flow and guarantees

1. `initialize_coalition` validates bounded, strictly increasing tier rules.
2. `register_merchant` requires both coalition admin and merchant consent.
3. `create_passport` creates the unique PDA and soulbound Token-2022 credential.
4. `record_receipt` requires the registered merchant signer, a strictly
   increasing nonce, nonzero opaque commitment, active merchant, and unpaused
   coalition. The trusted Clock sysvar—not caller input—derives the daily epoch.
5. `redeem` requires the Passport customer and checks issuer-local available
   credit. Redemption remains available while paused or inactive so an admin
   cannot trap already-earned customer value.

All reward/balance arithmetic is checked before writes. Failed transactions are
atomic. Events contain public keys, bounded counters, and an opaque commitment,
never a raw receipt, customer name, email, phone number, or basket contents.

## Testable CLI

The CLI uses the generated Anchor IDL and the official Anchor TypeScript client.
It has three explicit modes:

- `derive`: calculate public PDAs locally;
- `show`: fetch and decode one public account; and
- `status`: combine Coalition and Passport state into a readable tier/credit
  view; and
- `build` / `send`: inspect an unsigned instruction or explicitly sign it.

`build` reads no key file and contacts no RPC. `send` has no default wallet,
requires a caller-named keypair, and refuses to submit unless the RPC reports the
exact Solana Devnet genesis hash. Receipt references require a salt and are
domain-separated by program and merchant, or callers can provide a precomputed
32-byte commitment.

```sh
cd clients/passport-cli
npm ci
npm run build
npm test
npm run audit
npm run check-devnet

npm run cli -- derive coalition --authority <AUTHORITY_PUBKEY>
npm run cli -- show passport --address <PASSPORT_PDA>
npm run cli -- status --authority <AUTHORITY_PUBKEY> --customer <CUSTOMER_PUBKEY>
npm run cli -- build record-receipt \
  --signer <MERCHANT_PUBKEY> \
  --authority <COALITION_AUTHORITY_PUBKEY> \
  --customer <CUSTOMER_PUBKEY> \
  --nonce 1 --amount 120 --receipt-commitment <64_HEX_CHARACTERS>
```

See [`clients/passport-cli/README.md`](clients/passport-cli/README.md) for send
mode and the complete command surface.

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
anchor build

cd clients/passport-cli
npm ci
npm run build
npm test
npm run audit
```

Current evidence:

- 20 Rust tests pass: 8 Anchor unit, 1 real-SBF LiteSVM acceptance flow, 10
  deterministic core, and 1 JSON fixture integration test;
- 7 CLI tests cover validation, readable account/tier decoding, privacy
  commitments, PDA isolation, and offline encoding of all seven on-chain
  instruction builders;
- strict Clippy, formatting, Anchor SBF build, TypeScript build, and production
  dependency audit pass; and
- `target/deploy/coalition_passport.so` is 313,064 bytes, SHA-256
  `59988c56c38586425af52d4b78c0914b7d3f2e0f0a48d3fdb4b6524554d4fb6f`.

The [public clean-room CI run](https://github.com/spoconymacius3879254-ctrl/coalition-passport/actions/runs/29972255775)
rebuilds the SBF program, executes the real bytecode through LiteSVM, verifies
the versioned IDL and exact program digest, and publishes the SBF/IDL bundle as
a downloadable workflow artifact.

The host CPU lacks AVX, so the legacy validator binary cannot start. The
transaction suite instead uses Anchor's LiteSVM path and executes the real SBF
artifact—not a mocked instruction implementation.

## Devnet evidence

Program ID: `2A2227YnW1PEr6FrMLxZrjm8B3P3fHWQjjqM8tDNhxg6`

Deployment and transaction links will be added after the gitignored Devnet-only
deployer receives faucet SOL. This section intentionally does not present local
simulation as public deployment evidence.

The deployment runner defaults to a non-signing preflight. It verifies the
Devnet genesis hash, exact program/deployer identities, signer permissions,
SBF digest, CLI build, tests, audit, and RPC compatibility:

```sh
scripts/devnet-demo.sh --check
```

Only `scripts/devnet-demo.sh --execute` signs or submits. It first requires at
least 2.3 Devnet SOL, deploys with the explicit gitignored signer, funds the two
throwaway demo identities, runs the complete loyalty flow, and writes public
signatures plus final decoded state to gitignored
`evidence/runtime/complete.json`.

## Tradeoffs and trust boundaries

- The chain cannot prove a physical sale. Registered merchants are trusted
  receipt issuers; nonces, caps, status, and coalition pause bound but do not
  eliminate merchant fraud.
- The commitment hides raw receipt content but is not a zero-knowledge proof.
  Low-entropy references need a unique private salt to resist guessing.
- Reputation is intentionally non-transferable and credits are intentionally
  non-composable across issuers. This sacrifices a secondary market to preserve
  consumer clarity and merchant solvency.
- Admin pause blocks new accrual and registration, not customer redemption.
  Production governance should put admin authority behind a multisig/timelock.
- Card-payment verification, geofencing, fiat settlement, and off-chain merchant
  fulfillment are outside this submission's enforcement boundary.

## Safety

This project is Devnet-only demonstration software. It is not audited, does not
hold real funds, and is not a payment rail, investment product, or mainnet-ready
loyalty system.
