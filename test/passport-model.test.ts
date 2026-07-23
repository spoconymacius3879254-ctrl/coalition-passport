import { describe, expect, it } from "vitest";

import {
  INITIAL_PASSPORT,
  accrueReceipt,
  redeemCredit,
  tierFor,
} from "../lib/passport-model";

describe("passport explainer model", () => {
  it("matches the Rust demo fixture for a 120-unit cafe receipt", () => {
    const outcome = accrueReceipt(INITIAL_PASSPORT, "orbit", 120);

    expect(outcome.credit).toBe(12);
    expect(outcome.streakDelta).toBe(12);
    expect(outcome.next.balances.orbit).toBe(12);
    expect(tierFor(outcome.next.streak).name).toBe("Regular");
  });

  it("keeps merchant liabilities isolated", () => {
    const earned = accrueReceipt(INITIAL_PASSPORT, "orbit", 120).next;

    expect(() => redeemCredit(earned, "folio", 1)).toThrow(
      "A merchant can redeem only credit it issued",
    );
    expect(redeemCredit(earned, "orbit", 10).balances.orbit).toBe(2);
  });

  it("caps merchant credit without capping portable progress retroactively", () => {
    const first = accrueReceipt(INITIAL_PASSPORT, "orbit", 4900).next;
    expect(first.balances.orbit).toBe(50);
    expect(first.streak).toBe(50);
    expect(() => accrueReceipt(first, "orbit", 100)).toThrow("daily cap");
  });
});
