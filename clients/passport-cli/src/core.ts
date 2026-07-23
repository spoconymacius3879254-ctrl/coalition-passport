import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  clusterApiUrl,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import { Idl, Program } from "@anchor-lang/core";

export const PROGRAM_ID = new PublicKey("2A2227YnW1PEr6FrMLxZrjm8B3P3fHWQjjqM8tDNhxg6");
export const TOKEN_2022_PROGRAM_ID = new PublicKey("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
export const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
export const SYSTEM_PROGRAM_ID = new PublicKey("11111111111111111111111111111111");
export const DEVNET_GENESIS_HASH = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";

const moduleDirectory = dirname(fileURLToPath(import.meta.url));
const packageDirectory = basename(dirname(moduleDirectory)) === "dist"
  ? resolve(moduleDirectory, "../..")
  : resolve(moduleDirectory, "..");
export const DEFAULT_IDL_PATH = resolve(packageDirectory, "idl/coalition_passport.json");

export type Addresses = {
  coalition: PublicKey;
  merchant: PublicKey;
  passport: PublicKey;
  balance: PublicKey;
};

export function deriveCoalition(authority: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([Buffer.from("coalition"), authority.toBuffer()], PROGRAM_ID)[0];
}

export function deriveMerchant(coalition: PublicKey, merchantAuthority: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("merchant"), coalition.toBuffer(), merchantAuthority.toBuffer()],
    PROGRAM_ID,
  )[0];
}

export function derivePassport(coalition: PublicKey, customer: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("passport"), coalition.toBuffer(), customer.toBuffer()],
    PROGRAM_ID,
  )[0];
}

export function deriveBalance(passport: PublicKey, merchant: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("balance"), passport.toBuffer(), merchant.toBuffer()],
    PROGRAM_ID,
  )[0];
}

export function parsePublicKey(value: string, label: string): PublicKey {
  try {
    return new PublicKey(value);
  } catch {
    throw new Error(`${label} must be a valid base58 Solana public key`);
  }
}

export function parseUnsigned(value: string, label: string, maximum = (1n << 64n) - 1n): bigint {
  if (!/^(0|[1-9][0-9]*)$/.test(value)) throw new Error(`${label} must be an unsigned integer`);
  const parsed = BigInt(value);
  if (parsed > maximum) throw new Error(`${label} must be at most ${maximum}`);
  return parsed;
}

export function parseU16(value: string, label: string): number {
  const parsed = parseUnsigned(value, label, 65_535n);
  return Number(parsed);
}

export function parseTierThresholds(value: string): bigint[] {
  const tiers = value.split(",").map((part) => parseUnsigned(part.trim(), "tier threshold"));
  if (tiers.length === 0 || tiers.length > 16) throw new Error("tier thresholds must contain 1 to 16 values");
  if (tiers[0] === 0n) throw new Error("tier thresholds must be greater than zero");
  for (let index = 1; index < tiers.length; index += 1) {
    const previous = tiers[index - 1];
    const current = tiers[index];
    if (previous === undefined || current === undefined || previous >= current) {
      throw new Error("tier thresholds must be strictly increasing");
    }
  }
  return tiers;
}

export function receiptCommitment(reference: string, salt: string, merchant: PublicKey): number[] {
  if (reference.trim().length === 0) throw new Error("receipt reference must not be empty");
  if (salt.trim().length === 0) throw new Error("receipt salt must not be empty");
  return [
    ...createHash("sha256")
      .update("coalition-passport-receipt-v1\0", "utf8")
      .update(PROGRAM_ID.toBuffer())
      .update(merchant.toBuffer())
      .update("\0", "utf8")
      .update(salt, "utf8")
      .update("\0", "utf8")
      .update(reference, "utf8")
      .digest(),
  ];
}

export function parseReceiptCommitment(value: string): number[] {
  if (!/^[0-9a-fA-F]{64}$/.test(value)) throw new Error("receipt commitment must be exactly 64 hexadecimal characters");
  const bytes = [...Buffer.from(value, "hex")];
  if (bytes.every((byte) => byte === 0)) throw new Error("receipt commitment must not be all zeroes");
  return bytes;
}

export function deriveAddresses(authority: PublicKey, merchantAuthority: PublicKey, customer: PublicKey): Addresses {
  const coalition = deriveCoalition(authority);
  const merchant = deriveMerchant(coalition, merchantAuthority);
  const passport = derivePassport(coalition, customer);
  const balance = deriveBalance(passport, merchant);
  return { coalition, merchant, passport, balance };
}

export function deriveToken2022Ata(customer: PublicKey, mint: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [customer.toBuffer(), TOKEN_2022_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID,
  )[0];
}

export function rpcUrl(value?: string): string {
  return value ?? clusterApiUrl("devnet");
}

export async function loadIdl(path = DEFAULT_IDL_PATH): Promise<Idl> {
  const parsed: unknown = JSON.parse(await readFile(path, "utf8"));
  if (typeof parsed !== "object" || parsed === null || !("address" in parsed)) throw new Error("IDL is not a valid Anchor IDL");
  return parsed as Idl;
}

export async function programForReadOnlyUse(rpc: string, idlPath?: string): Promise<Program> {
  const idl = await loadIdl(idlPath);
  return new Program(idl, { connection: new Connection(rpc, "confirmed") });
}

/** Reads only a caller-named Solana keypair. There is deliberately no default. */
export async function loadExplicitSigner(path: string): Promise<Keypair> {
  const parsed: unknown = JSON.parse(await readFile(path, "utf8"));
  if (
    !Array.isArray(parsed) ||
    parsed.length !== 64 ||
    parsed.some((byte) => !Number.isInteger(byte) || byte < 0 || byte > 255)
  ) {
    throw new Error("signer keypair must be a 64-byte JSON array");
  }
  try {
    return Keypair.fromSecretKey(Uint8Array.from(parsed));
  } catch {
    throw new Error("signer keypair is not a valid Solana keypair");
  }
}

export async function sendDevnetInstruction(
  rpc: string,
  instruction: TransactionInstruction,
  signers: Keypair[],
): Promise<object> {
  if (signers.length === 0) throw new Error("at least one explicit signer is required");
  const connection = new Connection(rpc, "confirmed");
  const genesisHash = await connection.getGenesisHash();
  if (genesisHash !== DEVNET_GENESIS_HASH) {
    throw new Error(`refusing to submit: RPC genesis hash ${genesisHash} is not Solana Devnet`);
  }
  const signature = await sendAndConfirmTransaction(
    connection,
    new Transaction().add(instruction),
    signers,
    { commitment: "confirmed", preflightCommitment: "confirmed" },
  );
  return {
    mode: "sent",
    cluster: "devnet",
    signature,
    explorerUrl: `https://explorer.solana.com/tx/${signature}?cluster=devnet`,
  };
}

export function printableInstruction(instruction: TransactionInstruction, requiredSigners: string[]): object {
  return {
    mode: "build-only",
    warning: "This command did not sign, fetch a blockhash, simulate, or submit a transaction.",
    programId: instruction.programId.toBase58(),
    accounts: instruction.keys.map((key) => ({
      pubkey: key.pubkey.toBase58(),
      isSigner: key.isSigner,
      isWritable: key.isWritable,
    })),
    dataBase64: instruction.data.toString("base64"),
    requiredSigners,
  };
}

export function jsonSafe(value: unknown): unknown {
  if (typeof value === "bigint") return value.toString();
  if (value instanceof PublicKey) return value.toBase58();
  if (Array.isArray(value)) return value.map(jsonSafe);
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([key, entry]) => [key, jsonSafe(entry)]));
  }
  return value;
}
