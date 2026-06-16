"use client";
import { useEffect, useState } from "react";
import {
  LineChart, Line, XAxis, YAxis, Tooltip, CartesianGrid, ResponsiveContainer,
} from "recharts";
import { simulateContractCall } from "@/lib/contract";

const XLM_ADDR = process.env.NEXT_PUBLIC_XLM_ADDRESS ?? "";
const USDC_ADDR = process.env.NEXT_PUBLIC_USDC_ADDRESS ?? "";
const SCALE = 1e7;
const PERIODS = [300, 900, 3600, 86400]; // 5m, 15m, 1h, 24h
const PERIOD_LABELS: Record<number, string> = { 300: "5m", 900: "15m", 3600: "1h", 86400: "24h" };

interface DataPoint { time: string; twap: number; spot: number }

export default function TWAPChart() {
  const [data, setData] = useState<DataPoint[]>([]);
  const [period, setPeriod] = useState(3600);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!XLM_ADDR || !USDC_ADDR) {
      // Generate mock data when no contract is configured
      const now = Date.now();
      const mock: DataPoint[] = Array.from({ length: 20 }, (_, i) => ({
        time: new Date(now - (20 - i) * period * 1000 / 20).toLocaleTimeString(),
        twap: 0.09 + Math.sin(i / 4) * 0.005 + Math.random() * 0.002,
        spot: 0.09 + Math.sin(i / 4) * 0.005 + (Math.random() - 0.5) * 0.004,
      }));
      setData(mock);
      return;
    }

    setLoading(true);
    Promise.all([
      simulateContractCall("GXXXXXXX", "twap", [XLM_ADDR, USDC_ADDR, period]),
      simulateContractCall("GXXXXXXX", "get_price", [XLM_ADDR, USDC_ADDR]),
    ])
      .then(([twapRaw, spotRaw]) => {
        const now = new Date().toLocaleTimeString();
        setData((prev) => [
          ...prev.slice(-49),
          { time: now, twap: Number(twapRaw) / SCALE, spot: Number(spotRaw) / SCALE },
        ]);
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [period]);

  return (
    <div className="bg-gray-900 rounded-2xl p-5">
      <div className="flex items-center justify-between mb-4">
        <h2 className="font-semibold text-indigo-400">TWAP Price — XLM/USDC</h2>
        <div className="flex gap-1">
          {PERIODS.map((p) => (
            <button
              key={p}
              onClick={() => setPeriod(p)}
              className={`px-3 py-1 rounded text-xs ${period === p ? "bg-indigo-600" : "bg-gray-800 hover:bg-gray-700"}`}
            >
              {PERIOD_LABELS[p]}
            </button>
          ))}
        </div>
      </div>

      {loading && <p className="text-xs text-gray-500 mb-2">Loading…</p>}

      <ResponsiveContainer width="100%" height={220}>
        <LineChart data={data}>
          <CartesianGrid strokeDasharray="3 3" stroke="#1f2937" />
          <XAxis dataKey="time" tick={{ fontSize: 10, fill: "#6b7280" }} />
          <YAxis tick={{ fontSize: 10, fill: "#6b7280" }} domain={["auto", "auto"]} />
          <Tooltip
            contentStyle={{ background: "#111827", border: "1px solid #374151", borderRadius: 8 }}
            formatter={(v: number) => v.toFixed(6)}
          />
          <Line type="monotone" dataKey="twap" stroke="#6366f1" dot={false} name="TWAP" strokeWidth={2} />
          <Line type="monotone" dataKey="spot" stroke="#10b981" dot={false} name="Spot" strokeWidth={1} strokeDasharray="4 2" />
        </LineChart>
      </ResponsiveContainer>

      <div className="flex gap-4 mt-2 text-xs text-gray-500">
        <span><span className="inline-block w-4 h-0.5 bg-indigo-500 mr-1 align-middle" />TWAP</span>
        <span><span className="inline-block w-4 h-0.5 bg-emerald-500 mr-1 align-middle" />Spot</span>
      </div>
    </div>
  );
}
