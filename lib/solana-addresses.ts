import {
  address,
  getAddressEncoder,
  getProgramDerivedAddress,
} from "@solana/kit";

export const PROGRAM_ID = address(
  "2A2227YnW1PEr6FrMLxZrjm8B3P3fHWQjjqM8tDNhxg6",
);
export const DEMO_DEPLOYER = address(
  "Edk1vBTRxCeqxYmytWJcBYCxBxHsh4jXFNrVChdyLxc",
);
export const DEMO_MERCHANT = address(
  "4jxSCAiu779tcTzSdYymT8jY4uYArJBhDsssfLGfFAgd",
);
export const DEMO_CUSTOMER = address(
  "78tWfaJM1T9BmvPNfafJ64fYFWH2NJ9tKyHzNZ2B6W6E",
);

export async function deriveDemoAccounts() {
  const encoder = getAddressEncoder();
  const [coalition] = await getProgramDerivedAddress({
    seeds: ["coalition", encoder.encode(DEMO_DEPLOYER)],
    programAddress: PROGRAM_ID,
  });
  const [merchant] = await getProgramDerivedAddress({
    seeds: [
      "merchant",
      encoder.encode(coalition),
      encoder.encode(DEMO_MERCHANT),
    ],
    programAddress: PROGRAM_ID,
  });
  const [passport] = await getProgramDerivedAddress({
    seeds: [
      "passport",
      encoder.encode(coalition),
      encoder.encode(DEMO_CUSTOMER),
    ],
    programAddress: PROGRAM_ID,
  });
  const [balance] = await getProgramDerivedAddress({
    seeds: ["balance", encoder.encode(passport), encoder.encode(merchant)],
    programAddress: PROGRAM_ID,
  });
  return { coalition, merchant, passport, balance };
}
