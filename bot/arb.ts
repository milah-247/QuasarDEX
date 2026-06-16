/**
 * QuasarDEX Arbitrage Bot
 * Compares QuasarDEX AMM price vs Stellar native DEX (SDEX) order book.
 * Executes a swap when the spread exceeds MIN_SPREAD_PCT.
 */

import {
  Keypair, Networks, TransactionBuilder, BASE_FEE,
  Operation, Asset, Horizon, StellarToml,
  Contract, nativeToScVal, rpc, xdr,
} from "@stellar/stellar-sdk";

// ── Config ────────────────────────────────────────────────────────────────────

const RPC_URL = process.env.RPC_URL ?? "https://soroban-testnet.stellar.org";
const HORIZON_URL = process.env.HORIZON_URL ?? "https://horizon-testnet.stellar.org";
const CONTRACT_ID = process.env.CONTRACT_ID!;
const SECRET_KEY = process.env.TRADER_SECRET!;
const XLM_ADDR = process.env.XLM_CONTRACT_ADDR!;
const USDC_ADDR = process.env.USDC_CONTRACT_ADDR!;
const MIN_SPREAD_PCT = parseFloat(process.env.MIN_SPREAD_PCT ?? "0.5");
const TRADE_AMOUNT_XLM = parseFloat(process.env.TRADE_AMOUNT_XLM ?? "100"); // XLM to trade
const POLL_INTERVAL_MS = parseInt(process.env.POLL_INTERVAL_MS ?? "5000");
const SCALE = 1e7;

const keypair = Keypair.fromSecret(SECRET_KEY);
const server = new rpc.Server(RPC_URL);
const horizon = new Horizon.Server(HORIZON_URL);
const xlm = Asset.native();
const usdc = new Asset("USDC", process.env.USDC_ISSUER ?? "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5");

// ── Prices ────────────────────────────────────────────────────────────────────

/** QuasarDEX AMM spot price: USDC per XLM, scaled 1e7 */
async function getAmmPrice(): Promise<number> {
  const account = await server.getAccount(keypair.publicKey());
  const contract = new Contract(CONTRACT_ID);
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: Networks.TESTNET })
    .addOperation(contract.call("get_price", nativeToScVal(XLM_ADDR, { type: "address" }), nativeToScVal(USDC_ADDR, { type: "address" })))
    .setTimeout(30).build();
  const sim = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) throw new Error(sim.error);
  const raw = (sim as rpc.Api.SimulateTransactionSuccessResponse).result!.retval;
  return Number(xdr.ScVal.fromXDR(raw.toXDR()).i128().lo()) / SCALE;
}

/** Stellar SDEX best-ask price: USDC per XLM from order book */
async function getSdexPrice(): Promise<number> {
  const book = await horizon.orderbook(xlm, usdc).call();
  if (book.asks.length === 0) throw new Error("empty order book");
  return parseFloat(book.asks[0].price);
}

// ── Trade ─────────────────────────────────────────────────────────────────────

/** Execute swap on QuasarDEX: sell XLM for USDC */
async function executeAmmSwap(amountIn: number, minOut: number): Promise<string> {
  const account = await server.getAccount(keypair.publicKey());
  const contract = new Contract(CONTRACT_ID);
  const amtScaled = BigInt(Math.floor(amountIn * SCALE));
  const minScaled = BigInt(Math.floor(minOut * SCALE));

  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: Networks.TESTNET })
    .addOperation(contract.call(
      "swap",
      nativeToScVal(keypair.publicKey(), { type: "address" }),
      nativeToScVal(XLM_ADDR, { type: "address" }),
      nativeToScVal(USDC_ADDR, { type: "address" }),
      nativeToScVal(amtScaled, { type: "i128" }),
      nativeToScVal(minScaled, { type: "i128" }),
    ))
    .setTimeout(30).build();

  const sim = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) throw new Error(`Sim failed: ${sim.error}`);
  const prepared = rpc.assembleTransaction(tx, sim).build();
  prepared.sign(keypair);
  const result = await server.sendTransaction(prepared);
  return result.hash;
}

/** Execute path payment on SDEX: sell USDC for XLM */
async function executeSdexBuy(usdcAmount: number, minXlm: number): Promise<string> {
  const account = await horizon.loadAccount(keypair.publicKey());
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: Networks.TESTNET })
    .addOperation(Operation.pathPaymentStrictSend({
      sendAsset: usdc,
      sendAmount: usdcAmount.toFixed(7),
      destination: keypair.publicKey(),
      destAsset: xlm,
      destMin: minXlm.toFixed(7),
      path: [],
    }))
    .setTimeout(30).build();
  tx.sign(keypair);
  const result = await (horizon.submitTransaction(tx) as Promise<Horizon.HorizonApi.SubmitTransactionResponse>);
  return result.hash;
}

// ── Arbitrage logic ───────────────────────────────────────────────────────────

async function checkAndArbitrage() {
  let ammPrice: number, sdexPrice: number;
  try {
    [ammPrice, sdexPrice] = await Promise.all([getAmmPrice(), getSdexPrice()]);
  } catch (e) {
    console.error("[arb] price fetch error:", e);
    return;
  }

  const spread = ((ammPrice - sdexPrice) / sdexPrice) * 100;
  const ts = new Date().toISOString();
  console.log(`[${ts}] AMM=${ammPrice.toFixed(6)} SDEX=${sdexPrice.toFixed(6)} spread=${spread.toFixed(3)}%`);

  if (Math.abs(spread) < MIN_SPREAD_PCT) return;

  // Strategy A: AMM price > SDEX → buy on SDEX, sell on AMM
  if (spread > MIN_SPREAD_PCT) {
    console.log("[arb] AMM overpriced. Buy XLM on SDEX, sell on AMM.");
    const usdcCost = TRADE_AMOUNT_XLM * sdexPrice;
    const minXlm = TRADE_AMOUNT_XLM * 0.995; // 0.5% slippage
    const minUsdc = TRADE_AMOUNT_XLM * ammPrice * 0.995;
    try {
      const h1 = await executeSdexBuy(usdcCost, minXlm);
      console.log(`[arb] SDEX buy tx: ${h1}`);
      const h2 = await executeAmmSwap(TRADE_AMOUNT_XLM, minUsdc);
      console.log(`[arb] AMM sell tx: ${h2}`);
    } catch (e) {
      console.error("[arb] execution failed:", e);
    }
  }

  // Strategy B: SDEX price > AMM → buy on AMM, sell on SDEX
  if (spread < -MIN_SPREAD_PCT) {
    console.log("[arb] SDEX overpriced. Buy USDC on AMM, sell on SDEX.");
    const minUsdc = TRADE_AMOUNT_XLM * ammPrice * 0.995;
    const minXlm = TRADE_AMOUNT_XLM * 0.995;
    try {
      const h1 = await executeAmmSwap(TRADE_AMOUNT_XLM, minUsdc);
      console.log(`[arb] AMM buy tx: ${h1}`);
      // In prod: path-pay USDC→XLM on SDEX. Omitted for brevity.
      console.log(`[arb] SDEX sell step skipped in this example.`);
    } catch (e) {
      console.error("[arb] execution failed:", e);
    }
  }
}

// ── Entry point ───────────────────────────────────────────────────────────────

(async function main() {
  console.log(`QuasarDEX Arb Bot | threshold=${MIN_SPREAD_PCT}% | amount=${TRADE_AMOUNT_XLM} XLM | poll=${POLL_INTERVAL_MS}ms`);
  await checkAndArbitrage();
  setInterval(checkAndArbitrage, POLL_INTERVAL_MS);
})();
