"use client";
import { useState, useEffect } from "react";
import { simulateContractCall } from "@/lib/contract";
import TWAPChart from "@/components/TWAPChart";

const XLM_ADDR = process.env.NEXT_PUBLIC_XLM_ADDRESS ?? "";
const USDC_ADDR = process.env.NEXT_PUBLIC_USDC_ADDRESS ?? "";

interface PoolState {
  reserve_a: number;
  reserve_b: number;
  lp_token: string;
  fee_bps: number;
}

/** Impermanent loss given price ratio change: IL = 2√r/(1+r) - 1 */
function calcIL(initialPrice: number, currentPrice: number): number {
  const r = currentPrice / initialPrice;
  return (2 * Math.sqrt(r)) / (1 + r) - 1;
}

export default function PoolPage() {
  const [pool, setPool] = useState<PoolState | null>(null);
  const [lpBalance, setLpBalance] = useState<number>(0);
  const [initialPrice, setInitialPrice] = useState("1.0");

  useEffect(() => {
    simulateContractCall("GXXXXXXX", "get_pool", [XLM_ADDR, USDC_ADDR])
      .then((p: any) => setPool(p))
      .catch(() => {});
  }, []);

  const totalLpSupply = 1_000_000; // mock; fetch from lp_token.total_supply in prod
  const sharePercent = totalLpSupply > 0 ? (lpBalance / totalLpSupply) * 100 : 0;

  const currentPrice = pool ? pool.reserve_b / pool.reserve_a : 1;
  const il = calcIL(parseFloat(initialPrice) || 1, currentPrice);
  const ilPct = (il * 100).toFixed(2);

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Liquidity Positions</h1>

      {/* Pool stats */}
      <div className="bg-gray-900 rounded-2xl p-5 space-y-3">
        <h2 className="font-semibold text-indigo-400">XLM / USDC Pool</h2>
        {pool ? (
          <div className="grid grid-cols-2 gap-3 text-sm">
            <Stat label="Reserve XLM" value={pool.reserve_a.toLocaleString()} />
            <Stat label="Reserve USDC" value={pool.reserve_b.toLocaleString()} />
            <Stat label="Spot Price" value={`${currentPrice.toFixed(4)} USDC/XLM`} />
            <Stat label="Fee" value={`${(pool.fee_bps / 100).toFixed(2)}%`} />
          </div>
        ) : (
          <p className="text-gray-500 text-sm">Connect wallet to load pool data.</p>
        )}
      </div>

      {/* My position */}
      <div className="bg-gray-900 rounded-2xl p-5 space-y-3">
        <h2 className="font-semibold text-indigo-400">My Position</h2>
        <label className="text-sm text-gray-400 block">
          LP Token Balance
          <input
            type="number"
            value={lpBalance}
            onChange={(e) => setLpBalance(Number(e.target.value))}
            className="ml-3 bg-gray-800 rounded px-2 py-1 w-32 text-white"
          />
        </label>
        <Stat label="Pool Share" value={`${sharePercent.toFixed(4)}%`} />

        <label className="text-sm text-gray-400 block mt-2">
          Entry Price (USDC/XLM)
          <input
            type="number"
            value={initialPrice}
            onChange={(e) => setInitialPrice(e.target.value)}
            className="ml-3 bg-gray-800 rounded px-2 py-1 w-32 text-white"
          />
        </label>
        <div className={`flex justify-between text-sm ${il < 0 ? "text-red-400" : "text-green-400"}`}>
          <span>Impermanent Loss</span>
          <span>{ilPct}%</span>
        </div>
        <p className="text-xs text-gray-500">
          IL formula: 2√r/(1+r) − 1, where r = current/entry price
        </p>
      </div>

      <TWAPChart />
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-gray-800 rounded-xl p-3">
      <p className="text-xs text-gray-500 mb-1">{label}</p>
      <p className="font-semibold">{value}</p>
    </div>
  );
}
