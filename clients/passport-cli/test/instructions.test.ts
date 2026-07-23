import { BN } from "@anchor-lang/core";
import { PublicKey } from "@solana/web3.js";
import { describe, expect, it } from "vitest";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  deriveAddresses,
  deriveToken2022Ata,
  programForReadOnlyUse,
  receiptCommitment,
} from "../src/core.js";

type MethodBuilder = {
  accounts: (accounts: Record<string, unknown>) => { instruction: () => Promise<{ data: Buffer }> };
};

describe("IDL instruction builders", () => {
  it("encodes every public transaction instruction without RPC or secret keys", async () => {
    const program = await programForReadOnlyUse("http://127.0.0.1:9");
    const methods = program.methods as Record<string, (...args: unknown[]) => MethodBuilder>;
    const method = (name: string, ...args: unknown[]): MethodBuilder => {
      const builder = methods[name];
      if (builder === undefined) throw new Error(`missing IDL method ${name}`);
      return builder(...args);
    };
    const authority = new PublicKey("11111111111111111111111111111111");
    const merchantAuthority = new PublicKey("SysvarRent111111111111111111111111111111111");
    const customer = new PublicKey("SysvarC1ock11111111111111111111111111111111");
    const mint = new PublicKey("SysvarS1otHashes111111111111111111111111111");
    const { coalition, merchant, passport, balance } = deriveAddresses(authority, merchantAuthority, customer);
    const passportToken = deriveToken2022Ata(customer, mint);

    const instructions = await Promise.all([
      method("initializeCoalition", new BN("50"), [new BN("10"), new BN("25")])
        .accounts({ authority, coalition, systemProgram: SYSTEM_PROGRAM_ID }).instruction(),
      method("pauseCoalition").accounts({ authority, coalition }).instruction(),
      method("unpauseCoalition").accounts({ authority, coalition }).instruction(),
      method("registerMerchant", 1_000, new BN("500"))
        .accounts({ authority, coalition, merchantAuthority, merchant, systemProgram: SYSTEM_PROGRAM_ID }).instruction(),
      method("createPassport").accounts({
        customer, coalition, passport, passportMint: mint, passportToken,
        tokenProgram: TOKEN_2022_PROGRAM_ID, associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID, systemProgram: SYSTEM_PROGRAM_ID,
      }).instruction(),
      method("recordReceipt", new BN("1"), new BN("120"), receiptCommitment("order-123", "random-salt", merchant))
        .accounts({ merchantAuthority, coalition, merchant, passport, balance, systemProgram: SYSTEM_PROGRAM_ID }).instruction(),
      method("redeem", new BN("10")).accounts({ customer, coalition, passport, merchant, balance }).instruction(),
    ]);

    expect(instructions).toHaveLength(7);
    for (const instruction of instructions) expect(instruction.data.length).toBeGreaterThanOrEqual(8);
  });
});
