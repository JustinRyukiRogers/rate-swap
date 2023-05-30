use cosmwasm_schema::cw_serde;
use cosmwasm_schema::{schemars, serde};

use serde::{Serialize, Deserialize};


use cosmwasm_std::{Addr, Coin, Env, Order as cOrder, StdResult, StdError, Storage, Timestamp, Uint128, to_binary, Decimal};
use cw_storage_plus::Map;
use cw_storage_plus::Item;
use std::collections::HashMap;


use cw20::{Balance, Cw20CoinVerified, Expiration};


// State of the contract
#[cw_serde]
pub struct State {
    pub contract_owner: Addr,
    pub authorized_checker: Addr,
    pub liquidation_deadline: Expiration,
    pub liquidator: Addr,
    pub order_manager_contract: Addr,
    pub liquidation_threshold: u64,
    pub liquidation_penalty: u64,
    pub fyusdc_contract: Addr,
    pub usdc_contract: Addr,
    pub rsp_contract: Addr,
    pub atom_contract: Addr,

}

impl State {
    // function to update state
    pub fn update(
        &mut self,
        caller: &Addr,
        new_authorized_checker: Option<Addr>,
        new_liquidation_deadline: Option<Expiration>,
        new_liquidator: Option<Addr>,
        new_order_manager_contract: Option<Addr>,
        new_liquidation_threshold: Option<u64>,
        new_liquidation_penalty: Option<u64>,
        new_fyusdc_contract: Option<Addr>,
    ) -> StdResult<()> {  
        if caller != &self.contract_owner {
            return Err(StdError::generic_err("Unauthorized"));  
        }
        if let Some(checker) = new_authorized_checker {
            self.authorized_checker = checker;
        }
        if let Some(deadline) = new_liquidation_deadline {
            self.liquidation_deadline = deadline;
        }
        if let Some(liquidator) = new_liquidator {
            self.liquidator = liquidator;
        }
        if let Some(manager) = new_order_manager_contract {
            self.order_manager_contract = manager;
        }
        if let Some(threshold) = new_liquidation_threshold {
            self.liquidation_threshold = threshold;
        }
        if let Some(penalty) = new_liquidation_penalty {
            self.liquidation_penalty = penalty;
        }
        if let Some(fyusdc) = new_fyusdc_contract {
            self.fyusdc_contract = fyusdc;
        }
        Ok(())
    }
}


// Added constant for state storage
pub const STATE: Item<State> = Item::new("state");
pub const COLLATERALS: Map<&Addr, Uint128> = Map::new("collaterals");
pub const LOANS: Map<Addr, Uint128> = Map::new("loans");
pub const CONTRACT_USDC_BALANCE: Item<Uint128> = Item::new("contract_usdc_balance");





#[cw_serde]
#[derive(Default)]
pub struct GenericBalance {
    pub native: Vec<Coin>,
    pub cw20: Vec<Cw20CoinVerified>,
}



impl GenericBalance {
    pub fn add_tokens(&mut self, add: Balance) {
        match add {
            Balance::Native(balance) => {
                for token in balance.0 {
                    let index = self.native.iter().enumerate().find_map(|(i, exist)| {
                        if exist.denom == token.denom {
                            Some(i)
                        } else {
                            None
                        }
                    });
                    match index {
                        Some(idx) => self.native[idx].amount += token.amount,
                        None => self.native.push(token),
                    }
                }
            }
            Balance::Cw20(token) => {
                let index = self.cw20.iter().enumerate().find_map(|(i, exist)| {
                    if exist.address == token.address {
                        Some(i)
                    } else {
                        None
                    }
                });
                match index {
                    Some(idx) => self.cw20[idx].amount += token.amount,
                    None => self.cw20.push(token),
                }
            }
        };
    }
}




#[cw_serde]
pub struct Escrow {
    /// arbiter can decide to approve or refund the escrow
    pub arbiter: Addr,
    /// if approved, funds go to the recipient, cannot approve if recipient is none
    pub recipient: Option<Addr>,
    /// if refunded, funds go to the source
    pub source: Addr,
    /// Title of the escrow, for example for a bug bounty "Fix issue in contract.rs"
    pub title: String,
    /// Description of the escrow, a more in depth description of how to meet the escrow condition
    pub description: String,
    /// When end height set and block height exceeds this value, the escrow is expired.
    /// Once an escrow is expired, it can be returned to the original funder (via "refund").
    pub end_height: Option<u64>,
    /// When end time (in seconds since epoch 00:00:00 UTC on 1 January 1970) is set and
    /// block time exceeds this value, the escrow is expired.
    /// Once an escrow is expired, it can be returned to the original funder (via "refund").
    pub end_time: Option<u64>,
    /// Balance in Native and Cw20 tokens
    pub balance: GenericBalance,
    /// All possible contracts that we accept tokens from
    pub cw20_whitelist: Vec<Addr>,
}

impl Escrow {
    pub fn is_expired(&self, env: &Env) -> bool {
        if let Some(end_height) = self.end_height {
            if env.block.height > end_height {
                return true;
            }
        }

        if let Some(end_time) = self.end_time {
            if env.block.time > Timestamp::from_seconds(end_time) {
                return true;
            }
        }

        false
    }

    pub fn human_whitelist(&self) -> Vec<String> {
        self.cw20_whitelist.iter().map(|a| a.to_string()).collect()
    }
}

pub const ESCROWS: Map<&str, Escrow> = Map::new("escrow");

/// This returns the list of ids for all registered escrows
pub fn all_escrow_ids(storage: &dyn Storage) -> StdResult<Vec<String>> {
    ESCROWS
        .keys(storage, None, None, cOrder::Ascending)
        .collect()
}

