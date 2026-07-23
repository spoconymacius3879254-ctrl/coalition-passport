import { describe, expect, it } from "vitest";

import { deriveDemoAccounts } from "../lib/solana-addresses";

describe("browser PDA derivation", () => {
  it("matches the independently tested Anchor CLI", async () => {
    await expect(deriveDemoAccounts()).resolves.toEqual({
      coalition: "9wkJAfNGMhFftqMQfYiKa12bca7afLhYVnfQVS7EESzJ",
      merchant: "76Z2JsLwBZeupMQt2LUtxYmAw5DbE1m6j5eQe82kFYi1",
      passport: "F6vcWmQviVMU1TKifz4VEXMUenPPQENzKgmijzqhAphH",
      balance: "Fz7SvmKwauYszQAysSdU9RyTmUc877tPP6UBxhzav52M",
    });
  });
});
