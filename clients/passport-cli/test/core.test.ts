import { PublicKey } from "@solana/web3.js";
import { describe, expect, it } from "vitest";
import { DEVNET_GENESIS_HASH, deriveAddresses, deriveBalance, deriveCoalition, deriveMerchant, derivePassport, parseReceiptCommitment, parseTierThresholds, parseUnsigned, receiptCommitment } from "../src/core.js";

describe("input parsing", () => {
  it("pins the complete Solana Devnet genesis hash", () => {
    expect(DEVNET_GENESIS_HASH).toBe("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG");
  });

  it("rejects signed, fractional, and overflowing u64 values", () => {
    expect(() => parseUnsigned("-1", "value")).toThrow();
    expect(() => parseUnsigned("1.5", "value")).toThrow();
    expect(() => parseUnsigned("18446744073709551616", "value")).toThrow();
    expect(parseUnsigned("18446744073709551615", "value")).toBe(18_446_744_073_709_551_615n);
  });

  it("requires nonzero, strictly increasing bounded tiers", () => {
    expect(parseTierThresholds("1,10,25")).toEqual([1n, 10n, 25n]);
    expect(() => parseTierThresholds("0,1")).toThrow();
    expect(() => parseTierThresholds("5,5")).toThrow();
    expect(() => parseTierThresholds("10,9")).toThrow();
  });
});

describe("PDA and commitment helpers", () => {
  it("derives stable, merchant-isolated addresses", () => {
    const authority = new PublicKey("11111111111111111111111111111111");
    const customer = new PublicKey("SysvarC1ock11111111111111111111111111111111");
    const merchantA = new PublicKey("SysvarRent111111111111111111111111111111111");
    const merchantB = new PublicKey("SysvarS1otHashes111111111111111111111111111");
    const first = deriveAddresses(authority, merchantA, customer);
    expect(deriveAddresses(authority, merchantA, customer)).toEqual(first);
    expect(deriveAddresses(authority, merchantB, customer).balance).not.toEqual(first.balance);
    expect(first.coalition).toEqual(deriveCoalition(authority));
    expect(first.merchant).toEqual(deriveMerchant(first.coalition, merchantA));
    expect(first.passport).toEqual(derivePassport(first.coalition, customer));
    expect(first.balance).toEqual(deriveBalance(first.passport, first.merchant));
  });

  it("hashes receipt references deterministically without logging them", () => {
    const merchant = new PublicKey("SysvarRent111111111111111111111111111111111");
    expect(receiptCommitment("opaque-order-reference", "unique salt", merchant)).toEqual(receiptCommitment("opaque-order-reference", "unique salt", merchant));
    expect(receiptCommitment("opaque-order-reference", "unique salt", merchant)).not.toEqual(receiptCommitment("opaque-order-reference", "other salt", merchant));
    expect(receiptCommitment("opaque-order-reference", "unique salt", merchant)).toHaveLength(32);
    expect(parseReceiptCommitment("01".repeat(32))).toHaveLength(32);
    expect(() => parseReceiptCommitment("00".repeat(32))).toThrow();
  });
});
