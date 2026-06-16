//! Minimal fungible LP token contract for QuasarDEX pools.
#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String,
};

#[contracttype]
pub enum DataKey {
    Balance(Address),
    TotalSupply,
    Minter,
}

#[contract]
pub struct LpToken;

#[contractimpl]
impl LpToken {
    pub fn initialize(env: Env, minter: Address) {
        env.storage().instance().set(&DataKey::Minter, &minter);
        env.storage().instance().set(&DataKey::TotalSupply, &0_i128);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let minter: Address = env.storage().instance().get(&DataKey::Minter).unwrap();
        minter.require_auth();
        let bal = Self::balance(env.clone(), to.clone());
        env.storage().instance().set(&DataKey::Balance(to), &(bal + amount));
        let ts: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap();
        env.storage().instance().set(&DataKey::TotalSupply, &(ts + amount));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        from.require_auth();
        let bal = Self::balance(env.clone(), from.clone());
        assert!(bal >= amount, "insufficient balance");
        env.storage().instance().set(&DataKey::Balance(from), &(bal - amount));
        let ts: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap();
        env.storage().instance().set(&DataKey::TotalSupply, &(ts - amount));
    }

    pub fn balance(env: Env, addr: Address) -> i128 {
        env.storage().instance().get(&DataKey::Balance(addr)).unwrap_or(0)
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::TotalSupply).unwrap_or(0)
    }

    pub fn name(_env: Env) -> String {
        String::from_str(&_env, "QuasarDEX LP")
    }

    pub fn symbol(_env: Env) -> String {
        String::from_str(&_env, "QDLP")
    }
}
