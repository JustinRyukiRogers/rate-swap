use cosmwasm_std::{
    to_binary, Addr, Api, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::msg::{Cw20ExecuteMsg, Empty, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{load, Order, State};
use cosmwasm_std::Storage;


// In your contract.rs
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let state = State {
        order_manager: msg.order_manager,
        fyusdc_contract: msg.fyusdc_contract,
        usdc_contract: msg.usdc_contract,
    };
    state.save(deps.storage)?;
    Ok(Response::default())
}

// In your msg.rs
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub order_manager: Addr,
    pub fyusdc_contract: Addr,
    pub usdc_contract: Addr,
}

// In your state.rs
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub order_manager: Addr,
    pub fyusdc_contract: Addr,
    pub usdc_contract: Addr,
}


pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::MatchOrders {} => match_orders(deps, env, info),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    GetBidOrderbook {},
    GetAskOrderbook {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    MatchOrders {},
    UpdateBidOrder {
        id: String,
        new_quantity: Uint128,
    },
    UpdateAskOrder {
        id: String,
        new_quantity: Uint128,
    },
    RemoveBidOrder {
        id: String,
    },
    RemoveAskOrder {
        id: String,
    },
}

pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let state: State = load(deps.storage)?;
    match msg {
        QueryMsg::GetBidOrderbook {} => query_order_manager(deps, &state, msg),
        QueryMsg::GetAskOrderbook {} => query_order_manager(deps, &state, msg),
    }
}

fn match_orders(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let state: State = load(deps.storage)?;

    let bid_orderbook_binary: Binary = query_order_manager(deps.as_ref(), &state, QueryMsg::GetBidOrderbook {})?;
    let ask_orderbook_binary: Binary = query_order_manager(deps.as_ref(), &state, QueryMsg::GetAskOrderbook {})?;

    let bid_orderbook: Vec<Order> = from_binary(&bid_orderbook_binary)?;
    let ask_orderbook: Vec<Order> = from_binary(&ask_orderbook_binary)?;

    let matches = find_matching_orders(&bid_orderbook, &ask_orderbook)?;

    // Perform necessary token transfers and other actions based on the matches
    let mut responses = vec![];
    for (bid, ask, quantity) in matches {
        let usdc_amount = quantity;
        let fyusdc_amount = bid.price * quantity;
        let response = transfer_tokens(deps.branch(), &state, &bid.buyer, &ask.seller, usdc_amount, fyusdc_amount)?;
        responses.push(response);
    }

    // Cancel matched orders in the order_manager contract
    for (bid, ask, _) in &matches {
        let cancel_bid_msg = WasmMsg::Execute {
            contract_addr: state.order_manager.to_string(),
            msg: to_binary(&HandleMsg::CancelBid { id: bid.id.clone() })?,
            funds: vec![],
        };

        let cancel_ask_msg = WasmMsg::Execute {
            contract_addr: state.order_manager.to_string(),
            msg: to_binary(&HandleMsg::CancelAsk { id: ask.id.clone() })?,
            funds: vec![],
        };

        let res = deps.querier.execute(cancel_bid_msg)?;
        let _ = deps.querier.execute(cancel_ask_msg)?;
    }

    // Update partially filled orders in the order_manager contract
    for (bid, ask, matched_quantity) in &matches {
        if bid.quantity != matched_quantity {
            let update_bid_msg = WasmMsg::Execute {
                contract_addr: state.order_manager.to_string(),
                msg: to_binary(&ExecuteMsg::UpdateBidOrder {
                    id: bid.id.clone(),
                    new_quantity: bid.quantity - matched_quantity,
                })?,
                funds: vec![],
            };
            let _ = deps.querier.execute(update_bid_msg)?;
    }

    if ask.quantity != matched_quantity {
        let update_ask_msg = WasmMsg::Execute {
            contract_addr: state.order_manager.to_string(),
            msg: to_binary(&ExecuteMsg::UpdateAskOrder {
                id: ask.id.clone(),
                new_quantity: ask.quantity - matched_quantity,
            })?,
            funds: vec![],
        };
        let _ = deps.querier.execute(update_ask_msg)?;
    }
}

    let mut res = Response::new()
        .add_attribute("action", "match_orders")
        .add_attribute("matches", format!("{:?}", matches));

    // Add messages from token transfers
    for response in responses {
        res = res.add_messages(response.messages);
    }

    Ok(res)
}



fn find_matching_orders(
    bid_orderbook: &[Order],
    ask_orderbook: &[Order],
) -> StdResult<Vec<(Order, Order, Uint128)>> {
    let mut matches: Vec<(Order, Order, Uint128)> = vec![];

    let mut bid_iter = bid_orderbook.iter();
    let mut ask_iter = ask_orderbook.iter();
    let mut current_bid = bid_iter.next();
    let mut current_ask = ask_iter.next();

    while let (Some(bid), Some(ask)) = (current_bid, current_ask) {
        if bid.price >= ask.price {
            let matched_quantity = std::cmp::min(bid.quantity, ask.quantity);
            matches.push((bid.clone(), ask.clone(), matched_quantity));

            if bid.quantity == matched_quantity {
                current_bid = bid_iter.next();
            } else {
                current_bid = Some(&Order {
                    id: bid.id.clone(),
                    buyer: bid.buyer.clone(),
                    seller: bid.seller.clone(),
                    price: bid.price,
                    quantity: bid.quantity - matched_quantity,
                });
            }

            if ask.quantity == matched_quantity {
                current_ask = ask_iter.next();
            } else {
                current_ask = Some(&Order {
                    id: ask.id.clone(),
                    buyer: ask.buyer.clone(),
                    seller: ask.seller.clone(),
                    price: ask.price,
                    quantity: ask.quantity - matched_quantity,
                });
            }
        } else {
            break;
        }
    }

    Ok(matches)
}

//might need to come back and deal with partially filled orders. 

let bid_orderbook_binary: Binary = query_order_manager(deps.as_ref(), &state, QueryMsg::GetBidOrderbook {})?;
let bid_orderbook: Vec<Order> = from_binary(&bid_orderbook_binary)?;
let best_bid = find_best_bid(&bid_orderbook);
let ask_orderbook_binary: Binary = query_order_manager(deps.as_ref(), &state, QueryMsg::GetAskOrderbook {})?;
let ask_orderbook: Vec<Order> = from_binary(&ask_orderbook_binary)?;
let best_ask = find_best_ask(&ask_orderbook);


pub fn find_best_bid(bid_orderbook: &[Order]) -> Option<Order> {
    bid_orderbook.first().cloned()
}

pub fn find_best_ask(ask_orderbook: &[Order]) -> Option<Order> {
    ask_orderbook.first().cloned()
}

pub fn update_orders<S: Storage>(
    storage: &mut S,
    matched_bid: &Order,
    matched_ask: &Order,
    quantity: Uint128,
) -> StdResult<()> {
    let state: State = load(storage)?;

    // Update bid orderbook
    let bid_orderbook_binary: Binary = query_order_manager(deps.as_ref(), &state, QueryMsg::GetBidOrderbook {})?;
    let mut bid_orderbook: Vec<Order> = from_binary(&bid_orderbook_binary)?;

    // Remove orders if fully matched, otherwise update the remaining quantity
    if matched_bid.quantity == quantity {
        remove_bid_order(&mut bid_orderbook, &matched_bid.id)?;
    } else {
        update_bid_order(&mut bid_orderbook, &matched_bid.id, matched_bid.quantity - quantity)?;
    }

    // Update ask orderbook
    let ask_orderbook_binary: Binary = query_order_manager(deps.as_ref(), &state, QueryMsg::GetAskOrderbook {})?;
    let mut ask_orderbook: Vec<Order> = from_binary(&ask_orderbook_binary)?;

    if matched_ask.quantity == quantity {
        remove_ask_order(&mut ask_orderbook, &matched_ask.id)?;
    } else {
        update_ask_order(&mut ask_orderbook, &matched_ask.id, matched_ask.quantity - quantity)?;
    }

    // Save the updated orderbooks in the order_manager contract
    // You should implement the proper ExecuteMsg variants and handling for updating the orderbooks in the order_manager contract

    Ok(())
}

fn query_order_manager(
    deps: Deps,
    state: &State,
    msg: QueryMsg,
) -> StdResult<Binary> {
    let query_msg = to_binary(&msg)?;
    let response: StdResult<Binary> = deps.querier.query_wasm_smart(
        &state.order_manager,
        &query_msg,
    );
    response
}

fn transfer_tokens(
    deps: DepsMut,
    state: &State,
    buyer: &Addr,
    seller: &Addr,
    usdc_amount: Uint128,
    fyusdc_amount: Uint128,
) -> StdResult<Response> {
    // Transfer USDC from buyer to seller
    let usdc_transfer_msg = BankMsg::Send {
        from_address: buyer.clone(),
        to_address: seller.clone(),
        amount: vec![Coin {
            denom: state.usdc_denom.clone(),
            amount: usdc_amount,
        }],
    };

    // Transfer fyUSDC from seller to buyer using the CW20 Send message
    let fyusdc_send_msg = Cw20ExecuteMsg::Send {
        contract: state.fyusdc_contract.clone(),
        amount: fyusdc_amount,
        msg: to_binary(&Empty{})?,
    };
    let fyusdc_transfer_msg = WasmMsg::Execute {
        contract_addr: seller.clone(),
        msg: to_binary(&fyusdc_send_msg)?,
        funds: vec![],
    };

    let res = Response::new()
        .add_message(usdc_transfer_msg)
        .add_message(fyusdc_transfer_msg)
        .add_attribute("action", "transfer_tokens")
        .add_attribute("from", buyer)
        .add_attribute("to", seller)
        .add_attribute("usdc_amount", usdc_amount.to_string())
        .add_attribute("fyusdc_amount", fyusdc_amount.to_string());

    Ok(res)
}

fn create_cancel_messages(
    deps: &DepsMut,
    order_manager_addr: &Addr,
    bid_id: &str,
    ask_id: &str,
) -> StdResult<Vec<WasmMsg>> {
    let cancel_bid_msg = WasmMsg::Execute {
        contract_addr: order_manager_addr.to_string(),
        msg: to_binary(&HandleMsg::CancelBid { id: bid_id.to_string() })?,
        funds: vec![],
    };

    let cancel_ask_msg = WasmMsg::Execute {
        contract_addr: order_manager_addr.to_string(),
        msg: to_binary(&HandleMsg::CancelAsk { id: ask_id.to_string() })?,
        funds: vec![],
    };

    Ok(vec![cancel_bid_msg, cancel_ask_msg])
}