#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    Address, Env,
};

// Re-export contract under test and the lp_token contract
use crate::{Pool, QuasarDex, QuasarDexClient};

mod lp {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/release/lp_token.wasm"
    );
}

// ── Test helpers ───────────────────────────────────────────────────────────────

struct Setup {
    env: Env,
    contract_id: Address,
    client: QuasarDexClient<'static>,
    token_a: Address, // XLM mock
    token_b: Address, // USDC mock
    alice: Address,
    bob: Address,
}

fn deploy_mock_token(env: &Env, admin: &Address) -> Address {
    // Use Soroban's built-in mock token
    let addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    addr
}

fn mint_token(env: &Env, token: &Address, to: &Address, amount: i128) {
    let client = soroban_sdk::token::StellarAssetClient::new(env, token);
    client.mint(to, &amount);
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        let token_a = deploy_mock_token(&env, &admin);
        let token_b = deploy_mock_token(&env, &admin);

        // Mint tokens
        mint_token(&env, &token_a, &alice, 1_000_000 * 1_000_0000); // 1M XLM
        mint_token(&env, &token_b, &alice, 1_000_000 * 1_000_0000); // 1M USDC
        mint_token(&env, &token_a, &bob, 100_000 * 1_000_0000);
        mint_token(&env, &token_b, &bob, 100_000 * 1_000_0000);

        let contract_id = env.register_contract(None, QuasarDex);
        let client = QuasarDexClient::new(&env, &contract_id);
        client.initialize(&admin);

        Setup { env, contract_id, client, token_a, token_b, alice, bob }
    }

    fn create_pool(&self, fee_bps: u32) -> Address {
        self.client.create_pool(&self.token_a, &self.token_b, &fee_bps)
    }

    fn pool(&self) -> Pool {
        self.client.get_pool(&self.token_a, &self.token_b)
    }

    fn k(&self) -> i128 {
        let p = self.pool();
        p.reserve_a * p.reserve_b
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[test]
fn test_create_pool() {
    let s = Setup::new();
    let lp = s.create_pool(30); // 0.3%
    let pool = s.pool();
    assert_eq!(pool.fee_bps, 30);
    assert_eq!(pool.reserve_a, 0);
    assert_eq!(pool.reserve_b, 0);
    assert_eq!(pool.lp_token, lp);
}

#[test]
#[should_panic(expected = "pool exists")]
fn test_create_pool_duplicate_panics() {
    let s = Setup::new();
    s.create_pool(30);
    s.create_pool(30); // must panic
}

#[test]
fn test_add_liquidity_initial() {
    let s = Setup::new();
    s.create_pool(30);

    let amt_a = 10_000 * 1_000_0000_i128; // 10k XLM
    let amt_b = 1_000 * 1_000_0000_i128;  // 1k USDC

    let lp = s.client.add_liquidity(&s.alice, &s.token_a, &s.token_b, &amt_a, &amt_b, &0);
    let pool = s.pool();

    assert_eq!(pool.reserve_a, amt_a);
    assert_eq!(pool.reserve_b, amt_b);
    assert!(lp > 0, "should have minted LP tokens");

    // k invariant holds after add
    assert_eq!(pool.k_last, amt_a * amt_b);
}

#[test]
fn test_k_invariant_after_swap() {
    let s = Setup::new();
    s.create_pool(30);

    let amt_a = 10_000 * 1_000_0000_i128;
    let amt_b = 1_000 * 1_000_0000_i128;
    s.client.add_liquidity(&s.alice, &s.token_a, &s.token_b, &amt_a, &amt_b, &0);

    let k_before = s.k();

    // Bob swaps 100 XLM → USDC
    let swap_in = 100 * 1_000_0000_i128;
    let out = s.client.swap(&s.bob, &s.token_a, &s.token_b, &swap_in, &0);
    assert!(out > 0);

    let k_after = s.k();
    // k should only increase (fee captures value)
    assert!(k_after >= k_before, "k decreased after swap: before={k_before}, after={k_after}");
}

#[test]
fn test_swap_respects_fee() {
    let s = Setup::new();
    s.create_pool(30); // 0.3% fee

    let amt_a = 10_000 * 1_000_0000_i128;
    let amt_b = 1_000 * 1_000_0000_i128;
    s.client.add_liquidity(&s.alice, &s.token_a, &s.token_b, &amt_a, &amt_b, &0);

    // Swap 1000 XLM
    let swap_in = 1_000 * 1_000_0000_i128;
    let out_with_fee = s.client.swap(&s.bob, &s.token_a, &s.token_b, &swap_in, &0);

    // Manual constant product: out without fee = swap_in * reserve_b / (reserve_a + swap_in)
    let out_no_fee = swap_in * amt_b / (amt_a + swap_in);
    assert!(out_with_fee < out_no_fee, "fee must reduce output");
}

#[test]
fn test_swap_slippage_guard() {
    let s = Setup::new();
    s.create_pool(30);
    s.client.add_liquidity(&s.alice, &s.token_a, &s.token_b, &(10_000 * 1_000_0000_i128), &(1_000 * 1_000_0000_i128), &0);

    // min_out set impossibly high → must panic
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        s.client.swap(&s.bob, &s.token_a, &s.token_b, &(100 * 1_000_0000_i128), &(99_999 * 1_000_0000_i128));
    }));
    assert!(result.is_err(), "should have panicked on slippage");
}

#[test]
fn test_remove_liquidity() {
    let s = Setup::new();
    s.create_pool(30);
    let amt_a = 10_000 * 1_000_0000_i128;
    let amt_b = 1_000 * 1_000_0000_i128;
    let lp_amount = s.client.add_liquidity(&s.alice, &s.token_a, &s.token_b, &amt_a, &amt_b, &0);

    let supply = {
        let p = s.pool();
        lp::Client::new(&s.env, &p.lp_token).total_supply()
    };

    // Remove half
    let half_lp = lp_amount / 2;
    s.client.remove_liquidity(&s.alice, &s.token_a, &s.token_b, &half_lp, &0, &0);
    let pool = s.pool();

    // Reserves should be approximately halved
    assert!(pool.reserve_a < amt_a, "reserve_a should have decreased");
    assert!(pool.reserve_b < amt_b, "reserve_b should have decreased");

    // k should still hold
    assert_eq!(pool.k_last, pool.reserve_a * pool.reserve_b);
}

#[test]
fn test_get_price() {
    let s = Setup::new();
    s.create_pool(30);
    let amt_a = 10_000 * 1_000_0000_i128; // 10k XLM
    let amt_b = 1_000 * 1_000_0000_i128;  // 1k USDC → price = 0.1 USDC/XLM
    s.client.add_liquidity(&s.alice, &s.token_a, &s.token_b, &amt_a, &amt_b, &0);

    let price = s.client.get_price(&s.token_a, &s.token_b);
    // Expected: reserve_b / reserve_a * 1e7 = 1_000/10_000 * 1e7 = 1_000_000
    assert_eq!(price, 1_000_000_i128, "price should be 0.1 scaled 1e7");
}

#[test]
fn test_quote() {
    let s = Setup::new();
    s.create_pool(30);
    // quote is a pure function: amount_in * reserve_out / reserve_in
    let q = s.client.quote(&1_000_0000_i128, &10_000_0000_i128, &1_000_0000_i128);
    assert_eq!(q, 1_000_0000_i128 * 1_000_0000 / 10_000_0000);
}

#[test]
fn test_twap_after_swaps() {
    let s = Setup::new();
    s.create_pool(30);
    s.client.add_liquidity(&s.alice, &s.token_a, &s.token_b, &(10_000 * 1_000_0000_i128), &(1_000 * 1_000_0000_i128), &0);

    // Advance ledger time and perform swaps
    for i in 1..=5u64 {
        s.env.ledger().with_mut(|l| { l.timestamp = i * 60; });
        s.client.swap(&s.bob, &s.token_a, &s.token_b, &(10 * 1_000_0000_i128), &0);
    }

    let twap_price = s.client.twap(&s.token_a, &s.token_b, &300u64);
    let spot_price = s.client.get_price(&s.token_a, &s.token_b);

    // TWAP and spot should both be positive and in the same ballpark
    assert!(twap_price > 0, "twap must be positive");
    assert!(spot_price > 0, "spot must be positive");
    // After buying XLM, USDC per XLM decreases → twap roughly near initial 1_000_000
    assert!(twap_price > 500_000 && twap_price < 2_000_000, "twap out of range: {twap_price}");
}

#[test]
fn test_k_invariant_multiple_swaps() {
    let s = Setup::new();
    s.create_pool(30);
    s.client.add_liquidity(&s.alice, &s.token_a, &s.token_b, &(10_000 * 1_000_0000_i128), &(1_000 * 1_000_0000_i128), &0);

    let mut k_prev = s.k();
    for _ in 0..10 {
        s.client.swap(&s.bob, &s.token_a, &s.token_b, &(50 * 1_000_0000_i128), &0);
        let k_now = s.k();
        assert!(k_now >= k_prev, "k must be non-decreasing: prev={k_prev} now={k_now}");
        k_prev = k_now;
    }
}
