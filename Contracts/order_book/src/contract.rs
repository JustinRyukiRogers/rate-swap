#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, StdError, SubMsg, WasmMsg, Uint128, Decimal, CosmosMsg
};

use cw2::set_contract_version;
use cw20::{Balance, Cw20Coin, Cw20CoinVerified, Cw20ExecuteMsg, Cw20ReceiveMsg};
use std::collections::HashMap;

use crate::error::ContractError;
use crate::msg::{
    CreateMsg, DetailsResponse, ExecuteMsg, InstantiateMsg, ListResponse, QueryMsg, ReceiveMsg, OrderbookResponse, UserOrdersResponse, StateResponse
};
use crate::state::{State, Order, OrderBucket, all_escrow_ids, Escrow, GenericBalance, ESCROWS, OrderType, STATE, ORDER_BOOK, ORDERS};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw20-escrow";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    let state = State {
        fyusdc_contract: deps.api.addr_validate(&_msg.fyusdc_contract)?,
        usdc_contract: deps.api.addr_validate(&_msg.usdc_contract)?,
        max_order_id: 0,
    };
    STATE.save(deps.storage, &state)?;

    // Define price points. Since Decimal has 18 fractional digits, we represent 0.5 as 500_000_000_000_000_000
    let mut price: Decimal = Decimal::new(Uint128::from(500_000_000_000_000_000u128));
    // 1.0 is represented as 1_000_000_000_000_000_000
    let end: Decimal = Decimal::new(Uint128::from(1_000_000_000_000_000_000u128));
    // 0.005 is represented as 5_000_000_000_000_000
    let increment: Decimal = Decimal::new(Uint128::from(5_000_000_000_000_000u128));

    while price <= end {
        let price_str = price.to_string();
        let order_bucket = OrderBucket {
            price: price_str.clone(),
            bids: Vec::new(),
            asks: Vec::new(),
        };
        ORDER_BOOK.save(deps.storage, &price_str, &order_bucket)?;
        // increment the price
        price = price + increment;
    }
    Ok(Response::default())
}


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, StdError> {
    match msg {
        ExecuteMsg::Create(msg) => {
            execute_create(deps, msg, Balance::from(info.funds), &info.sender)
        }
        ExecuteMsg::SetRecipient { id, recipient } => {
            execute_set_recipient(deps, env, info, id, recipient)
        }
        ExecuteMsg::Approve { id } => execute_approve(deps, env, info, id),
        ExecuteMsg::TopUp { id } => execute_top_up(deps, id, Balance::from(info.funds)),
        ExecuteMsg::Refund { id } => execute_refund(deps, env, info, id),
        ExecuteMsg::Receive(msg) => execute_receive(deps, env, info, msg),
        ExecuteMsg::CancelBid { order_id, price } => cancel_bid(deps, info, order_id, price),
        ExecuteMsg::CancelAsk { order_id, price } => cancel_ask(deps, info, order_id, price),
        ExecuteMsg::UpdateBidOrder { id, new_quantity } => update_bid_order(deps, env, info, id, new_quantity),
        ExecuteMsg::UpdateAskOrder { id, new_quantity } => update_ask_order(deps, env, info, id, new_quantity),
        //ExecuteMsg::MatchOrders {} => match_orders(deps, env, info),

    }
}

pub fn execute_receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
) -> Result<Response, StdError> {
    let state= STATE.load(deps.storage)?;
    let msg: ReceiveMsg = from_binary(&wrapper.msg)?;
    let balance = Balance::Cw20(Cw20CoinVerified {
        amount: wrapper.amount,
        address: info.sender.clone(),     
    });

    println!("info.sender: {:?}", info.sender);
    println!("State: {:?}",state.fyusdc_contract);
    println!("jeff");

    match &info.sender.clone() {
        sender if sender == state.fyusdc_contract || sender == state.usdc_contract => (),
        _ => return Err(StdError::generic_err("Invalid sender")),
    }

    
    let api = deps.api;
    match msg {
        ReceiveMsg::Create(msg) => {
            execute_create(deps, msg, balance, &api.addr_validate(&wrapper.sender)?)
        }
        ReceiveMsg::TopUp { id } => execute_top_up(deps, id, balance),
        ReceiveMsg::CreateAsk { orderer, quantity, price } => create_ask(deps, env, info, wrapper, orderer, price, quantity),
        ReceiveMsg::CreateBid { orderer, quantity, price } => create_bid(deps, env, info, wrapper, orderer, price, quantity),


    }
}




fn create_bid(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
    orderer: Addr,
    price: Decimal,
    quantity: Uint128,
) -> StdResult<Response> {
    // Load state
    let state = STATE.load(deps.storage)?;

    // Check that the user has sent enough USDC
    let required_balance = price * quantity;
    if wrapper.amount != required_balance {
        return Err(StdError::generic_err("Insufficient funds sent"));
    }

    // Create and add the bid order to the orderbook
    let order_id = generate_order_id( &mut deps)?;
    let order = Order {
        id: order_id.clone(),
        owner: info.sender.clone(),
        orderer: orderer,
        price,
        quantity: quantity,
        Type: "Bid".to_string()
    };

    // Load the order bucket for the price point from the orderbook
    let mut bucket = ORDER_BOOK.load(deps.storage, &format!("{}", price))?;
    // Add a cloned order to the bucket and save it back to the orderbook
    bucket.add_order(order.clone(), OrderType::Bid);
    ORDER_BOOK.save(deps.storage, &format!("{}", price), &bucket)?;
    ORDERS.save(deps.storage, &order.id, &order)?;


    Ok(Response::new()
        .add_attribute("action", "create_bid")
        .add_attribute("order_id", order_id))
}

fn create_ask(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
    orderer: Addr,
    price: Decimal,
    quantity: Uint128,
) -> StdResult<Response> {
    // Load state
    let state = STATE.load(deps.storage)?;

    // Check that the user has sent enough fyUSDC
    let required_balance = quantity;
    if wrapper.amount != required_balance {
        return Err(StdError::generic_err("Insufficient funds sent"));
    }

    // Create and add the bid order to the orderbook
    let order_id = generate_order_id( &mut deps)?;
    let order = Order {
        id: order_id.clone(),
        owner: info.sender.clone(),
        orderer: orderer,
        price,
        quantity: quantity,
        Type: "Ask".to_string()
    };

    // Load the order bucket for the price point from the orderbook
    let mut bucket = ORDER_BOOK.load(deps.storage, &format!("{}", price))?;
    // Add the order to the bucket and save it back to the orderbook
    bucket.add_order(order.clone(), OrderType::Ask);
    ORDER_BOOK.save(deps.storage, &format!("{}", price), &bucket)?;
    ORDERS.save(deps.storage, &order.id, &order)?;


    Ok(Response::new()
        .add_attribute("action", "create_ask")
        .add_attribute("order_id", order_id))
}

pub fn cancel_bid(
    mut deps: DepsMut,
    info: MessageInfo,
    order_id: String,
    price: Decimal,
) -> StdResult<Response> {
    // Load the order bucket for the price point from the orderbook
    let mut bucket = ORDER_BOOK.load(deps.storage, &format!("{}", price))?;

    // Check if the order exists and the sender is the owner
    match bucket.bids.iter().find(|order| order.id == order_id) {
        Some(order) if order.orderer == info.sender => {
            bucket.remove_order(&order_id)?;
            ORDER_BOOK.save(deps.storage, &format!("{}", price), &bucket)?;

            // Also remove the order from the ORDERS map
            ORDERS.remove(deps.storage, &order_id);

            Ok(Response::new().add_attribute("action", "cancel_bid").add_attribute("order_id", order_id))
        },
        _ => Err(StdError::generic_err("Order does not exist or the sender is not the owner")),
    }
}

pub fn cancel_ask(
    deps: DepsMut,
    info: MessageInfo,
    order_id: String,
    price: Decimal,
) -> StdResult<Response> {
    // Load the order bucket for the price point from the orderbook
    let mut bucket = ORDER_BOOK.load(deps.storage, &format!("{}", price))?;

    // Check if the order exists and the sender is the owner
    match bucket.asks.iter().find(|order| order.id == order_id) {
        Some(order) if order.orderer == info.sender => {
            bucket.remove_order(&order_id)?;
            ORDER_BOOK.save(deps.storage, &format!("{}", price), &bucket)?;
            
            // Also remove the order from the ORDERS map
            ORDERS.remove(deps.storage, &order_id);

            Ok(Response::new().add_attribute("action", "cancel_ask").add_attribute("order_id", order_id))
        },
        _ => Err(StdError::generic_err("Order does not exist or the sender is not the owner")),
    }
}

pub fn update_bid_order(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String,
    new_quantity: Uint128,
) -> StdResult<Response> {
    // Load state
    let state = STATE.load(deps.storage)?;

    // Get the order by its ID
    let mut order = ORDERS.load(deps.storage, &id)?;

    // Ensure the sender is the order's owner
    if order.orderer != info.sender {
        return Err(StdError::generic_err("Sender must be the order's owner"));
    }

    // Ensure the new quantity is lower than the current quantity
    if new_quantity > order.quantity {
        return Err(StdError::generic_err("New quantity must be lower than the current quantity"));
    }

    // Get the order bucket using the order's price
    let mut bucket = ORDER_BOOK.load(deps.storage, &order.price.to_string())?;

    // Find the order in the bucket and update its quantity
    for bid in &mut bucket.bids {
        if bid.id == id {
            bid.quantity = new_quantity;
            break;
        }
    }

    // Save the updated order and bucket
    order.quantity = new_quantity;
    ORDERS.save(deps.storage, &id, &order)?;
    ORDER_BOOK.save(deps.storage, &order.price.to_string(), &bucket)?;

    // Return excess tokens to the owner
    let excess_amount = order.quantity.checked_sub(new_quantity)?;
    let transfer_msg = Cw20ExecuteMsg::Transfer {
        recipient: info.sender.to_string(),
        amount: excess_amount,
    };
    let cosmos_msg = WasmMsg::Execute {
        contract_addr: state.usdc_contract.to_string(),
        msg: to_binary(&transfer_msg)?,
        funds: vec![],
    };
    let cosmos_response = Response::new()
        .add_message(cosmos_msg)
        .add_attribute("action", "update_bid_order")
        .add_attribute("order_id", &id)
        .add_attribute("new_quantity", &new_quantity.to_string());
    
    Ok(cosmos_response)
}


pub fn update_ask_order(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String,
    new_quantity: Uint128,
) -> StdResult<Response> {
    // Load state
    let state = STATE.load(deps.storage)?;

    // Get the order by its ID
    let mut order = ORDERS.load(deps.storage, &id)?;

    // Ensure the sender is the order's owner
    if order.orderer != info.sender {
        return Err(StdError::generic_err("Sender must be the order's owner"));
    }

    // Ensure the new quantity is lower than the current quantity
    if new_quantity > order.quantity {
        return Err(StdError::generic_err("New quantity must be lower than the current quantity"));
    }

    // Get the order bucket using the order's price
    let mut bucket = ORDER_BOOK.load(deps.storage, &order.price.to_string())?;

    // Find the order in the bucket and update its quantity
    for ask in &mut bucket.asks {
        if ask.id == id {
            ask.quantity = new_quantity;
            break;
        }
    }

    // Save the updated order and bucket
    order.quantity = new_quantity;
    ORDERS.save(deps.storage, &id, &order)?;
    ORDER_BOOK.save(deps.storage, &order.price.to_string(), &bucket)?;

    // Return excess tokens to the owner
    let excess_amount = order.quantity.checked_sub(new_quantity)?;
    let transfer_msg = Cw20ExecuteMsg::Transfer {
        recipient: info.sender.to_string(),
        amount: excess_amount,
    };
    let cosmos_msg = WasmMsg::Execute {
        contract_addr: state.usdc_contract.to_string(),
        msg: to_binary(&transfer_msg)?,
        funds: vec![],
    };
    let cosmos_response = Response::new()
        .add_message(cosmos_msg)
        .add_attribute("action", "update_ask_order")
        .add_attribute("order_id", &id)
        .add_attribute("new_quantity", &new_quantity.to_string());
    
    Ok(cosmos_response)
}

pub fn execute_create(
    deps: DepsMut,
    msg: CreateMsg,
    balance: Balance,
    sender: &Addr,
) -> Result<Response, StdError> {
    if balance.is_empty() {
        return Err(StdError::generic_err("Balance cannot be empty"));
    }

    let mut cw20_whitelist = msg.addr_whitelist(deps.api)?;

    let escrow_balance = match balance {
        Balance::Native(balance) => GenericBalance {
            native: balance.0,
            cw20: vec![],
        },
        Balance::Cw20(token) => {
            // make sure the token sent is on the whitelist by default
            if !cw20_whitelist.iter().any(|t| t == &token.address) {
                cw20_whitelist.push(token.address.clone())
            }
            GenericBalance {
                native: vec![],
                cw20: vec![token],
            }
        }
    };

    let recipient: Option<Addr> = msg
        .recipient
        .and_then(|addr| deps.api.addr_validate(&addr).ok());

    let escrow = Escrow {
        arbiter: deps.api.addr_validate(&msg.arbiter)?,
        recipient,
        source: sender.clone(),
        title: msg.title,
        description: msg.description,
        end_height: msg.end_height,
        end_time: msg.end_time,
        balance: escrow_balance,
        cw20_whitelist,
    };

    // try to store it, fail if the id was already in use
    ESCROWS.update(deps.storage, &msg.id, |existing| match existing {
        None => Ok(escrow),
        Some(_) => Err(StdError::generic_err("ID is already in use")),
    })?;

    let res = Response::new().add_attributes(vec![("action", "create"), ("id", msg.id.as_str())]);
    Ok(res)
}


pub fn execute_set_recipient(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    id: String,
    recipient: String,
) -> Result<Response, StdError> {
    let mut escrow = ESCROWS.load(deps.storage, &id)?;
    if info.sender != escrow.arbiter {
        return Err(StdError::generic_err("Unauthorized access"));
    }

    let recipient = deps.api.addr_validate(recipient.as_str())?;
    escrow.recipient = Some(recipient.clone());
    ESCROWS.save(deps.storage, &id, &escrow)?;

    Ok(Response::new().add_attributes(vec![
        ("action", "set_recipient"),
        ("id", id.as_str()),
        ("recipient", recipient.as_str()),
    ]))
}
pub fn execute_top_up(
    deps: DepsMut,
    id: String,
    balance: Balance,
) -> Result<Response, StdError> {
    if balance.is_empty() {
        return Err(StdError::generic_err("Balance cannot be empty"));
    }
    // this fails is no escrow there
    let mut escrow = ESCROWS.load(deps.storage, &id)?;

    if let Balance::Cw20(token) = &balance {
        // ensure the token is on the whitelist
        if !escrow.cw20_whitelist.iter().any(|t| t == &token.address) {
            return Err(StdError::generic_err("Token is not in the whitelist"));
        }
    };

    escrow.balance.add_tokens(balance);

    // and save
    ESCROWS.save(deps.storage, &id, &escrow)?;

    let res = Response::new().add_attributes(vec![("action", "top_up"), ("id", id.as_str())]);
    Ok(res)
}

pub fn execute_approve(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String,
) -> Result<Response, StdError> {
    let escrow = ESCROWS.load(deps.storage, &id)?;

    if info.sender != escrow.arbiter {
        return Err(StdError::generic_err("Unauthorized access"));
    }

    if escrow.is_expired(&env) {
        return Err(StdError::generic_err("The escrow has expired"));
    }


    let recipient = escrow.recipient.ok_or_else(|| StdError::generic_err("Recipient not set"))?;

    // we delete the escrow
    ESCROWS.remove(deps.storage, &id);

    // send all tokens out
    let messages: Vec<SubMsg> = send_tokens(&recipient, &escrow.balance)?;

    Ok(Response::new()
        .add_attribute("action", "approve")
        .add_attribute("id", id)
        .add_attribute("to", recipient)
        .add_submessages(messages))
}

pub fn execute_refund(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String,
) -> Result<Response, StdError> {
    let escrow = ESCROWS.load(deps.storage, &id)?;

    if !escrow.is_expired(&env) && info.sender != escrow.arbiter {
        return Err(StdError::generic_err("Unauthorized access"));
    }
    else {
        // we delete the escrow
        ESCROWS.remove(deps.storage, &id);

        // send all tokens out
        let messages = send_tokens(&escrow.source, &escrow.balance)?;

        Ok(Response::new()
            .add_attribute("action", "refund")
            .add_attribute("id", id)
            .add_attribute("to", escrow.source)
            .add_submessages(messages))
    }
}

fn send_tokens(to: &Addr, balance: &GenericBalance) -> StdResult<Vec<SubMsg>> {
    let native_balance = &balance.native;
    let mut msgs: Vec<SubMsg> = if native_balance.is_empty() {
        vec![]
    } else {
        vec![SubMsg::new(BankMsg::Send {
            to_address: to.into(),
            amount: native_balance.to_vec(),
        })]
    };

    let cw20_balance = &balance.cw20;
    let cw20_msgs: StdResult<Vec<_>> = cw20_balance
        .iter()
        .map(|c| {
            let msg = Cw20ExecuteMsg::Transfer {
                recipient: to.into(),
                amount: c.amount,
            };
            let exec = SubMsg::new(WasmMsg::Execute {
                contract_addr: c.address.to_string(),
                msg: to_binary(&msg)?,
                funds: vec![],
            });
            Ok(exec)
        })
        .collect();
    msgs.append(&mut cw20_msgs?);
    Ok(msgs)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::List {} => to_binary(&query_list(deps)?),
        QueryMsg::Details { id } => to_binary(&query_details(deps, id)?),
        QueryMsg::GetOrderbook {} => to_binary(&query_orderbook(deps)?),
        QueryMsg::GetUserOrders { user } => to_binary(&query_user_orders(deps, user)?),
        QueryMsg::GetState {} => to_binary(&query_state(deps)?)
    }
}

pub fn query_state(deps: Deps) -> StdResult<StateResponse> {
    // Load state from storage
    let state = STATE.load(deps.storage)?;

    // Put it in a vector, since your response expects a vector
    let State: Vec<State> = vec![state];

    Ok(StateResponse { State })
}


pub fn query_orderbook(deps: Deps) -> StdResult<OrderbookResponse> {
    // Initialize an empty vector to hold the results
    let mut order_buckets: Vec<OrderBucket> = vec![];

    // Iterate through the order book
    for result in ORDER_BOOK.range(deps.storage, None, None, cosmwasm_std::Order::Ascending) {
        let (_price, bucket) = result?;
        order_buckets.push(bucket);
    }

    Ok(OrderbookResponse { order_bucket: order_buckets })
}



pub fn query_user_orders(deps: Deps, user: Addr) -> StdResult<UserOrdersResponse> {
    // Initialize an empty vector to hold the results
    let mut user_orders: Vec<Order> = vec![];

    // Iterate through the order book
    for result in ORDER_BOOK.range(deps.storage, None, None, cosmwasm_std::Order::Ascending) {
        let (_price, bucket) = result?;
        // Check each order in the bids and asks
        for order in bucket.bids.iter().chain(bucket.asks.iter()) {
            if order.owner == user {
                user_orders.push(order.clone());
            }
        }
    }

    Ok(UserOrdersResponse { orders: user_orders })
}





fn query_details(deps: Deps, id: String) -> StdResult<DetailsResponse> {
    let escrow = ESCROWS.load(deps.storage, &id)?;

    let cw20_whitelist = escrow.human_whitelist();

    // transform tokens
    let native_balance = escrow.balance.native;

    let cw20_balance: StdResult<Vec<_>> = escrow
        .balance
        .cw20
        .into_iter()
        .map(|token| {
            Ok(Cw20Coin {
                address: token.address.into(),
                amount: token.amount,
            })
        })
        .collect();

    let recipient = escrow.recipient.map(|addr| addr.into_string());

    let details = DetailsResponse {
        id,
        arbiter: escrow.arbiter.into(),
        recipient,
        source: escrow.source.into(),
        title: escrow.title,
        description: escrow.description,
        end_height: escrow.end_height,
        end_time: escrow.end_time,
        native_balance,
        cw20_balance: cw20_balance?,
        cw20_whitelist,
    };
    Ok(details)
}

fn query_list(deps: Deps) -> StdResult<ListResponse> {
    Ok(ListResponse {
        escrows: all_escrow_ids(deps.storage)?,
    })
}

pub fn update_order_bucket(
    deps: DepsMut,
    price: String,
    order: Order,
    order_type: OrderType
) -> StdResult<()> {
    // Try to load the bucket for the given price
    match ORDER_BOOK.load(deps.storage, &price) {
        // If it exists, update it
        Ok(mut bucket) => {
            bucket.add_order(order, order_type);
            ORDER_BOOK.save(deps.storage, &price, &bucket)
        },
        // If it doesn't exist, return an error
        Err(_) => Err(StdError::generic_err("No order bucket exists for the given price")),
    }
}

pub fn generate_order_id(deps: &mut DepsMut<'_>) -> StdResult<String> {
    // Load the state.
    let mut state = STATE.load(deps.storage)?;

    // Increment the max_order_id.
    state.max_order_id += 1;

    // Save the new state.
    STATE.save(deps.storage, &state)?;

    // Return the new order ID as a string.
    Ok(format!("{}", state.max_order_id))
}


#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, Addr, CosmosMsg, Decimal, Uint128, WasmMsg};

    #[test]
    fn test_execute_receive() {
        let mut deps = mock_dependencies();

        // Initialize the state
        let usdc_contract = "usdc_contract".to_string();
        let fyusdc_contract = "fyusdc_contract".to_string();

        let state = State {
            fyusdc_contract: Addr::unchecked(fyusdc_contract.clone()),
            usdc_contract: Addr::unchecked(usdc_contract.clone()),
            max_order_id: 0,
        };

        STATE.save(&mut deps.storage, &state).unwrap();

        let env = mock_env();
        let info = mock_info(&fyusdc_contract, &coins(250, "usdc"));

        let msg = Cw20ReceiveMsg {
            sender: info.sender.clone().into_string(),
            amount: Uint128::new(250),
            msg: to_binary(&ReceiveMsg::CreateAsk { 
                quantity: Uint128::new(500), 
                price: Decimal::percent(50) 
            }).unwrap(),
        };

        // Execute the contract
        let _res = execute_receive(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
        
        // Let's check if the sender is being recognized correctly
        let updated_state = STATE.load(deps.as_ref().storage).unwrap();
        assert_eq!(updated_state.fyusdc_contract, info.sender);
        assert_eq!(updated_state.usdc_contract, Addr::unchecked(usdc_contract));

        // Now let's attempt to call `execute_receive` with a different sender
        let different_sender_info = mock_info("another_contract", &coins(250, "usdc"));
        let different_sender_msg = Cw20ReceiveMsg {
            sender: different_sender_info.sender.clone().into_string(),
            amount: Uint128::new(250),
            msg: to_binary(&ReceiveMsg::CreateAsk { 
                quantity: Uint128::new(500), 
                price: Decimal::percent(50) 
            }).unwrap(),
        };

        let different_sender_res = execute_receive(deps.as_mut(), env, different_sender_info, different_sender_msg);
        assert!(different_sender_res.is_err(), "Should fail due to invalid sender");
    }
}



