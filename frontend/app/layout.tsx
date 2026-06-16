import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = { title: "QuasarDEX", description: "AMM on Stellar" };

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className="min-h-screen bg-gray-950 text-gray-100 font-sans">
        <nav className="flex items-center gap-6 px-6 py-4 border-b border-gray-800">
          <span className="text-xl font-bold text-indigo-400">QuasarDEX ✦</span>
          <a href="/" className="hover:text-indigo-300">Swap</a>
          <a href="/pool" className="hover:text-indigo-300">Pool</a>
        </nav>
        <main className="max-w-2xl mx-auto py-10 px-4">{children}</main>
      </body>
    </html>
  );
}
