"use client";

import {
  createSolanaRpc,
} from "@solana/kit";
import { useMemo, useState } from "react";

import {
  INITIAL_PASSPORT,
  MERCHANTS,
  TIERS,
  accrueReceipt,
  redeemCredit,
  tierFor,
  type MerchantId,
  type PassportState,
} from "../lib/passport-model";
import {
  DEMO_DEPLOYER,
  PROGRAM_ID,
  deriveDemoAccounts,
} from "../lib/solana-addresses";

const DEVNET_RPC = "https://api.devnet.solana.com" as const;
const REPOSITORY =
  "https://github.com/spoconymacius3879254-ctrl/coalition-passport";

type ChainCheck = {
  programDeployed: boolean;
  programBytes: number;
  deployerSol: number;
  coalition: string;
  passport: string;
  merchant: string;
  balance: string;
  checkedAt: string;
};

function short(value: string) {
  return `${value.slice(0, 5)}…${value.slice(-5)}`;
}

export default function PassportDemo() {
  const [passport, setPassport] = useState<PassportState>(INITIAL_PASSPORT);
  const [merchantId, setMerchantId] = useState<MerchantId>("orbit");
  const [receiptUnits, setReceiptUnits] = useState(120);
  const [message, setMessage] = useState(
    "Choose a merchant and record an example receipt. This explainer never signs or submits.",
  );
  const [chain, setChain] = useState<ChainCheck | null>(null);
  const [checking, setChecking] = useState(false);
  const [chainError, setChainError] = useState("");
  const tier = tierFor(passport.streak);
  const nextTier = TIERS.find((entry) => entry.threshold > passport.streak);
  const progress = nextTier
    ? Math.min(100, (passport.streak / nextTier.threshold) * 100)
    : 100;
  const selectedMerchant = useMemo(
    () => MERCHANTS.find((merchant) => merchant.id === merchantId) ?? MERCHANTS[0],
    [merchantId],
  );

  function recordReceipt() {
    try {
      const outcome = accrueReceipt(passport, merchantId, receiptUnits);
      setPassport(outcome.next);
      setMessage(
        `${selectedMerchant.name} signed nonce ${outcome.nonce}: +${outcome.credit} local credit, +${outcome.streakDelta} portable streak.`,
      );
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Receipt could not be applied");
    }
  }

  function redeem(merchant: MerchantId) {
    try {
      setPassport((current) => redeemCredit(current, merchant, 1));
      const issuer = MERCHANTS.find((entry) => entry.id === merchant);
      setMessage(`${issuer?.name ?? "Merchant"} redeemed one unit from its own ledger.`);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Credit could not be redeemed");
    }
  }

  async function checkDevnet() {
    setChecking(true);
    setChainError("");
    try {
      const rpc = createSolanaRpc(DEVNET_RPC);
      const accounts = await deriveDemoAccounts();
      const [program, balance] = await Promise.all([
        rpc.getAccountInfo(PROGRAM_ID, {
          commitment: "confirmed",
          encoding: "base64",
        }).send(),
        rpc.getBalance(DEMO_DEPLOYER, { commitment: "confirmed" }).send(),
      ]);
      setChain({
        programDeployed: Boolean(program.value?.executable),
        programBytes: Number(program.value?.space ?? 0),
        deployerSol: Number(balance.value) / 1_000_000_000,
        coalition: accounts.coalition,
        passport: accounts.passport,
        merchant: accounts.merchant,
        balance: accounts.balance,
        checkedAt: new Date().toISOString(),
      });
    } catch (error) {
      setChainError(
        error instanceof Error ? error.message : "Devnet RPC check failed",
      );
    } finally {
      setChecking(false);
    }
  }

  return (
    <main>
      <nav aria-label="Primary navigation">
        <a className="wordmark" href="#top" aria-label="Coalition Passport home">
          <span aria-hidden="true">CP</span>
          Coalition Passport
        </a>
        <div>
          <a href="#journey">Try the model</a>
          <a href="#devnet">Verify Devnet</a>
          <a className="repo-link" href={REPOSITORY}>
            Source ↗
          </a>
        </div>
      </nav>

      <section className="hero" id="top">
        <div className="eyebrow">SOLANA · TOKEN-2022 · OPEN SOURCE</div>
        <h1>
          Loyalty that travels.
          <br />
          Liability that doesn&apos;t.
        </h1>
        <p className="lede">
          A soulbound neighbourhood Passport turns verified visits into portable
          reputation, while every shop keeps its own credits and redemption ledger.
        </p>
        <div className="hero-actions">
          <a className="primary" href="#journey">
            Run a receipt
          </a>
          <a className="secondary" href="#architecture">
            See the boundary
          </a>
        </div>
        <div className="passport-card" aria-label="Example loyalty Passport">
          <div className="passport-topline">
            <span>NEIGHBOURHOOD PASSPORT</span>
            <span>NON-TRANSFERABLE</span>
          </div>
          <div className="passport-mark" aria-hidden="true">
            <i />
            <i />
            <i />
          </div>
          <div>
            <small>MEMBER</small>
            <strong>78tW…B6W6E</strong>
          </div>
          <div>
            <small>PORTABLE TIER</small>
            <strong>{tier.name.toUpperCase()}</strong>
          </div>
          <div>
            <small>ISSUED ON</small>
            <strong>SOLANA DEVNET</strong>
          </div>
        </div>
      </section>

      <section className="principle" id="architecture">
        <div>
          <span className="section-number">01</span>
          <h2>One identity signal.<br />Three isolated ledgers.</h2>
        </div>
        <p>
          The Passport&apos;s streak is readable by any partner app. Spendable credit
          remains inside the PDA of the merchant that issued it. A bookstore can
          recognize a café regular without owing the café a cent.
        </p>
      </section>

      <section className="journey" id="journey">
        <div className="section-heading">
          <div>
            <span className="section-number">02</span>
            <h2>Receipt lab</h2>
          </div>
          <p>Local deterministic explainer · no wallet · no transaction</p>
        </div>

        <div className="lab-grid">
          <div className="controls">
            <label htmlFor="merchant">Merchant signer</label>
            <div className="merchant-picker" id="merchant">
              {MERCHANTS.map((merchant) => (
                <button
                  className={merchant.id === merchantId ? "selected" : ""}
                  key={merchant.id}
                  onClick={() => setMerchantId(merchant.id)}
                  style={{ "--merchant": merchant.accent } as React.CSSProperties}
                  type="button"
                >
                  <span>{merchant.name}</span>
                  <small>{merchant.earnBps / 100}% earn rate</small>
                </button>
              ))}
            </div>
            <label htmlFor="receipt-units">Receipt units</label>
            <div className="amount-row">
              <input
                id="receipt-units"
                min="1"
                onChange={(event) => setReceiptUnits(Number(event.target.value))}
                step="1"
                type="number"
                value={receiptUnits}
              />
              <button className="primary" onClick={recordReceipt} type="button">
                Record receipt
              </button>
            </div>
            <output className="event-log" aria-live="polite">
              <span>Latest event</span>
              {message}
            </output>
          </div>

          <div className="passport-state">
            <div className="tier-row">
              <div>
                <span>Portable reputation</span>
                <strong>{tier.name}</strong>
              </div>
              <div>
                <span>Streak</span>
                <strong>{passport.streak}</strong>
              </div>
              <div>
                <span>Visits</span>
                <strong>{passport.visits}</strong>
              </div>
            </div>
            <div className="progress-track" aria-label={`${progress}% tier progress`}>
              <i style={{ width: `${progress}%` }} />
            </div>
            <p className="next-tier">
              {nextTier
                ? `${nextTier.threshold - passport.streak} points to ${nextTier.name}`
                : "Highest coalition tier reached"}
            </p>
            <div className="balances">
              {MERCHANTS.map((merchant) => (
                <article key={merchant.id}>
                  <i style={{ background: merchant.accent }} />
                  <div>
                    <span>{merchant.name}</span>
                    <strong>{passport.balances[merchant.id]} credits</strong>
                  </div>
                  <button
                    disabled={passport.balances[merchant.id] < 1}
                    onClick={() => redeem(merchant.id)}
                    type="button"
                  >
                    Redeem 1
                  </button>
                </article>
              ))}
            </div>
          </div>
        </div>
      </section>

      <section className="boundary">
        <div>
          <span>PORTABLE</span>
          <h3>Streak + tier</h3>
          <p>Readable by games, ticketing, communities, and partner merchants.</p>
        </div>
        <div className="boundary-lock" aria-hidden="true">≠</div>
        <div>
          <span>ISSUER-LOCAL</span>
          <h3>Redeemable credit</h3>
          <p>Writable and redeemable only through the merchant&apos;s bound PDA.</p>
        </div>
      </section>

      <section className="devnet" id="devnet">
        <div className="section-heading">
          <div>
            <span className="section-number">03</span>
            <h2>Public-chain check</h2>
          </div>
          <button className="secondary" disabled={checking} onClick={checkDevnet} type="button">
            {checking ? "Checking…" : "Query Solana Devnet"}
          </button>
        </div>
        <p className="devnet-intro">
          This read-only check derives the same public PDAs as the CLI and queries
          the canonical Devnet RPC. It never asks for a wallet or signing permission.
        </p>
        {chainError && <p className="error" role="alert">{chainError}</p>}
        <div className="chain-grid">
          <article>
            <span>Program</span>
            <strong>{chain ? (chain.programDeployed ? "Executable" : "Awaiting deployment") : "Not checked"}</strong>
            <a href={`https://explorer.solana.com/address/${PROGRAM_ID}?cluster=devnet`}>
              {short(PROGRAM_ID)} ↗
            </a>
          </article>
          <article>
            <span>Deployer balance</span>
            <strong>{chain ? `${chain.deployerSol.toFixed(4)} SOL` : "—"}</strong>
            <code>{short(DEMO_DEPLOYER)}</code>
          </article>
          <article>
            <span>Coalition PDA</span>
            <strong>{chain ? short(chain.coalition) : "Derived on check"}</strong>
            <code>{chain?.coalition ?? "coalition + authority"}</code>
          </article>
          <article>
            <span>Passport PDA</span>
            <strong>{chain ? short(chain.passport) : "Derived on check"}</strong>
            <code>{chain?.passport ?? "passport + customer"}</code>
          </article>
        </div>
        {chain && (
          <p className="checked-at">
            Confirmed RPC response at {new Date(chain.checkedAt).toLocaleString()}.
            Program data: {chain.programBytes.toLocaleString()} bytes.
          </p>
        )}
      </section>

      <footer>
        <div>
          <strong>Coalition Passport</strong>
          <p>Devnet demonstration software. Not audited or mainnet-ready.</p>
        </div>
        <div>
          <a href={REPOSITORY}>GitHub</a>
          <a href={`${REPOSITORY}/blob/main/README.md`}>Architecture</a>
          <a href={`${REPOSITORY}/actions`}>CI evidence</a>
        </div>
      </footer>
    </main>
  );
}
