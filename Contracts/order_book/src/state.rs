use cosmwasm_schema::cw_serde;
use cosmwasm_schema::{schemars, serde};

use serde::{Serialize, Deserialize};


use cosmwasm_std::{Addr, Coin, Env, Order as cOrder, StdResult, StdError, Storage, Timestamp, Uint128, to_binary};
use cw_storage_plus::Map;
use cw_storage_plus::Item;
use std::collections::HashMap;


use cw20::{Balance, Cw20CoinVerified};

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
pub struct State {
    pub fyusdc_contract: Addr,
    pub usdc_contract: Addr,
    pub max_order_id: u64,

}

pub static STATE: Item<State> = Item::new("state");
pub static ORDER_BOOK: Map<&str, OrderBucket> = Map::new("order_book");



#[cw_serde]
pub struct OrderBucket {
    pub price: String,
    pub bids: Vec<Order>,
    pub asks: Vec<Order>,
}

impl OrderBucket {
    pub fn add_order(&mut self, order: Order, order_type: OrderType) {
        match order_type {
            OrderType::Bid => self.bids.push(order),
            OrderType::Ask => self.asks.push(order),
        }
    }

    pub fn remove_order(&mut self, order_id: &str) -> StdResult<()> {
        let bid_position = self.bids.iter().position(|order| order.id == order_id);
        let ask_position = self.asks.iter().position(|order| order.id == order_id);

        match (bid_position, ask_position) {
            (Some(bid_idx), None) => {
                self.bids.remove(bid_idx);
                Ok(())
            }
            (None, Some(ask_idx)) => {
                self.asks.remove(ask_idx);
                Ok(())
            }
            (_, _) => Err(StdError::generic_err("Order does not exist in the bucket")),
        }
    }
}

pub enum OrderType {
    Bid,
    Ask,
}


#[cw_serde]
pub struct Order {
    pub id: String,
    pub owner: Addr,
    pub quantity: Uint128,
    pub price: Uint128,
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

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::testing::MockStorage;

    #[test]
    fn no_escrow_ids() {
        let storage = MockStorage::new();
        let ids = all_escrow_ids(&storage).unwrap();
        assert_eq!(0, ids.len());
    }

    fn dummy_escrow() -> Escrow {
        Escrow {
            arbiter: Addr::unchecked("arb"),
            recipient: Some(Addr::unchecked("recip")),
            source: Addr::unchecked("source"),
            title: "some_escrow".to_string(),
            description: "some escrow desc".to_string(),
            end_height: None,
            end_time: None,
            balance: Default::default(),
            cw20_whitelist: vec![],
        }
    }

    #[test]
    fn all_escrow_ids_in_order() {
        let mut storage = MockStorage::new();
        ESCROWS.save(&mut storage, "lazy", &dummy_escrow()).unwrap();
        ESCROWS
            .save(&mut storage, "assign", &dummy_escrow())
            .unwrap();
        ESCROWS.save(&mut storage, "zen", &dummy_escrow()).unwrap();

        let ids = all_escrow_ids(&storage).unwrap();
        assert_eq!(3, ids.len());
        assert_eq!(
            vec!["assign".to_string(), "lazy".to_string(), "zen".to_string()],
            ids
        )
    }
}
