#!/usr/bin/env bash
set -euo pipefail

MODE="${1:---check}"
if [[ "$MODE" != "--check" && "$MODE" != "--execute" ]]; then
  echo "usage: scripts/devnet-demo.sh [--check|--execute]" >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLI_DIR="$REPO_ROOT/clients/passport-cli"
RPC_URL="https://api.devnet.solana.com"
EXPECTED_GENESIS="EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"
EXPECTED_PROGRAM="2A2227YnW1PEr6FrMLxZrjm8B3P3fHWQjjqM8tDNhxg6"
EXPECTED_DEPLOYER="Edk1vBTRxCeqxYmytWJcBYCxBxHsh4jXFNrVChdyLxc"
EXPECTED_SBF_SHA="59988c56c38586425af52d4b78c0914b7d3f2e0f0a48d3fdb4b6524554d4fb6f"
MINIMUM_LAMPORTS=2300000000

SOLANA_BIN_DIR="${SOLANA_BIN_DIR:-$HOME/.local/share/solana/install/active_release/bin}"
SOLANA="$SOLANA_BIN_DIR/solana"
SOLANA_KEYGEN="$SOLANA_BIN_DIR/solana-keygen"
DEPLOYER="$REPO_ROOT/.anchor/devnet-deployer.json"
MERCHANT="$REPO_ROOT/.anchor/devnet-merchant.json"
CUSTOMER="$REPO_ROOT/.anchor/devnet-customer.json"
PROGRAM_KEYPAIR="$REPO_ROOT/target/deploy/coalition_passport-keypair.json"
PROGRAM_SO="$REPO_ROOT/target/deploy/coalition_passport.so"
EVIDENCE_DIR="$REPO_ROOT/evidence/runtime"

for executable in "$SOLANA" "$SOLANA_KEYGEN"; do
  [[ -x "$executable" ]] || { echo "missing executable: $executable" >&2; exit 1; }
done
for file in "$DEPLOYER" "$MERCHANT" "$CUSTOMER" "$PROGRAM_KEYPAIR" "$PROGRAM_SO"; do
  [[ -f "$file" ]] || { echo "missing required file: $file" >&2; exit 1; }
done
for keypair in "$DEPLOYER" "$MERCHANT" "$CUSTOMER" "$PROGRAM_KEYPAIR"; do
  permissions="$(stat -c '%a' "$keypair")"
  [[ "$permissions" == "600" ]] || {
    echo "refusing keypair with permissions $permissions: $keypair" >&2
    exit 1
  }
done

DEPLOYER_PUBKEY="$("$SOLANA_KEYGEN" pubkey "$DEPLOYER")"
MERCHANT_PUBKEY="$("$SOLANA_KEYGEN" pubkey "$MERCHANT")"
CUSTOMER_PUBKEY="$("$SOLANA_KEYGEN" pubkey "$CUSTOMER")"
PROGRAM_PUBKEY="$("$SOLANA_KEYGEN" pubkey "$PROGRAM_KEYPAIR")"
[[ "$DEPLOYER_PUBKEY" == "$EXPECTED_DEPLOYER" ]] || {
  echo "unexpected deployer public key: $DEPLOYER_PUBKEY" >&2
  exit 1
}
[[ "$PROGRAM_PUBKEY" == "$EXPECTED_PROGRAM" ]] || {
  echo "unexpected program public key: $PROGRAM_PUBKEY" >&2
  exit 1
}

ACTUAL_SBF_SHA="$(sha256sum "$PROGRAM_SO" | cut -d' ' -f1)"
[[ "$ACTUAL_SBF_SHA" == "$EXPECTED_SBF_SHA" ]] || {
  echo "unexpected SBF digest: $ACTUAL_SBF_SHA" >&2
  exit 1
}
GENESIS_HASH="$("$SOLANA" genesis-hash --url "$RPC_URL")"
[[ "$GENESIS_HASH" == "$EXPECTED_GENESIS" ]] || {
  echo "refusing non-Devnet RPC genesis hash: $GENESIS_HASH" >&2
  exit 1
}

(
  cd "$CLI_DIR"
  npm run build
  npm test
  npm run audit
  npm run check-devnet
)
echo "preflight passed"
echo "program:  $PROGRAM_PUBKEY"
echo "deployer: $DEPLOYER_PUBKEY"
echo "merchant: $MERCHANT_PUBKEY"
echo "customer: $CUSTOMER_PUBKEY"

if [[ "$MODE" == "--check" ]]; then
  echo "check-only mode: no transaction was signed or submitted"
  exit 0
fi

BALANCE_TEXT="$("$SOLANA" balance "$DEPLOYER_PUBKEY" --lamports --url "$RPC_URL")"
if [[ ! "$BALANCE_TEXT" =~ ^[0-9]+([[:space:]]+lamports)?$ ]]; then
  echo "unexpected Devnet balance response" >&2
  exit 1
fi
BALANCE_LAMPORTS="${BALANCE_TEXT%% *}"
if (( BALANCE_LAMPORTS < MINIMUM_LAMPORTS )); then
  echo "insufficient Devnet balance: $BALANCE_LAMPORTS lamports; require $MINIMUM_LAMPORTS" >&2
  exit 1
fi
mkdir -p "$EVIDENCE_DIR"
[[ ! -e "$EVIDENCE_DIR/complete.json" ]] || {
  echo "refusing to rerun a completed demo; archive evidence/runtime first" >&2
  exit 1
}

"$SOLANA" program deploy "$PROGRAM_SO" \
  --program-id "$PROGRAM_KEYPAIR" --keypair "$DEPLOYER" \
  --upgrade-authority "$DEPLOYER" --url "$RPC_URL" \
  --commitment confirmed --use-rpc --output json \
  | tee "$EVIDENCE_DIR/deploy.json"
"$SOLANA" transfer "$MERCHANT_PUBKEY" 0.02 \
  --from "$DEPLOYER" --allow-unfunded-recipient \
  --url "$RPC_URL" --commitment confirmed --output json \
  | tee "$EVIDENCE_DIR/fund-merchant.json"
"$SOLANA" transfer "$CUSTOMER_PUBKEY" 0.02 \
  --from "$DEPLOYER" --allow-unfunded-recipient \
  --url "$RPC_URL" --commitment confirmed --output json \
  | tee "$EVIDENCE_DIR/fund-customer.json"

run_cli() {
  local evidence_name="$1"
  shift
  (cd "$CLI_DIR" && npm run --silent cli -- "$@") \
    | tee "$EVIDENCE_DIR/$evidence_name.json"
}
run_cli initialize-coalition send initialize-coalition \
  --signer-keypair "$DEPLOYER" --max-receipt-units 500 --tiers 10,30,70 \
  --rpc "$RPC_URL"
run_cli register-merchant send register-merchant \
  --signer-keypair "$DEPLOYER" --merchant-keypair "$MERCHANT" \
  --earn-bps 1000 --daily-cap 500 --rpc "$RPC_URL"
run_cli create-passport send create-passport \
  --signer-keypair "$CUSTOMER" --authority "$DEPLOYER_PUBKEY" --rpc "$RPC_URL"
run_cli record-receipt send record-receipt \
  --signer-keypair "$MERCHANT" --authority "$DEPLOYER_PUBKEY" \
  --customer "$CUSTOMER_PUBKEY" --nonce 1 --amount 120 \
  --receipt-commitment 9f9e9d9c9b9a999897969594939291908f8e8d8c8b8a89888786858483828180 \
  --rpc "$RPC_URL"
run_cli before-redeem status --authority "$DEPLOYER_PUBKEY" \
  --customer "$CUSTOMER_PUBKEY" --merchant-authority "$MERCHANT_PUBKEY" \
  --rpc "$RPC_URL"
run_cli redeem send redeem --signer-keypair "$CUSTOMER" \
  --authority "$DEPLOYER_PUBKEY" --merchant-authority "$MERCHANT_PUBKEY" \
  --units 10 --rpc "$RPC_URL"
run_cli final-status status --authority "$DEPLOYER_PUBKEY" \
  --customer "$CUSTOMER_PUBKEY" --merchant-authority "$MERCHANT_PUBKEY" \
  --rpc "$RPC_URL"

node - "$EVIDENCE_DIR" "$PROGRAM_PUBKEY" <<'NODE'
const fs = require("fs");
const path = require("path");
const [directory, programId] = process.argv.slice(2);
const names = [
  "deploy", "fund-merchant", "fund-customer", "initialize-coalition",
  "register-merchant", "create-passport", "record-receipt", "redeem",
];
const evidence = {
  cluster: "devnet",
  programId,
  programUrl: `https://explorer.solana.com/address/${programId}?cluster=devnet`,
  transactions: {},
  finalStatus: JSON.parse(fs.readFileSync(path.join(directory, "final-status.json"), "utf8")),
};
for (const name of names) {
  const item = JSON.parse(fs.readFileSync(path.join(directory, `${name}.json`), "utf8"));
  const signature = item.signature;
  evidence.transactions[name] = {
    signature,
    explorerUrl: signature
      ? `https://explorer.solana.com/tx/${signature}?cluster=devnet`
      : item.explorerUrl || null,
  };
}
fs.writeFileSync(
  path.join(directory, "complete.json"),
  `${JSON.stringify(evidence, null, 2)}\n`,
  { mode: 0o600 },
);
process.stdout.write(`${JSON.stringify(evidence, null, 2)}\n`);
NODE
