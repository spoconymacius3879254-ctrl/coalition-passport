export const TIERS = [
  { threshold: 0, name: "Neighbour", perk: "Local credit only" },
  { threshold: 10, name: "Regular", perk: "Partner welcome perks" },
  { threshold: 30, name: "Local", perk: "Coalition express lane" },
  { threshold: 100, name: "Cornerstone", perk: "Community-level access" },
] as const;

export const MERCHANTS = [
  {
    id: "orbit",
    name: "Orbit Coffee",
    category: "Coffee",
    earnBps: 1000,
    dailyCap: 50,
    accent: "#f2a65a",
  },
  {
    id: "folio",
    name: "Folio Books",
    category: "Books",
    earnBps: 600,
    dailyCap: 40,
    accent: "#97d8c4",
  },
  {
    id: "grove",
    name: "Grove Market",
    category: "Groceries",
    earnBps: 400,
    dailyCap: 30,
    accent: "#c6a7ff",
  },
] as const;

export type MerchantId = (typeof MERCHANTS)[number]["id"];

export type PassportState = {
  visits: number;
  streak: number;
  balances: Record<MerchantId, number>;
  earnedToday: Record<MerchantId, number>;
  nextNonce: Record<MerchantId, number>;
};

export type ReceiptOutcome = {
  next: PassportState;
  credit: number;
  streakDelta: number;
  nonce: number;
};

export const INITIAL_PASSPORT: PassportState = {
  visits: 0,
  streak: 0,
  balances: { orbit: 0, folio: 0, grove: 0 },
  earnedToday: { orbit: 0, folio: 0, grove: 0 },
  nextNonce: { orbit: 1, folio: 1, grove: 1 },
};

export function tierFor(streak: number) {
  return [...TIERS].reverse().find((tier) => streak >= tier.threshold) ?? TIERS[0];
}

export function accrueReceipt(
  state: PassportState,
  merchantId: MerchantId,
  receiptUnits: number,
): ReceiptOutcome {
  const merchant = MERCHANTS.find((entry) => entry.id === merchantId);
  if (!merchant) throw new Error("Unknown merchant");
  if (!Number.isSafeInteger(receiptUnits) || receiptUnits <= 0) {
    throw new Error("Receipt units must be a positive integer");
  }

  const rawCredit = Math.floor((receiptUnits * merchant.earnBps) / 10_000);
  const remainingCap = Math.max(0, merchant.dailyCap - state.earnedToday[merchantId]);
  const credit = Math.min(rawCredit, remainingCap);
  if (credit <= 0) throw new Error("This receipt earns no credit or the daily cap is exhausted");

  const streakDelta = Math.min(credit, 50);
  const nonce = state.nextNonce[merchantId];
  return {
    credit,
    streakDelta,
    nonce,
    next: {
      visits: state.visits + 1,
      streak: state.streak + streakDelta,
      balances: {
        ...state.balances,
        [merchantId]: state.balances[merchantId] + credit,
      },
      earnedToday: {
        ...state.earnedToday,
        [merchantId]: state.earnedToday[merchantId] + credit,
      },
      nextNonce: {
        ...state.nextNonce,
        [merchantId]: nonce + 1,
      },
    },
  };
}

export function redeemCredit(
  state: PassportState,
  merchantId: MerchantId,
  units: number,
): PassportState {
  if (!Number.isSafeInteger(units) || units <= 0) {
    throw new Error("Redemption must be a positive integer");
  }
  if (state.balances[merchantId] < units) {
    throw new Error("A merchant can redeem only credit it issued");
  }
  return {
    ...state,
    balances: {
      ...state.balances,
      [merchantId]: state.balances[merchantId] - units,
    },
  };
}
