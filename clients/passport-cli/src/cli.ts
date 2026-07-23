#!/usr/bin/env node
import { Keypair, PublicKey, TransactionInstruction } from "@solana/web3.js";
import { BN } from "@anchor-lang/core";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  DEFAULT_IDL_PATH,
  PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  accountClientName,
  deriveBalance,
  deriveAddresses,
  deriveCoalition,
  deriveMerchant,
  derivePassport,
  deriveToken2022Ata,
  integerValue,
  jsonSafe,
  loadExplicitSigner,
  parsePublicKey,
  parseReceiptCommitment,
  parseTierThresholds,
  parseU16,
  parseUnsigned,
  printableInstruction,
  programForReadOnlyUse,
  receiptCommitment,
  rpcUrl,
  sendDevnetInstruction,
  tierLevelFor,
} from "./core.js";

type Flags = Record<string, string>;

function usage(): never {
  throw new Error(`Usage:
  passport-cli derive coalition --authority PUBKEY
  passport-cli derive merchant --authority PUBKEY --merchant-authority PUBKEY
  passport-cli derive passport --authority PUBKEY --customer PUBKEY
  passport-cli derive balance --authority PUBKEY --merchant-authority PUBKEY --customer PUBKEY
  passport-cli show <coalition|merchant|passport|balance> --address PUBKEY [--rpc URL] [--idl PATH]
  passport-cli status --authority PUBKEY --customer PUBKEY [--merchant-authority PUBKEY] [--rpc URL]
  passport-cli build initialize-coalition --signer PUBKEY --max-receipt-units U64 --tiers N,N,... [--idl PATH]
  passport-cli build <pause-coalition|unpause-coalition> --signer PUBKEY [--idl PATH]
  passport-cli build register-merchant --signer PUBKEY --merchant-authority PUBKEY --earn-bps U16 --daily-cap U64 [--idl PATH]
  passport-cli build create-passport --signer PUBKEY --authority PUBKEY --passport-mint PUBKEY [--idl PATH]
  passport-cli build record-receipt --signer PUBKEY --authority PUBKEY --customer PUBKEY --nonce U64 --amount U64 (--receipt-commitment HEX | --receipt-reference TEXT --receipt-salt TEXT) [--idl PATH]
  passport-cli build redeem --signer PUBKEY --authority PUBKEY --merchant-authority PUBKEY --units U64 [--idl PATH]

Replace build with send and --signer PUBKEY with --signer-keypair FILE to sign
and submit to verified Solana Devnet. register-merchant also takes
--merchant-keypair FILE; create-passport generates an ephemeral mint signer.

Build commands never read key files or contact RPC. Send commands refuse any
RPC whose genesis hash is not Solana Devnet.`);
}

function parse(argv: string[]): { positionals: string[]; flags: Flags } {
  const positionals: string[] = [];
  const flags: Flags = {};
  for (let index = 0; index < argv.length; index += 1) {
    const item = argv[index];
    if (item?.startsWith("--")) {
      const key = item.slice(2);
      const value = argv[index + 1];
      if (value === undefined || value.startsWith("--")) throw new Error(`missing value for --${key}`);
      if (flags[key] !== undefined) throw new Error(`--${key} was supplied more than once`);
      flags[key] = value;
      index += 1;
    } else if (item !== undefined) {
      positionals.push(item);
    }
  }
  return { positionals, flags };
}

function required(flags: Flags, name: string): string {
  const value = flags[name];
  if (value === undefined) throw new Error(`--${name} is required`);
  return value;
}

function output(value: unknown): void {
  process.stdout.write(`${JSON.stringify(jsonSafe(value), null, 2)}\n`);
}

function receiptHash(flags: Flags, merchant: import("@solana/web3.js").PublicKey): number[] {
  const commitment = flags["receipt-commitment"];
  const reference = flags["receipt-reference"];
  const salt = flags["receipt-salt"];
  if (commitment !== undefined) {
    if (reference !== undefined || salt !== undefined) throw new Error("use either --receipt-commitment or --receipt-reference with --receipt-salt");
    return parseReceiptCommitment(commitment);
  }
  if (reference === undefined || salt === undefined) {
    throw new Error("record-receipt requires --receipt-commitment or both --receipt-reference and --receipt-salt");
  }
  return receiptCommitment(reference, salt, merchant);
}

async function main(): Promise<void> {
  const { positionals, flags } = parse(process.argv.slice(2));
  const [group, action] = positionals;
  if (group === undefined) usage();

  if (group === "status") {
    const authority = parsePublicKey(required(flags, "authority"), "authority");
    const customer = parsePublicKey(required(flags, "customer"), "customer");
    const coalition = deriveCoalition(authority);
    const passport = derivePassport(coalition, customer);
    const program = await programForReadOnlyUse(rpcUrl(flags.rpc), flags.idl ?? DEFAULT_IDL_PATH);
    const accountClients = program.account as Record<string, { fetch: (key: PublicKey) => Promise<Record<string, unknown>> }>;
    const coalitionClient = accountClients.coalition;
    const passportClient = accountClients.passport;
    if (coalitionClient === undefined || passportClient === undefined) throw new Error("IDL is missing Coalition or Passport account clients");
    const [coalitionState, passportState] = await Promise.all([
      coalitionClient.fetch(coalition),
      passportClient.fetch(passport),
    ]);
    const tierCountValue = coalitionState.tierCount;
    if (typeof tierCountValue !== "number" || !Number.isInteger(tierCountValue) || tierCountValue < 1) {
      throw new Error("coalition tier count is invalid");
    }
    if (!Array.isArray(coalitionState.tierThresholds)) throw new Error("coalition tier thresholds are invalid");
    const thresholds = coalitionState.tierThresholds
      .slice(0, tierCountValue)
      .map((value, index) => integerValue(value, `tier threshold ${index}`));
    const streakPoints = integerValue(passportState.streakPoints, "Passport streak points");
    const level = tierLevelFor(streakPoints, thresholds);
    const result: Record<string, unknown> = {
      programId: PROGRAM_ID,
      coalition,
      passport,
      customer,
      paused: coalitionState.paused,
      totalVisits: passportState.totalVisits,
      streakPoints,
      tier: {
        level,
        reachedThresholds: thresholds.slice(0, level),
        nextThreshold: thresholds[level] ?? null,
      },
    };
    const merchantAuthorityValue = flags["merchant-authority"];
    if (merchantAuthorityValue !== undefined) {
      const merchantAuthority = parsePublicKey(merchantAuthorityValue, "merchant authority");
      const merchant = deriveMerchant(coalition, merchantAuthority);
      const balance = deriveBalance(passport, merchant);
      const balanceClient = accountClients.merchantBalance;
      if (balanceClient === undefined) throw new Error("IDL is missing MerchantBalance account client");
      const balanceState = await balanceClient.fetch(balance);
      const earned = integerValue(balanceState.earnedUnits, "earned units");
      const redeemed = integerValue(balanceState.redeemedUnits, "redeemed units");
      if (redeemed > earned) throw new Error("decoded merchant balance is corrupt");
      result.merchant = merchant;
      result.balance = balance;
      result.credit = { earned, redeemed, available: earned - redeemed };
    }
    output(result);
    return;
  }

  if (action === undefined) usage();

  if (group === "derive") {
    const authority = parsePublicKey(required(flags, "authority"), "authority");
    const coalition = deriveCoalition(authority);
    if (action === "coalition") output({ programId: PROGRAM_ID, coalition });
    else if (action === "merchant") output({ programId: PROGRAM_ID, merchant: deriveMerchant(coalition, parsePublicKey(required(flags, "merchant-authority"), "merchant authority")) });
    else if (action === "passport") output({ programId: PROGRAM_ID, passport: derivePassport(coalition, parsePublicKey(required(flags, "customer"), "customer")) });
    else if (action === "balance") {
      const merchant = deriveMerchant(coalition, parsePublicKey(required(flags, "merchant-authority"), "merchant authority"));
      const passport = derivePassport(coalition, parsePublicKey(required(flags, "customer"), "customer"));
      output({ programId: PROGRAM_ID, balance: deriveBalance(passport, merchant) });
    } else throw new Error(`unknown derivation: ${action}`);
    return;
  }

  if (group === "show") {
    const address = parsePublicKey(required(flags, "address"), "address");
    const program = await programForReadOnlyUse(rpcUrl(flags.rpc), flags.idl ?? DEFAULT_IDL_PATH);
    const account = (program.account as Record<string, { fetch: (key: typeof address) => Promise<unknown> }>)[accountClientName(action)];
    if (account === undefined) throw new Error(`IDL does not define account type ${action}`);
    output(await account.fetch(address));
    return;
  }

  if (group !== "build" && group !== "send") usage();
  const sending = group === "send";
  const rpc = rpcUrl(flags.rpc);
  const primaryKeypair = sending ? await loadExplicitSigner(required(flags, "signer-keypair")) : undefined;
  const signer = primaryKeypair?.publicKey ?? parsePublicKey(required(flags, "signer"), "signer");
  const program = await programForReadOnlyUse(rpc, flags.idl ?? DEFAULT_IDL_PATH);
  const methods = program.methods as Record<string, (...args: unknown[]) => { accounts: (accounts: Record<string, unknown>) => { instruction: () => Promise<import("@solana/web3.js").TransactionInstruction> } }>;
  const method = (name: string, ...args: unknown[]) => {
    const builder = methods[name];
    if (builder === undefined) throw new Error(`IDL does not define method ${name}`);
    return builder(...args);
  };
  const finish = async (
    instruction: TransactionInstruction,
    requiredSigners: PublicKey[],
    signers: Keypair[],
    extra: Record<string, unknown> = {},
  ): Promise<void> => {
    if (sending) {
      output({ ...(await sendDevnetInstruction(rpc, instruction, signers)), ...extra });
    } else {
      output({ ...printableInstruction(instruction, requiredSigners.map((key) => key.toBase58())), ...extra });
    }
  };

  if (action === "initialize-coalition") {
    const authority = signer;
    const { coalition } = deriveAddresses(authority, authority, authority);
    const instruction = await method("initializeCoalition",
      new BN(parseUnsigned(required(flags, "max-receipt-units"), "max receipt units").toString()),
      parseTierThresholds(required(flags, "tiers")).map((tier) => new BN(tier.toString())),
    ).accounts({ authority, coalition, systemProgram: SYSTEM_PROGRAM_ID }).instruction();
    await finish(instruction, [authority], [primaryKeypair!]);
    return;
  }

  if (action === "pause-coalition" || action === "unpause-coalition") {
    const authority = signer;
    const coalition = deriveCoalition(authority);
    const methodName = action === "pause-coalition" ? "pauseCoalition" : "unpauseCoalition";
    const instruction = await method(methodName).accounts({ authority, coalition }).instruction();
    await finish(instruction, [authority], [primaryKeypair!]);
    return;
  }

  if (action === "register-merchant") {
    const authority = signer;
    const merchantKeypair = sending ? await loadExplicitSigner(required(flags, "merchant-keypair")) : undefined;
    const merchantAuthority = merchantKeypair?.publicKey ??
      parsePublicKey(required(flags, "merchant-authority"), "merchant authority");
    const { coalition, merchant } = deriveAddresses(authority, merchantAuthority, authority);
    const instruction = await method("registerMerchant",
      parseU16(required(flags, "earn-bps"), "earn bps"),
      new BN(parseUnsigned(required(flags, "daily-cap"), "daily cap").toString()),
    ).accounts({ authority, coalition, merchantAuthority, merchant, systemProgram: SYSTEM_PROGRAM_ID }).instruction();
    await finish(instruction, [authority, merchantAuthority], [primaryKeypair!, merchantKeypair!]);
    return;
  }

  if (action === "create-passport") {
    const customer = signer;
    const authority = parsePublicKey(required(flags, "authority"), "authority");
    const mintKeypair = sending ? Keypair.generate() : undefined;
    const mint = mintKeypair?.publicKey ?? parsePublicKey(required(flags, "passport-mint"), "passport mint");
    if (mint.equals(customer)) throw new Error("passport mint signer must differ from the customer signer");
    const { coalition, passport } = deriveAddresses(authority, authority, customer);
    const passportToken = deriveToken2022Ata(customer, mint);
    const instruction = await method("createPassport").accounts({
      customer, coalition, passport, passportMint: mint, passportToken,
      tokenProgram: TOKEN_2022_PROGRAM_ID, associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID, systemProgram: SYSTEM_PROGRAM_ID,
    }).instruction();
    await finish(instruction, [customer, mint], [primaryKeypair!, mintKeypair!], { passportMint: mint });
    return;
  }

  if (action === "record-receipt") {
    const merchantAuthority = signer;
    const authority = parsePublicKey(required(flags, "authority"), "authority");
    const customer = parsePublicKey(required(flags, "customer"), "customer");
    const { coalition, merchant, passport, balance } = deriveAddresses(authority, merchantAuthority, customer);
    const instruction = await method("recordReceipt",
      new BN(parseUnsigned(required(flags, "nonce"), "nonce").toString()),
      new BN(parseUnsigned(required(flags, "amount"), "amount").toString()),
      receiptHash(flags, merchant),
    ).accounts({ merchantAuthority, coalition, merchant, passport, balance, systemProgram: SYSTEM_PROGRAM_ID }).instruction();
    await finish(instruction, [merchantAuthority], [primaryKeypair!]);
    return;
  }

  if (action === "redeem") {
    const customer = signer;
    const authority = parsePublicKey(required(flags, "authority"), "authority");
    const merchantAuthority = parsePublicKey(required(flags, "merchant-authority"), "merchant authority");
    const { coalition, merchant, passport, balance } = deriveAddresses(authority, merchantAuthority, customer);
    const instruction = await method("redeem", new BN(parseUnsigned(required(flags, "units"), "units").toString()))
      .accounts({ customer, coalition, passport, merchant, balance }).instruction();
    await finish(instruction, [customer], [primaryKeypair!]);
    return;
  }
  usage();
}

void main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`passport-cli: ${message}\n`);
  process.exitCode = 1;
});
