use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, BankMsg, Binary, CanonicalAddr, CosmosMsg, Deps, DepsMut, Env, MessageInfo, 
    QuerierWrapper, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};

use cw2::set_contract_version;
use cw20::{Balance, Cw20Coin, Cw20CoinVerified, Cw20ExecuteMsg, Cw20ReceiveMsg};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// rest of the code


const USDC_CONTRACT_ADDR: &str = "usdc_contract_address";
const FYUSDC_CONTRACT_ADDR: &str = "fyusdc_contract_address";
const MATCHING_ENGINE_CONTRACT_ADDR: &str = "matching_engine_contract_address";


// Data Structures

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Order {
    pub id: String,
    pub owner: Addr,
    pub quantity: Uint128,
    pub price: Uint128,
}

use cw_storage_plus::{Item, Map};

pub const BID_ORDERBOOK: Map<&str, Order> = Map::new("bid_orderbook");
pub const ASK_ORDERBOOK: Map<&str, Order> = Map::new("ask_orderbook");
pub const USER_BIDS: Map<&str, Vec<String>> = Map::new("user_bids");
pub const USER_ASKS: Map<&str, Vec<String>> = Map::new("user_asks");

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
    CreateBid { quantity: Uint128, price: Uint128 },
    CreateAsk { quantity: Uint128, price: Uint128 },
    CancelBid { id: String },
    CancelAsk { id: String },
    UpdateBidOrder { id: String, new_quantity: Uint128 },
    UpdateAskOrder { id: String, new_quantity: Uint128 },
    MatchOrders {},  // Add this line.
    },
}

#[entry_point]
pub fn handle(deps: DepsMut, env: Env, info: MessageInfo, msg: HandleMsg) -> StdResult<Response> {
    match msg {
        HandleMsg::CreateBid { quantity, price } => create_bid(deps, env, info, quantity, price),
        HandleMsg::CreateAsk { quantity, price } => create_ask(deps, env, info, quantity, price),
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
) -> StdResult<Uint128> {
    let usdc_balance = deps.querier.query_balance(owner, USDC_CONTRACT_ADDR)?;
    if usdc_balance.balance < *required_balance {
        Err(StdError::generic_err("Insufficient USDC balance"))
    } else {
        Ok(usdc_balance.balance)
    }
}

pub fn check_fyusdc_balance(
    deps: &Deps,
    owner: &Addr,
    required_balance: &Uint128,
) -> StdResult<(Uint128)> {
    let fyusdc_balance = deps.querier.query_balance(owner, FYUSDC_CONTRACT_ADDR)?;
    if fyusdc_balance.balance < *required_balance {
        Err(StdError::generic_err("Insufficient fyUSDC balance"))
    } else {
        Ok(fyusdc_balance.balance)
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
    order.quantity = new_quantity;
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
    order.quantity = new_quantity;
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

    // Check if the sent amount of tokens is equal to the required balance
    let sent_amount = info.funds.iter().find(|coin| coin.denom == "usdc").map(|coin| coin.amount).unwrap_or_else(Uint128::zero);
    if sent_amount != required_balance {
        return Err(StdError::generic_err("Insufficient funds sent"));
    }

    // Load orders from storage
    let order_id = generate_order_id();

    // Create and add the bid order to the orderbook
    let order_id = generate_order_id();
    let order = Order {
        id: order_id.clone(),
        owner: info.sender.clone(),
        price,
        quantity: quantity,
    };

    insert_bid_order(&mut state.bid_orderbook, order);

    // Save the updated orderbook to storage
    BID_ORDERBOOK.save(deps.storage, &order_id, &order)?;

    // Add the order ID to the list of orders for the user
    let mut user_bids = USER_BIDS.may_load(deps.storage, info.sender.as_str())?.unwrap_or(Vec::new());
    user_bids.push(order_id.clone());
    USER_BIDS.save(deps.storage, info.sender.as_str(), &user_bids)?;

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
    let required_balance = quantity;
    check_fyusdc_balance(&deps.as_ref(), &info.sender, &required_balance)?;

    // Generate order ID
    let order_id = generate_order_id();

    // Create and add the ask order to the orderbook
    let order = Order {
        id: order_id.clone(),
        owner: info.sender.clone(),
        price,
        quantity: quantity,
    };

    insert_ask_order(&mut state.bid_orderbook, order);
    ASK_ORDERBOOK.save(deps.storage, &order_id, &order)?;

    // Add the ask ID to the list of asks for the user
    let mut user_asks = USER_ASKS.may_load(deps.storage, info.sender.as_str())?.unwrap_or(Vec::new());
    user_asks.push(order_id.clone());
    USER_ASKS.save(deps.storage, info.sender.as_str(), &user_asks)?;

    // Escrow fyUSDC tokens
    let escrow_fyusdc = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: FYUSDC_CONTRACT_ADDR.into(),
        msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
            owner: info.sender.to_string(),
            recipient: env.contract.address.to_string(),
            amount: quantity,
        })?,
        funds: vec![],
    });

    // Call match_orders in the matching_engine contract
    let call_matching_engine = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: MATCHING_ENGINE_CONTRACT_ADDR.into(),
        msg: to_binary(&HandleMsg::MatchOrders)?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_submessage(SubMsg::new(escrow_fyusdc))
        .add_submessage(SubMsg::new(call_matching_engine))
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
            amount: (order.price * order.quantity),
        })?,
        funds: vec![],
    };

    let mut user_bids = USER_BIDS.load(deps.storage, info.sender.as_str())?;
    user_bids.retain(|id| id != &order_id);
    USER_BIDS.save(deps.storage, info.sender.as_str(), &user_bids)?;


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
            amount: order.quantity,
        })?,
        funds: vec![],
    };

    let mut user_asks = USER_ASKS.load(deps.storage, info.sender.as_str())?;
    user_asks.retain(|id| id != &order_id);
    USER_ASKS.save(deps.storage, info.sender.as_str(), &user_asks)?;


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
    GetUserBids { user: Addr },
    GetUserAsks { user: Addr },
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetBidOrderbook {} => to_binary(&get_bid_orderbook(deps)?),
        QueryMsg::GetAskOrderbook {} => to_binary(&get_ask_orderbook(deps)?),
        QueryMsg::GetUserBids { user } => to_binary(&get_user_bids(deps, user)?),
        QueryMsg::GetUserAsks { user } => to_binary(&get_user_asks(deps, user)?),


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

pub fn get_user_bids(deps: Deps, user: String) -> StdResult<Vec<Order>> {
    let bid_ids = USER_BIDS.load(deps.storage, &user)?;
    let mut bids = Vec::new();
    for bid_id in bid_ids {
        let bid = BIDS.load(deps.storage, &bid_id)?;
        bids.push(Order {
            id: bid_id,
            price: bid.price,
            quantity: bid.quantity,
        });
    }
    Ok(bids)
}

pub fn get_user_asks(deps: Deps, user: String) -> StdResult<Vec<Order>> {
    let ask_ids = USER_ASKS.load(deps.storage, &user)?;
    let mut asks = Vec::new();
    for ask_id in ask_ids {
        let ask = ASKS.load(deps.storage, &ask_id)?;
        asks.push(Order {
            id: ask_id,
            price: ask.price,
            quantity: ask.quantity,
        });
    }
    Ok(asks)
}

// Helper Functions

pub fn generate_order_id() -> String {
    // Replace this with a proper order ID generation mechanism.
    format!("{}", uuid::Uuid::new_v4().to_string())
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

pub fn execute_receive(
    deps: DepsMut,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
) -> StdResult<Response> {
    // Check that the sender is either the USDC or fyUSDC contract
    match info.sender.as_str() {
        USDC_CONTRACT_ADDR | FYUSDC_CONTRACT_ADDR => (),
        _ => return Err(StdError::generic_err("Invalid sender")),
    }

    // Verify that the tokens are being sent by the order's owner
    let order_id = parse_order_id_from_msg(&wrapper.msg)?;
    let order = BID_ORDERBOOK.load(deps.storage, &order_id)?;
    if order.owner != wrapper.sender {
        return Err(StdError::generic_err("Unauthorized"));
    }

    // Update the escrowed amount for the order
    let new_escrowed_amount = wrapper.amount + order.escrowed_amount;
    BID_ORDERBOOK.update(deps.storage, &order_id, |mut order| {
        order.escrowed_amount = new_escrowed_amount;
        Ok(order)
    })?;

    Ok(Response::new()
        .add_attribute("action", "receive")
        .add_attribute("from", &wrapper.sender)
        .add_attribute("amount", wrapper.amount.to_string())
        .add_attribute("order_id", order_id))
}



// Unit tests and integration tests will be written later.