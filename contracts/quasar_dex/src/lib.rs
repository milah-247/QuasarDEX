//! QuasarDEX — constant-product AMM (x*y=k) on Soroban
//! Supports XLM/USDC pools, LP token minting, and on-chain TWAP oracles.
#![no_std]

mod tests;

use soroban_sdk::{
    contract, contractimpl, contracttype, log, symbol_short,
    token::Client as TokenClient,
    Address, Env, Map, Vec,
};

// ── Types ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct Pool {
    pub token_a: Address,
    pub token_b: Address,
    pub reserve_a: i128,
    pub reserve_b: i128,
    pub lp_token: Address,
    pub fee_bps: u32,
    pub k_last: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct TWAPObservation {
    pub timestamp: u64,
    pub price_cumulative_a: i128, // cumulative (reserve_b / reserve_a) * dt, scaled 1e7
    pub price_cumulative_b: i128, // cumulative (reserve_a / reserve_b) * dt, scaled 1e7
}

#[contracttype]
pub enum DataKey {
    Admin,
    Pool(Address, Address),
    Twap(Address, Address),
}

// ── Events ───────────────────────────────────────────────────────────────────

fn emit_pool_created(env: &Env, token_a: &Address, token_b: &Address, lp_token: &Address) {
    env.events()
        .publish((symbol_short!("PoolCrtd"), token_a, token_b), lp_token);
}

fn emit_liquidity_added(env: &Env, provider: &Address, token_a: &Address, token_b: &Address, amount_a: i128, amount_b: i128, lp_minted: i128) {
    env.events()
        .publish((symbol_short!("LiqAdd"), token_a, token_b), (provider, amount_a, amount_b, lp_minted));
}

fn emit_liquidity_removed(env: &Env, provider: &Address, token_a: &Address, token_b: &Address, amount_a: i128, amount_b: i128) {
    env.events()
        .publish((symbol_short!("LiqRem"), token_a, token_b), (provider, amount_a, amount_b));
}

fn emit_swap(env: &Env, trader: &Address, token_in: &Address, token_out: &Address, amount_in: i128, amount_out: i128) {
    env.events()
        .publish((symbol_short!("Swap"), token_in, token_out), (trader, amount_in, amount_out));
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Integer square root (Babylonian method)
fn isqrt(n: i128) -> i128 {
    if n < 0 { panic!("negative sqrt") }
    if n == 0 { return 0; }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Canonical key ordering: smaller address first
fn sorted(a: Address, b: Address) -> (Address, Address) {
    // compare via string encoding to get a stable order
    if a < b { (a, b) } else { (b, a) }
}

fn load_pool(env: &Env, token_a: &Address, token_b: &Address) -> Pool {
    let (ka, kb) = sorted(token_a.clone(), token_b.clone());
    env.storage()
        .persistent()
        .get(&DataKey::Pool(ka, kb))
        .expect("pool not found")
}

fn save_pool(env: &Env, pool: &Pool) {
    let (ka, kb) = sorted(pool.token_a.clone(), pool.token_b.clone());
    env.storage()
        .persistent()
        .set(&DataKey::Pool(ka, kb), pool);
}

fn lp_mint(env: &Env, lp_token: &Address, to: &Address, amount: i128) {
    lp_token::Client::new(env, lp_token).mint(to, &amount);
}

fn lp_burn(env: &Env, lp_token: &Address, from: &Address, amount: i128) {
    lp_token::Client::new(env, lp_token).burn(from, &amount);
}

fn lp_supply(env: &Env, lp_token: &Address) -> i128 {
    lp_token::Client::new(env, lp_token).total_supply()
}

// ── TWAP helpers ─────────────────────────────────────────────────────────────

const TWAP_SCALE: i128 = 10_000_000; // 1e7

fn update_twap(env: &Env, pool: &Pool) {
    let (ka, kb) = sorted(pool.token_a.clone(), pool.token_b.clone());
    let now = env.ledger().timestamp();

    let mut obs: Vec<TWAPObservation> = env
        .storage()
        .persistent()
        .get(&DataKey::Twap(ka.clone(), kb.clone()))
        .unwrap_or_else(|| Vec::new(env));

    let (last_cum_a, last_cum_b, last_ts) = if obs.is_empty() {
        (0_i128, 0_i128, now)
    } else {
        let last = obs.get(obs.len() - 1).unwrap();
        (last.price_cumulative_a, last.price_cumulative_b, last.timestamp)
    };

    let dt = (now - last_ts) as i128;

    // price_a = reserve_b / reserve_a (how much B per A)
    let (cum_a, cum_b) = if pool.reserve_a > 0 && pool.reserve_b > 0 {
        (
            last_cum_a + (pool.reserve_b * TWAP_SCALE / pool.reserve_a) * dt,
            last_cum_b + (pool.reserve_a * TWAP_SCALE / pool.reserve_b) * dt,
        )
    } else {
        (last_cum_a, last_cum_b)
    };

    let observation = TWAPObservation {
        timestamp: now,
        price_cumulative_a: cum_a,
        price_cumulative_b: cum_b,
    };

    // Keep only the last 60 observations to bound storage
    if obs.len() >= 60 {
        // remove oldest by rebuilding (Soroban Vec has no remove)
        let mut new_obs = Vec::new(env);
        for i in 1..obs.len() {
            new_obs.push_back(obs.get(i).unwrap());
        }
        obs = new_obs;
    }
    obs.push_back(observation);
    env.storage()
        .persistent()
        .set(&DataKey::Twap(ka, kb), &obs);
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct QuasarDex;

#[contractimpl]
impl QuasarDex {
    // ── Admin ────────────────────────────────────────────────────────────────

    pub fn initialize(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    // ── create_pool ──────────────────────────────────────────────────────────

    pub fn create_pool(
        env: Env,
        token_a: Address,
        token_b: Address,
        fee_bps: u32,
    ) -> Address {
        assert!(fee_bps <= 100, "fee_bps > 1%");
        let (ka, kb) = sorted(token_a.clone(), token_b.clone());
        assert!(
            !env.storage().persistent().has(&DataKey::Pool(ka.clone(), kb.clone())),
            "pool exists"
        );

        // Deploy LP token contract
        let lp_wasm = env.deployer().upload_contract_wasm(lp_token::WASM);
        let lp_token = env
            .deployer()
            .with_current_contract(env.crypto().sha256(&env.ledger().sequence().to_xdr(&env)))
            .deploy_v2(lp_wasm, ());
        lp_token::Client::new(&env, &lp_token).initialize(&env.current_contract_address());

        let pool = Pool {
            token_a: ka.clone(),
            token_b: kb.clone(),
            reserve_a: 0,
            reserve_b: 0,
            lp_token: lp_token.clone(),
            fee_bps,
            k_last: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Pool(ka.clone(), kb.clone()), &pool);

        emit_pool_created(&env, &ka, &kb, &lp_token);
        lp_token
    }

    // ── add_liquidity ────────────────────────────────────────────────────────

    pub fn add_liquidity(
        env: Env,
        provider: Address,
        token_a: Address,
        token_b: Address,
        amount_a: i128,
        amount_b: i128,
        min_lp: i128,
    ) -> i128 {
        provider.require_auth();
        let mut pool = load_pool(&env, &token_a, &token_b);

        // Determine actual amounts to deposit (maintain ratio when pool non-empty)
        let (actual_a, actual_b) = if pool.reserve_a == 0 && pool.reserve_b == 0 {
            (amount_a, amount_b)
        } else {
            // optimal b given a
            let optimal_b = amount_a * pool.reserve_b / pool.reserve_a;
            if optimal_b <= amount_b {
                (amount_a, optimal_b)
            } else {
                let optimal_a = amount_b * pool.reserve_a / pool.reserve_b;
                (optimal_a, amount_b)
            }
        };

        // Transfer tokens from provider to contract
        TokenClient::new(&env, &pool.token_a).transfer(&provider, &env.current_contract_address(), &actual_a);
        TokenClient::new(&env, &pool.token_b).transfer(&provider, &env.current_contract_address(), &actual_b);

        // Mint LP tokens
        let supply = lp_supply(&env, &pool.lp_token);
        let lp_minted = if supply == 0 {
            // geometric mean of deposits, minus MINIMUM_LIQUIDITY (1000)
            let initial = isqrt(actual_a * actual_b);
            assert!(initial > 1000, "initial liquidity too small");
            initial - 1000
        } else {
            // proportional to smaller share
            let share_a = actual_a * supply / pool.reserve_a;
            let share_b = actual_b * supply / pool.reserve_b;
            share_a.min(share_b)
        };

        assert!(lp_minted >= min_lp, "slippage: lp < min_lp");

        pool.reserve_a += actual_a;
        pool.reserve_b += actual_b;
        pool.k_last = pool.reserve_a * pool.reserve_b;
        save_pool(&env, &pool);
        update_twap(&env, &pool);

        lp_mint(&env, &pool.lp_token, &provider, lp_minted);
        emit_liquidity_added(&env, &provider, &pool.token_a, &pool.token_b, actual_a, actual_b, lp_minted);
        lp_minted
    }

    // ── remove_liquidity ─────────────────────────────────────────────────────

    pub fn remove_liquidity(
        env: Env,
        provider: Address,
        token_a: Address,
        token_b: Address,
        lp_amount: i128,
        min_a: i128,
        min_b: i128,
    ) {
        provider.require_auth();
        let mut pool = load_pool(&env, &token_a, &token_b);
        let supply = lp_supply(&env, &pool.lp_token);
        assert!(supply > 0, "no liquidity");

        let out_a = lp_amount * pool.reserve_a / supply;
        let out_b = lp_amount * pool.reserve_b / supply;
        assert!(out_a >= min_a && out_b >= min_b, "slippage");

        lp_burn(&env, &pool.lp_token, &provider, lp_amount);
        TokenClient::new(&env, &pool.token_a).transfer(&env.current_contract_address(), &provider, &out_a);
        TokenClient::new(&env, &pool.token_b).transfer(&env.current_contract_address(), &provider, &out_b);

        pool.reserve_a -= out_a;
        pool.reserve_b -= out_b;
        pool.k_last = pool.reserve_a * pool.reserve_b;
        save_pool(&env, &pool);
        update_twap(&env, &pool);

        emit_liquidity_removed(&env, &provider, &pool.token_a, &pool.token_b, out_a, out_b);
    }

    // ── swap ─────────────────────────────────────────────────────────────────

    pub fn swap(
        env: Env,
        trader: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_out: i128,
    ) -> i128 {
        trader.require_auth();
        let mut pool = load_pool(&env, &token_in, &token_out);
        assert!(pool.reserve_a > 0 && pool.reserve_b > 0, "no liquidity");

        // Determine reserve_in / reserve_out by token direction
        let (reserve_in, reserve_out, is_a_in) = if token_in == pool.token_a {
            (pool.reserve_a, pool.reserve_b, true)
        } else {
            (pool.reserve_b, pool.reserve_a, false)
        };

        // amount_in after fee
        let fee_numerator = 10_000_i128 - pool.fee_bps as i128;
        let amount_in_with_fee = amount_in * fee_numerator;
        let amount_out = amount_in_with_fee * reserve_out
            / (reserve_in * 10_000 + amount_in_with_fee);

        assert!(amount_out >= min_out, "slippage: out < min_out");
        assert!(amount_out < reserve_out, "insufficient liquidity");

        // Transfer
        TokenClient::new(&env, &token_in).transfer(&trader, &env.current_contract_address(), &amount_in);
        TokenClient::new(&env, &token_out).transfer(&env.current_contract_address(), &trader, &amount_out);

        // Update reserves
        if is_a_in {
            pool.reserve_a += amount_in;
            pool.reserve_b -= amount_out;
        } else {
            pool.reserve_b += amount_in;
            pool.reserve_a -= amount_out;
        }
        pool.k_last = pool.reserve_a * pool.reserve_b;
        save_pool(&env, &pool);
        update_twap(&env, &pool);

        emit_swap(&env, &trader, &token_in, &token_out, amount_in, amount_out);
        amount_out
    }

    // ── View functions ────────────────────────────────────────────────────────

    /// Spot price of token_a in terms of token_b, scaled 1e7
    pub fn get_price(env: Env, token_a: Address, token_b: Address) -> i128 {
        let pool = load_pool(&env, &token_a, &token_b);
        assert!(pool.reserve_a > 0, "no reserves");
        // If queried a==pool.token_a: price = reserve_b / reserve_a
        if token_a == pool.token_a {
            pool.reserve_b * TWAP_SCALE / pool.reserve_a
        } else {
            pool.reserve_a * TWAP_SCALE / pool.reserve_b
        }
    }

    /// TWAP over `period` seconds, scaled 1e7
    pub fn twap(env: Env, token_a: Address, token_b: Address, period: u64) -> i128 {
        let (ka, kb) = sorted(token_a.clone(), token_b.clone());
        let obs: Vec<TWAPObservation> = env
            .storage()
            .persistent()
            .get(&DataKey::Twap(ka, kb))
            .expect("no observations");

        let now = env.ledger().timestamp();
        let target = now.saturating_sub(period);

        // Find oldest observation within the period
        let mut oldest_idx = obs.len() - 1;
        for i in 0..obs.len() {
            if obs.get(i).unwrap().timestamp >= target {
                oldest_idx = i;
                break;
            }
        }

        let oldest = obs.get(oldest_idx).unwrap();
        let newest = obs.get(obs.len() - 1).unwrap();
        let dt = (newest.timestamp - oldest.timestamp) as i128;
        if dt == 0 {
            return Self::get_price(env, token_a.clone(), token_b.clone());
        }

        let (cum_start, cum_end) = if token_a == ka {
            (oldest.price_cumulative_a, newest.price_cumulative_a)
        } else {
            (oldest.price_cumulative_b, newest.price_cumulative_b)
        };

        (cum_end - cum_start) / dt
    }

    /// Pure quote: how much out given amount_in and reserves
    pub fn quote(env: Env, amount_in: i128, reserve_in: i128, reserve_out: i128) -> i128 {
        assert!(reserve_in > 0 && reserve_out > 0, "bad reserves");
        amount_in * reserve_out / reserve_in
    }

    /// Get pool info
    pub fn get_pool(env: Env, token_a: Address, token_b: Address) -> Pool {
        load_pool(&env, &token_a, &token_b)
    }
}
