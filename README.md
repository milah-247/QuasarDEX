# QuasarDEX ✦

A high-performance constant-product AMM protocol on Stellar/Soroban with XLM/USDC pools, LP token minting, on-chain TWAP oracles, and arbitrage integrations.

---

## Table of Contents

1. [Problem It Solves](#problem-it-solves)
2. [Architecture](#architecture)
3. [Project Structure](#project-structure)
4. [Getting Started](#getting-started)
5. [Contributing](#contributing)

---

## Problem It Solves

Stellar's native DEX (SDEX) uses a central limit order book — efficient for large trades but poor for automated liquidity provision and on-chain price feeds. QuasarDEX fills three gaps:

| Gap | Solution |
|-----|----------|
| No AMM liquidity pools on Soroban | Constant-product pools (`x*y=k`) with permissionless creation |
| No trustless on-chain price oracle | TWAP observations stored per-pool, queryable over any time window |
| Price discrepancies between AMM and SDEX go uncaptured | Arbitrage bot monitors spread and executes when > 0.5% |

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     Frontend (Next.js 14)                │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │  Swap UI     │  │ LP Dashboard │  │  TWAP Chart   │  │
│  │ price impact │  │ share % + IL │  │  (Recharts)   │  │
│  └──────┬───────┘  └──────┬───────┘  └──────┬────────┘  │
│         └─────────────────┴─────────────────┘           │
│                     lib/contract.ts                      │
│              (Stellar SDK + simulateTransaction)         │
└───────────────────────────┬─────────────────────────────┘
                            │ RPC
┌───────────────────────────▼─────────────────────────────┐
│               Soroban Smart Contracts                    │
│                                                          │
│  ┌─────────────────────────────────────────────────┐    │
│  │              quasar_dex (main)                   │    │
│  │                                                  │    │
│  │  create_pool ──► deploys lp_token per pool       │    │
│  │  add_liquidity ─► mint LP = √(a·b) − 1000 (init)│    │
│  │  remove_liquidity ◄─ burn LP, return reserves    │    │
│  │  swap ──► x*y=k with fee_bps, updates TWAP       │    │
│  │  get_price ──► reserve_b/reserve_a × 1e7         │    │
│  │  twap ──► cumulative price ÷ elapsed seconds     │    │
│  └──────────────────┬──────────────────────────────┘    │
│                     │ deploys / calls                    │
│  ┌──────────────────▼──────────────────────────────┐    │
│  │              lp_token (per pool)                 │    │
│  │         mint · burn · balance · total_supply     │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│               Arbitrage Bot (TypeScript)                 │
│                                                          │
│  poll every 5s                                           │
│    ├─ get_price (AMM via simulateTransaction)            │
│    └─ order book best-ask (Stellar Horizon)              │
│                                                          │
│  spread > 0.5% ?                                         │
│    ├─ AMM overpriced → buy on SDEX, sell on AMM          │
│    └─ SDEX overpriced → buy on AMM, sell on SDEX         │
└─────────────────────────────────────────────────────────┘
```

### Key Design Decisions

- **Canonical pool key** — token addresses sorted before storage so `(XLM, USDC)` and `(USDC, XLM)` resolve to the same pool.
- **MINIMUM_LIQUIDITY (1000)** — burned on first deposit to prevent price manipulation with dust amounts.
- **TWAP ring buffer** — stores up to 60 observations per pool; cumulative prices updated on every swap and liquidity event.
- **Fee applied pre-swap** — `amount_in_with_fee = amount_in × (10000 − fee_bps)`, keeping k non-decreasing after every trade.

---

## Project Structure

```
QuasarDEX/
├── Cargo.toml                        # Rust workspace
├── .env.example                      # All environment variables
├── contracts/
│   ├── lp_token/src/lib.rs           # Fungible LP token (mint/burn)
│   └── quasar_dex/src/
│       ├── lib.rs                    # AMM contract
│       └── tests.rs                  # Testutils suite (k-invariant assertions)
├── frontend/
│   ├── app/
│   │   ├── page.tsx                  # Swap UI
│   │   └── pool/page.tsx             # LP dashboard
│   ├── components/TWAPChart.tsx      # Price chart
│   └── lib/contract.ts               # SDK helpers + calcPriceImpact
└── bot/arb.ts                        # Arbitrage bot
```

---

## Getting Started

### Prerequisites

- Rust + `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [Stellar CLI](https://developers.stellar.org/docs/tools/stellar-cli): `cargo install stellar-cli`
- Node.js 18+

### Build & Test Contracts

```bash
# Build
stellar contract build

# Run tests
cargo test -p quasar_dex
```

### Deploy (Testnet)

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/lp_token.wasm \
  --network testnet --source <YOUR_KEY>

stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/quasar_dex.wasm \
  --network testnet --source <YOUR_KEY>
```

### Run Frontend

```bash
cd frontend
cp ../.env.example .env.local   # fill in contract IDs
npm install
npm run dev
```

### Run Arbitrage Bot

```bash
cd bot
cp ../.env.example .env         # fill in TRADER_SECRET and contract IDs
npm install
npm start
```

---

## Contributing

Contributions are welcome. Please follow the process below.

### Workflow

1. Fork the repository and create a feature branch:
   ```bash
   git checkout -b feat/your-feature
   ```
2. Make your changes and ensure tests pass:
   ```bash
   cargo test -p quasar_dex
   ```
3. Commit using [Conventional Commits](https://www.conventionalcommits.org/):
   ```
   feat: add multi-hop swap routing
   fix: correct TWAP interpolation at period boundary
   ```
4. Open a pull request against `main` with a clear description of the change and what was tested.

### Areas to Contribute

- **Multi-hop routing** — path-finding across multiple pools
- **Fee distribution** — protocol fee split to treasury
- **Flash loans** — single-transaction borrow/repay
- **Price impact UI** — improve warnings for large trades
- **More test coverage** — edge cases: zero reserves, max i128, fee = 0

### Code Standards

- Soroban contracts: no `std`, minimal storage writes, all public fns require auth where applicable.
- TypeScript: strict mode, no `any` without comment explaining why.
- Keep PRs focused — one concern per PR.

### Reporting Issues

Open a GitHub Issue with:
- Steps to reproduce
- Expected vs actual behaviour
- Relevant contract address / transaction hash if on-chain
