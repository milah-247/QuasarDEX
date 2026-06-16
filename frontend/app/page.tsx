"use client";
import { useState, useEffect } from "react";
import { calcPriceImpact, simulateContractCall } from "@/lib/contract";

const TOKENS = ["XLM", "USDC"];
const XLM_ADDR = process.env.NEXT_PUBLIC_XLM_ADDRESS ?? "";
const USDC_ADDR = process.env.NEXT_PUBLIC_USDC_ADDRESS ?? "";
const TOKEN_ADDR: Record<string, string> = { XLM: XLM_ADDR, USDC: USDC_ADDR };

export default function SwapPage() {
  const [tokenIn, setTokenIn] = useState("XLM");
  const [tokenOut, setTokenOut] = useState("USDC");
  const [amountIn, setAmountIn] = useState("");
  const [amountOut, setAmountOut] = useState<number | null>(null);
  const [priceImpact, setPriceImpact] = useState<number | null>(null);
  const [pool, setPool] = useState<{ reserve_a: bigint; reserve_b: bigint; fee_bps: number } | null>(null);
  const [loading, setLoading] = useState(false);
  const [status, setStatus] = useState("");

  // Fetch pool reserves on token change
  useEffect(() => {
    if (!TOKEN_ADDR[tokenIn] || !TOKEN_ADDR[tokenOut]) return;
    simulateContractCall("GXXXXXXX", "get_pool", [TOKEN_ADDR[tokenIn], TOKEN_ADDR[tokenOut]])
      .then((p: any) => setPool({ reserve_a: BigInt(p.reserve_a), reserve_b: BigInt(p.reserve_b), fee_bps: p.fee_bps }))
      .catch(() => setPool(null));
  }, [tokenIn, tokenOut]);

  useEffect(() => {
    if (!pool || !amountIn || isNaN(Number(amountIn))) {
      setAmountOut(null);
      setPriceImpact(null);
      return;
    }
    const amt = BigInt(Math.floor(Number(amountIn) * 1e7));
    const [resIn, resOut] = tokenIn === "XLM"
      ? [pool.reserve_a, pool.reserve_b]
      : [pool.reserve_b, pool.reserve_a];

    const feeNum = 10_000n - BigInt(pool.fee_bps);
    const amtWithFee = amt * feeNum;
    const out = Number(amtWithFee * resOut / (resIn * 10_000n + amtWithFee)) / 1e7;
    setAmountOut(out);
    setPriceImpact(calcPriceImpact(amt, resIn, resOut, pool.fee_bps));
  }, [amountIn, pool, tokenIn]);

  async function handleSwap() {
    setLoading(true);
    setStatus("Submitting swap...");
    try {
      // In production: sign and submit transaction with wallet
      await new Promise((r) => setTimeout(r, 1000));
      setStatus("Swap submitted! (connect wallet to execute)");
    } finally {
      setLoading(false);
    }
  }

  const impactColor = priceImpact === null ? "" : priceImpact > 5 ? "text-red-400" : priceImpact > 1 ? "text-yellow-400" : "text-green-400";

  return (
    <div className="bg-gray-900 rounded-2xl p-6 shadow-xl max-w-md mx-auto">
      <h1 className="text-2xl font-bold mb-6">Swap</h1>

      <div className="space-y-3">
        {/* Input */}
        <div className="bg-gray-800 rounded-xl p-4 flex gap-3 items-center">
          <input
            type="number"
            placeholder="0.0"
            value={amountIn}
            onChange={(e) => setAmountIn(e.target.value)}
            className="flex-1 bg-transparent text-2xl outline-none w-0 min-w-0"
          />
          <select
            value={tokenIn}
            onChange={(e) => { setTokenIn(e.target.value); if (e.target.value === tokenOut) setTokenOut(TOKENS.find(t => t !== e.target.value)!); }}
            className="bg-gray-700 rounded-lg px-3 py-1 text-sm"
          >
            {TOKENS.map(t => <option key={t}>{t}</option>)}
          </select>
        </div>

        {/* Flip */}
        <button
          onClick={() => { setTokenIn(tokenOut); setTokenOut(tokenIn); setAmountIn(""); }}
          className="mx-auto block text-gray-400 hover:text-indigo-400 text-lg"
        >⇅</button>

        {/* Output */}
        <div className="bg-gray-800 rounded-xl p-4 flex gap-3 items-center">
          <span className="flex-1 text-2xl text-gray-400">
            {amountOut !== null ? amountOut.toFixed(6) : "0.0"}
          </span>
          <span className="bg-gray-700 rounded-lg px-3 py-1 text-sm">{tokenOut}</span>
        </div>
      </div>

      {/* Price impact */}
      {priceImpact !== null && (
        <div className={`mt-3 text-sm flex justify-between ${impactColor}`}>
          <span>Price Impact</span>
          <span>{priceImpact.toFixed(2)}%</span>
        </div>
      )}

      {pool && amountIn && (
        <div className="mt-1 text-xs text-gray-500 flex justify-between">
          <span>Fee</span>
          <span>{(pool.fee_bps / 100).toFixed(2)}%</span>
        </div>
      )}

      <button
        onClick={handleSwap}
        disabled={loading || !amountIn || amountOut === null}
        className="mt-5 w-full py-3 bg-indigo-600 hover:bg-indigo-500 disabled:bg-gray-700 disabled:text-gray-500 rounded-xl font-semibold transition"
      >
        {loading ? "Swapping…" : "Swap"}
      </button>

      {status && <p className="mt-3 text-sm text-center text-gray-400">{status}</p>}
    </div>
  );
}
