use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, WasmMsg, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128,
};

use cw_storage_plus::Item;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const USDC_CONTRACT_ADDR: &str = "usdc_contract_address";
const FYUSDC_CONTRACT_ADDR: &str = "fyusdc_contract_address";
const MATCHING_ENGINE_CONTRACT_ADDR: &str = "matching_engine_contract_address";


// Data Structures

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Order {
    pub id: String,
    pub owner: Addr,
    pub amount: Uint128,
    pub price: Uint128,
}

use cw_storage_plus::{Item, Map};

pub const BID_ORDERBOOK: Map<&str, Order> = Map::new("bid_orderbook");
pub const ASK_ORDERBOOK: Map<&str, Order> = Map::new("ask_orderbook");


// Initialization

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {}

#[entry_point]
pub fn init(_deps: DepsMut, _env: Env, _info: MessageInfo, _msg: InitMsg) -> StdResult<Response> {
    Ok(Response::default())
}


// Message Handlers

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    CreateBid { amount: Uint128, price: Uint128 },
    CreateAsk { amount: Uint128, price: Uint128 },
    CancelBid { id: String },
    CancelAsk { id: String },
    UpdateBidOrder { id: String, new_quantity: Uint128 },
    UpdateAskOrder { id: String, new_quantity: Uint128 },
}

#[entry_point]
pub fn handle(deps: DepsMut, env: Env, info: MessageInfo, msg: HandleMsg) -> StdResult<Response> {
    match msg {
        HandleMsg::CreateBid { amount, price } => create_bid(deps, env, info, amount, price),
        HandleMsg::CreateAsk { amount, price } => create_ask(deps, env, info, amount, price),
        HandleMsg::CancelBid { id } => cancel_bid(deps, env, info, id),
        HandleMsg::CancelAsk { id } => cancel_ask(deps, env, info, id),
        HandleMsg::UpdateBidOrder { id, new_quantity } => update_bid_order(deps, env, info, id, new_quantity),
        HandleMsg::UpdateAskOrder { id, new_quantity } => update_ask_order(deps, env, info, id, new_quantity),
    }
}


// Implement create_bid(), create_ask(), cancel_bid(), and cancel_ask() functions

pub fn check_usdc_balance(
    deps: &Deps,
    owner: &Addr,
    required_balance: &Uint128,
) -> StdResult<()> {
    let usdc_balance = deps.querier.query_balance(owner, USDC_CONTRACT_ADDR)?;
    if usdc_balance.amount < *required_balance {
        Err(StdError::generic_err("Insufficient USDC balance"))
    } else {
        Ok(())
    }
}

pub fn check_fyusdc_balance(
    deps: &Deps,
    owner: &Addr,
    required_balance: &Uint128,
) -> StdResult<()> {
    let fyusdc_balance = deps.querier.query_balance(owner, FYUSDC_CONTRACT_ADDR)?;
    if fyusdc_balance.amount < *required_balance {
        Err(StdError::generic_err("Insufficient fyUSDC balance"))
    } else {
        Ok(())
    }
}

pub fn update_bid_order(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    id: String,
    new_quantity: Uint128,
) -> StdResult<Response> {
    let mut order = BID_ORDERBOOK.load(deps.storage, &id)?;
    order.amount = new_quantity;
    BID_ORDERBOOK.save(deps.storage, &id, &order)?;
    Ok(Response::default())
}

pub fn update_ask_order(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    id: String,
    new_quantity: Uint128,
) -> StdResult<Response> {
    let mut order = ASK_ORDERBOOK.load(deps.storage, &id)?;
    order.amount = new_quantity;
    ASK_ORDERBOOK.save(deps.storage, &id, &order)?;
    Ok(Response::default())
}


pub fn create_bid(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    price: Uint128,
    quantity: Uint128,
) -> StdResult<Response> {
    // Check that the user has enough USDC
    let required_balance = price * quantity;
    check_usdc_balance(&deps.as_ref(), &info.sender, &required_balance)?;

    // Load orders from storage
    let order_id = generate_order_id();

    // Create and add the bid order to the orderbook
    let order_id = generate_order_id();
    let order = Order {
        id: order_id.clone(),
        owner: info.sender.clone(),
        price,
        amount: quantity,
    };

    insert_bid_order(&mut state.bid_orderbook, order);

 // Save the updated orderbook to storage
    BID_ORDERBOOK.save(deps.storage, &order_id, &order)?;

    // Escrow USDC tokens
    let escrow_usdc = WasmMsg::Execute {
        contract_addr: USDC_CONTRACT_ADDR.into(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: env.contract.address.to_string(),
            amount: (price * quantity),
        })?,
        funds: vec![],
    };

    // Call match_orders in the matching_engine contract
    let call_matching_engine = WasmMsg::Execute {
        contract_addr: MATCHING_ENGINE_CONTRACT_ADDR.into(),
        msg: to_binary(&HandleMsg::MatchOrders {})?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(escrow_usdc)
        .add_message(call_matching_engine)
        .add_attribute("action", "create_bid")
        .add_attribute("order_id", order_id))
}




pub fn create_ask(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    price: Uint128,
    quantity: Uint128,
) -> StdResult<Response> {
    // Check that the user has enough fyUSDC
    check_fyusdc_balance(&deps.as_ref(), &info.sender, &quantity)?;

    // Load orders from storage
    let order_id = generate_order_id();

    // Create and add the ask order to the orderbook
    let order_id = generate_order_id();
    let order = Order {
        id: order_id.clone(),
        owner: info.sender.clone(),
        price,
        amount: quantity,
    };
    insert_ask_order(&mut state.ask_orderbook, order);

    // Save the updated orderbook to storage
    ASK_ORDERBOOK.save(deps.storage, &order_id, &order)?;

    // Escrow fyUSDC tokens
    let escrow_fyusdc = WasmMsg::Execute {
        contract_addr: FYUSDC_CONTRACT_ADDR.into(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: env.contract.address.to_string(),
            amount: quantity,
        })?,
        funds: vec![],
    };

    // Call match_orders in the matching_engine contract
    let call_matching_engine = WasmMsg::Execute {
        contract_addr: MATCHING_ENGINE_CONTRACT_ADDR.into(),
        msg: to_binary(&HandleMsg::MatchOrders {})?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(escrow_fyusdc)
        .add_message(call_matching_engine)
        .add_attribute("action", "create_ask")
        .add_attribute("order_id", order_id))
}



pub fn cancel_bid(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    order_id: String,
) -> StdResult<Response> {
    // Load order from storage
    let order = BID_ORDERBOOK.remove(deps.storage, &order_id)?;

    // Return escrowed USDC tokens
    let return_usdc = WasmMsg::Execute {
        contract_addr: USDC_CONTRACT_ADDR.into(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: order.owner.to_string(),
            amount: (order.price * order.amount),
        })?,
        funds: vec![],
    };


    Ok(Response::new()
        .add_message(return_usdc)
        .add_attribute("action", "cancel_bid")
        .add_attribute("order_id", order_id))
}


pub fn cancel_ask(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    order_id: String,
) -> StdResult<Response> {
    // Load order from storage
    let order = ASK_ORDERBOOK.remove(deps.storage, &order_id)?;

    // Return escrowed fyUSDC tokens
    let return_fyusdc = WasmMsg::Execute {
        contract_addr: FYUSDC_CONTRACT_ADDR.into(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: order.owner.to_string(),
            amount: order.amount,
        })?,
        funds: vec![],
    };


    Ok(Response::new()
        .add_message(return_fyusdc)
        .add_attribute("action", "cancel_ask")
        .add_attribute("order_id", order_id))
}



// Query Handlers

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetBidOrderbook {},
    GetAskOrderbook {},
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetBidOrderbook {} => to_binary(&get_bid_orderbook(deps)?),
        QueryMsg::GetAskOrderbook {} => to_binary(&get_ask_orderbook(deps)?),
    }
}

pub fn get_bid_orderbook(deps: Deps) -> StdResult<Vec<Order>> {
    let orders: Vec<Order> = BID_ORDERBOOK.values(deps.storage).collect();
    Ok(orders)
}

pub fn get_ask_orderbook(deps: Deps) -> StdResult<Vec<Order>> {
    let orders: Vec<Order> = ASK_ORDERBOOK.values(deps.storage).collect();
    Ok(orders)
}

// Helper Functions

pub fn generate_order_id() -> String {
    // Replace this with a proper order ID generation mechanism.
    format!("{}", uuid::Uuid::new_v4())
}

pub fn insert_bid_order(orderbook: &mut Vec<Order>, new_order: Order) {
    let index = orderbook
        .iter()
        .position(|order| order.price < new_order.price)
        .unwrap_or(orderbook.len());
    orderbook.insert(index, new_order);
}

pub fn insert_ask_order(orderbook: &mut Vec<Order>, new_order: Order) {
    let index = orderbook
        .iter()
        .position(|order| order.price > new_order.price)
        .unwrap_or(orderbook.len());
    orderbook.insert(index, new_order);
}

pub fn remove_bid_order(
    orderbook: &mut Vec<Order>,
    sender: &Addr,
    id: &str,
) -> StdResult<Order> {
    if let Some(index) = orderbook.iter().position(|order| order.owner == *sender && order.id == *id) {
        Ok(orderbook.remove(index))
    } else {
        Err(StdError::generic_err("Bid not found"))
    }
}

pub fn remove_ask_order(
    orderbook: &mut Vec<Order>,
    sender: &Addr,
    id: &str,
) -> StdResult<Order> {
    if let Some(index) = orderbook.iter().position(|order| order.owner == *sender && order.id == *id) {
        Ok(orderbook.remove(index))
    } else {
        Err(StdError::generic_err("Ask not found"))
    }
}

// Unit tests and integration tests will be written later.