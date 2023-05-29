#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, StdError, SubMsg, WasmMsg, Uint128, Decimal
};

use cw2::set_contract_version;
use cw20::{Balance, Cw20Coin, Cw20CoinVerified, Cw20ExecuteMsg, Cw20ReceiveMsg};
use std::collections::HashMap;

use crate::error::ContractError;
use crate::msg::{
    CreateMsg, DetailsResponse, ExecuteMsg, InstantiateMsg, ListResponse, QueryMsg, ReceiveMsg, OrderbookResponse, UserOrdersResponse
};
use crate::state::{State, Order, OrderBucket, all_escrow_ids, Escrow, GenericBalance, ESCROWS, OrderType, STATE, ORDER_BOOK};

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
        _ => todo!(),
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

    match info.sender.clone() {
        sender if sender == state.fyusdc_contract || sender == state.usdc_contract => (),
        _ => return Err(StdError::generic_err("Invalid sender")),
    }
    
    let api = deps.api;
    match msg {
        ReceiveMsg::Create(msg) => {
            execute_create(deps, msg, balance, &api.addr_validate(&wrapper.sender)?)
        }
        ReceiveMsg::TopUp { id } => execute_top_up(deps, id, balance),
        ReceiveMsg::CreateAsk { quantity, price } => create_ask(deps, env, info, wrapper, quantity, price),
        ReceiveMsg::CreateBid { quantity, price } => create_bid(deps, env, info, wrapper, quantity, price),

    }
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

pub fn create_bid(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
    price: Uint128,
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
        price,
        quantity: quantity,
    };

    // Load the order bucket for the price point from the orderbook
    let mut bucket = ORDER_BOOK.load(deps.storage, &format!("{}", price))?;
    // Add the order to the bucket and save it back to the orderbook
    bucket.add_order(order, OrderType::Bid);
    ORDER_BOOK.save(deps.storage, &format!("{}", price), &bucket)?;

    Ok(Response::new()
        .add_attribute("action", "create_bid")
        .add_attribute("order_id", order_id))
}

pub fn create_ask(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
    price: Uint128,
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
        price,
        quantity: quantity,
    };

    // Load the order bucket for the price point from the orderbook
    let mut bucket = ORDER_BOOK.load(deps.storage, &format!("{}", price))?;
    // Add the order to the bucket and save it back to the orderbook
    bucket.add_order(order, OrderType::Ask);
    ORDER_BOOK.save(deps.storage, &format!("{}", price), &bucket)?;

    Ok(Response::new()
        .add_attribute("action", "create_ask")
        .add_attribute("order_id", order_id))
}

pub fn cancel_bid(
    deps: DepsMut,
    info: MessageInfo,
    order_id: String,
    price: Uint128,
) -> StdResult<Response> {
    // Load the order bucket for the price point from the orderbook
    let mut bucket = ORDER_BOOK.load(deps.storage, &format!("{}", price))?;

    // Check if the order exists and the sender is the owner
    match bucket.bids.iter().find(|order| order.id == order_id) {
        Some(order) if order.owner == info.sender => {
            bucket.remove_order(&order_id)?;
            ORDER_BOOK.save(deps.storage, &format!("{}", price), &bucket)?;
            Ok(Response::new().add_attribute("action", "cancel_bid").add_attribute("order_id", order_id))
        },
        _ => Err(StdError::generic_err("Order does not exist or the sender is not the owner")),
    }
}

pub fn cancel_ask(
    deps: DepsMut,
    info: MessageInfo,
    order_id: String,
    price: Uint128,
) -> StdResult<Response> {
    // Load the order bucket for the price point from the orderbook
    let mut bucket = ORDER_BOOK.load(deps.storage, &format!("{}", price))?;

    // Check if the order exists and the sender is the owner
    match bucket.asks.iter().find(|order| order.id == order_id) {
        Some(order) if order.owner == info.sender => {
            bucket.remove_order(&order_id)?;
            ORDER_BOOK.save(deps.storage, &format!("{}", price), &bucket)?;
            Ok(Response::new().add_attribute("action", "cancel_ask").add_attribute("order_id", order_id))
        },
        _ => Err(StdError::generic_err("Order does not exist or the sender is not the owner")),
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
    }
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
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{attr, coin, coins, CosmosMsg, StdError, Uint128};

    use crate::msg::ExecuteMsg::TopUp;

    use super::*;

    #[test]
    fn happy_path_native() {
        let mut deps = mock_dependencies();

        // instantiate an empty contract
        let instantiate_msg = InstantiateMsg {};
        let info = mock_info(&String::from("anyone"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        // create an escrow
        let create = CreateMsg {
            id: "foobar".to_string(),
            arbiter: String::from("arbitrate"),
            recipient: Some(String::from("recd")),
            title: "some_title".to_string(),
            end_time: None,
            end_height: Some(123456),
            cw20_whitelist: None,
            description: "some_description".to_string(),
        };
        let sender = String::from("source");
        let balance = coins(100, "tokens");
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::Create(create.clone());
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "create"), res.attributes[0]);

        // ensure the details is what we expect
        let details = query_details(deps.as_ref(), "foobar".to_string()).unwrap();
        assert_eq!(
            details,
            DetailsResponse {
                id: "foobar".to_string(),
                arbiter: String::from("arbitrate"),
                recipient: Some(String::from("recd")),
                source: String::from("source"),
                title: "some_title".to_string(),
                description: "some_description".to_string(),
                end_height: Some(123456),
                end_time: None,
                native_balance: balance.clone(),
                cw20_balance: vec![],
                cw20_whitelist: vec![],
            }
        );

        // approve it
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap();
        assert_eq!(1, res.messages.len());
        assert_eq!(("action", "approve"), res.attributes[0]);
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: create.recipient.unwrap(),
                amount: balance,
            }))
        );

        // second attempt fails (not found)
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::NotFound { .. })));
    }

    #[test]
    fn happy_path_cw20() {
        let mut deps = mock_dependencies();

        // instantiate an empty contract
        let instantiate_msg = InstantiateMsg {};
        let info = mock_info(&String::from("anyone"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        // create an escrow
        let create = CreateMsg {
            id: "foobar".to_string(),
            arbiter: String::from("arbitrate"),
            recipient: Some(String::from("recd")),
            title: "some_title".to_string(),
            end_time: None,
            end_height: None,
            cw20_whitelist: Some(vec![String::from("other-token")]),
            description: "some_description".to_string(),
        };
        let receive = Cw20ReceiveMsg {
            sender: String::from("source"),
            amount: Uint128::new(100),
            msg: to_binary(&ExecuteMsg::Create(create.clone())).unwrap(),
        };
        let token_contract = String::from("my-cw20-token");
        let info = mock_info(&token_contract, &[]);
        let msg = ExecuteMsg::Receive(receive.clone());
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "create"), res.attributes[0]);

        // ensure the whitelist is what we expect
        let details = query_details(deps.as_ref(), "foobar".to_string()).unwrap();
        assert_eq!(
            details,
            DetailsResponse {
                id: "foobar".to_string(),
                arbiter: String::from("arbitrate"),
                recipient: Some(String::from("recd")),
                source: String::from("source"),
                title: "some_title".to_string(),
                description: "some_description".to_string(),
                end_height: None,
                end_time: None,
                native_balance: vec![],
                cw20_balance: vec![Cw20Coin {
                    address: String::from("my-cw20-token"),
                    amount: Uint128::new(100),
                }],
                cw20_whitelist: vec![String::from("other-token"), String::from("my-cw20-token")],
            }
        );

        // approve it
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap();
        assert_eq!(1, res.messages.len());
        assert_eq!(("action", "approve"), res.attributes[0]);
        let send_msg = Cw20ExecuteMsg::Transfer {
            recipient: create.recipient.unwrap(),
            amount: receive.amount,
        };
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_contract,
                msg: to_binary(&send_msg).unwrap(),
                funds: vec![]
            }))
        );

        // second attempt fails (not found)
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::NotFound { .. })));
    }

    #[test]
    fn set_recipient_after_creation() {
        let mut deps = mock_dependencies();

        // instantiate an empty contract
        let instantiate_msg = InstantiateMsg {};
        let info = mock_info(&String::from("anyone"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        // create an escrow
        let create = CreateMsg {
            id: "foobar".to_string(),
            arbiter: String::from("arbitrate"),
            recipient: None,
            title: "some_title".to_string(),
            end_time: None,
            end_height: Some(123456),
            cw20_whitelist: None,
            description: "some_description".to_string(),
        };
        let sender = String::from("source");
        let balance = coins(100, "tokens");
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::Create(create.clone());
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "create"), res.attributes[0]);

        // ensure the details is what we expect
        let details = query_details(deps.as_ref(), "foobar".to_string()).unwrap();
        assert_eq!(
            details,
            DetailsResponse {
                id: "foobar".to_string(),
                arbiter: String::from("arbitrate"),
                recipient: None,
                source: String::from("source"),
                title: "some_title".to_string(),
                description: "some_description".to_string(),
                end_height: Some(123456),
                end_time: None,
                native_balance: balance.clone(),
                cw20_balance: vec![],
                cw20_whitelist: vec![],
            }
        );

        // approve it, should fail as we have not set recipient
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id });
        match res {
            Err(ContractError::RecipientNotSet {}) => {}
            _ => panic!("Expect recipient not set error"),
        }

        // test setting recipient not arbiter
        let msg = ExecuteMsg::SetRecipient {
            id: create.id.clone(),
            recipient: "recp".to_string(),
        };
        let info = mock_info("someoneelse", &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        match res {
            Err(ContractError::Unauthorized {}) => {}
            _ => panic!("Expect unauthorized error"),
        }

        // test setting recipient valid
        let msg = ExecuteMsg::SetRecipient {
            id: create.id.clone(),
            recipient: "recp".to_string(),
        };
        let info = mock_info(&create.arbiter, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(
            res.attributes,
            vec![
                attr("action", "set_recipient"),
                attr("id", create.id.as_str()),
                attr("recipient", "recp")
            ]
        );

        // approve it, should now work with recp
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap();
        assert_eq!(1, res.messages.len());
        assert_eq!(("action", "approve"), res.attributes[0]);
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "recp".to_string(),
                amount: balance,
            }))
        );
    }

    #[test]
    fn add_tokens_proper() {
        let mut tokens = GenericBalance::default();
        tokens.add_tokens(Balance::from(vec![coin(123, "atom"), coin(789, "eth")]));
        tokens.add_tokens(Balance::from(vec![coin(456, "atom"), coin(12, "btc")]));
        assert_eq!(
            tokens.native,
            vec![coin(579, "atom"), coin(789, "eth"), coin(12, "btc")]
        );
    }

    #[test]
    fn add_cw_tokens_proper() {
        let mut tokens = GenericBalance::default();
        let bar_token = Addr::unchecked("bar_token");
        let foo_token = Addr::unchecked("foo_token");
        tokens.add_tokens(Balance::Cw20(Cw20CoinVerified {
            address: foo_token.clone(),
            amount: Uint128::new(12345),
        }));
        tokens.add_tokens(Balance::Cw20(Cw20CoinVerified {
            address: bar_token.clone(),
            amount: Uint128::new(777),
        }));
        tokens.add_tokens(Balance::Cw20(Cw20CoinVerified {
            address: foo_token.clone(),
            amount: Uint128::new(23400),
        }));
        assert_eq!(
            tokens.cw20,
            vec![
                Cw20CoinVerified {
                    address: foo_token,
                    amount: Uint128::new(35745),
                },
                Cw20CoinVerified {
                    address: bar_token,
                    amount: Uint128::new(777),
                }
            ]
        );
    }

    #[test]
    fn top_up_mixed_tokens() {
        let mut deps = mock_dependencies();

        // instantiate an empty contract
        let instantiate_msg = InstantiateMsg {};
        let info = mock_info(&String::from("anyone"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        // only accept these tokens
        let whitelist = vec![String::from("bar_token"), String::from("foo_token")];

        // create an escrow with 2 native tokens
        let create = CreateMsg {
            id: "foobar".to_string(),
            arbiter: String::from("arbitrate"),
            recipient: Some(String::from("recd")),
            title: "some_title".to_string(),
            end_time: None,
            end_height: None,
            cw20_whitelist: Some(whitelist),
            description: "some_description".to_string(),
        };
        let sender = String::from("source");
        let balance = vec![coin(100, "fee"), coin(200, "stake")];
        let info = mock_info(&sender, &balance);
        let msg = ExecuteMsg::Create(create.clone());
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "create"), res.attributes[0]);

        // top it up with 2 more native tokens
        let extra_native = vec![coin(250, "random"), coin(300, "stake")];
        let info = mock_info(&sender, &extra_native);
        let top_up = ExecuteMsg::TopUp {
            id: create.id.clone(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, top_up).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "top_up"), res.attributes[0]);

        // top up with one foreign token
        let bar_token = String::from("bar_token");
        let base = TopUp {
            id: create.id.clone(),
        };
        let top_up = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: String::from("random"),
            amount: Uint128::new(7890),
            msg: to_binary(&base).unwrap(),
        });
        let info = mock_info(&bar_token, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, top_up).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "top_up"), res.attributes[0]);

        // top with a foreign token not on the whitelist
        // top up with one foreign token
        let baz_token = String::from("baz_token");
        let base = TopUp {
            id: create.id.clone(),
        };
        let top_up = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: String::from("random"),
            amount: Uint128::new(7890),
            msg: to_binary(&base).unwrap(),
        });
        let info = mock_info(&baz_token, &[]);
        let err = execute(deps.as_mut(), mock_env(), info, top_up).unwrap_err();
        assert_eq!(err, ContractError::NotInWhitelist {});

        // top up with second foreign token
        let foo_token = String::from("foo_token");
        let base = TopUp {
            id: create.id.clone(),
        };
        let top_up = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: String::from("random"),
            amount: Uint128::new(888),
            msg: to_binary(&base).unwrap(),
        });
        let info = mock_info(&foo_token, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, top_up).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "top_up"), res.attributes[0]);

        // approve it
        let id = create.id.clone();
        let info = mock_info(&create.arbiter, &[]);
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Approve { id }).unwrap();
        assert_eq!(("action", "approve"), res.attributes[0]);
        assert_eq!(3, res.messages.len());

        // first message releases all native coins
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: create.recipient.clone().unwrap(),
                amount: vec![coin(100, "fee"), coin(500, "stake"), coin(250, "random")],
            }))
        );

        // second one release bar cw20 token
        let send_msg = Cw20ExecuteMsg::Transfer {
            recipient: create.recipient.clone().unwrap(),
            amount: Uint128::new(7890),
        };
        assert_eq!(
            res.messages[1],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: bar_token,
                msg: to_binary(&send_msg).unwrap(),
                funds: vec![]
            }))
        );

        // third one release foo cw20 token
        let send_msg = Cw20ExecuteMsg::Transfer {
            recipient: create.recipient.unwrap(),
            amount: Uint128::new(888),
        };
        assert_eq!(
            res.messages[2],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: foo_token,
                msg: to_binary(&send_msg).unwrap(),
                funds: vec![]
            }))
        );
    }
}
