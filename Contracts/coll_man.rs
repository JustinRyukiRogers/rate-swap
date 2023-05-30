use schemars::JsonSchema;
use cosmwasm_std::Timestamp;
use cosmwasm_std::{to_binary, WasmMsg};
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    Addr, attr, BankMsg, Binary, coins, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Storage, Uint128, WasmQuery, from_binary, to_binary,
};
use cosmwasm_std::deps::{querier, Api, ContractInfo, OwnedDeps, QuerierWrapper, Storage};



// Added cw_storage_plus import
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_storage_plus::Item;
use cw_storage_plus::Map;
use cw0::Expiration;

// State of the contract
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub contract_owner: String,
    pub authorized_checker: Addr,
    pub liquidation_deadline: Expiration,
    pub liquidator: Addr,
    pub order_manager_contract: Addr,
    pub liquidation_threshold: u64
    pub liquidation_penalty: u64
    pub fyusdc_contract: Addr,
    pub rsp_contract: Addr,

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
            return Err(StdError::unauthorized());  
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PricesResponse {
    pub usdc_price: Decimal,
    pub atom_price: Decimal,
}


// Added constant for state storage
const STATE: Item<State> = Item::new("state");
const COLLATERALS: Map<&[u8], Uint128> = Map::new("collaterals");
const LOANS: Map<&[u8], Uint128> = Map::new("loans");
const CONTRACT_USDC_BALANCE: Item<Uint128> = Item::new("contract_usdc_balance");

//Storage collateral
fn read_collateral_balance(storage: &dyn Storage, address: &Addr) -> StdResult<Uint128> {
    COLLATERALS.load(storage, address.as_bytes())
}

fn save_collateral_balance(storage: &mut dyn Storage, address: &Addr, amount: Uint128) -> StdResult<()> {
    COLLATERALS.save(storage, address.as_bytes(), &amount)
}

//Storage loan
fn read_loan_balance(storage: &dyn Storage, address: &Addr) -> StdResult<Uint128> {
    LOANS.load(storage, address.as_bytes())
}

fn save_loan_balance(storage: &mut dyn Storage, address: &Addr, amount: Uint128) -> StdResult<()> {
    LOANS.save(storage, address.as_bytes(), &amount)
}

// Storage USDC balance
fn read_usdc_balance(storage: &dyn Storage) -> StdResult<Uint128> {
    CONTRACT_USDC_BALANCE.load(storage)
}

fn save_usdc_balance(storage: &mut dyn Storage, amount: Uint128) -> StdResult<()> {
    CONTRACT_USDC_BALANCE.save(storage, &amount)
}

// Load the state from storage
fn load_state(storage: &dyn Storage) -> StdResult<State> {
    STATE.load(storage)
}

// Save the state in storage
fn save_state(storage: &mut dyn Storage, state: &State) -> StdResult<()> {
    STATE.save(storage, state)
}


// Constants for contract addresses storage
const CONTRACT_FYUSDC: Item<CanonicalAddr> = Item::new("contract_fyusdc");
const CONTRACT_ORDER_MANAGER: Item<CanonicalAddr> = Item::new("contract_order_manager");

// InstantiateMsg is used when instantiating the contract
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub authorized_checker: Addr,
    pub liquidation_deadline: Expiration,
    pub liquidator: Addr,
    pub order_manager_contract: Addr,
    pub fyusdc_contract: Addr,
    pub liquidation_threshold: u64,
    pub liquidation_penalty: u64,
    pub rsp_contract: Addr
}


// ExecuteMsg contains the contract's messages
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    DepositCollateral { amount: Uint128 },
    WithdrawCollateral { amount: Uint128 },
    Borrow { amount: Uint128 },
    RepayLoan { amount: Uint128 },
    LiquidateCollateral { borrower: Addr, amount: Uint128 }, 
    WithdrawUSDC {amount: Uint128},
}

// QueryMsg contains the contract's query messages
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    GetCollateral { address: Addr }, 
    GetLoan { address: Addr }, 
}

// Update HandleMsg
pub enum HandleMsg {
    WithdrawUSDC {
        amount: Uint128,
    },
}


pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let state = State {
        contract_owner: info.sender,
        authorized_checker: deps.api.addr_validate(&msg.authorized_checker)?,
        liquidation_deadline: msg.liquidation_deadline,
        liquidator: deps.api.addr_validate(&msg.liquidator)?,
        order_manager_contract: deps.api.addr_validate(&msg.order_manager_contract)?,
        fyusdc_contract: deps.api.addr_validate(&msg.fyusdc_contract)?,
        liquidation_threshold: msg.liquidation_threshold,
        liquidation_penalty: msg.liquidation_penalty,
        contract_usdc_balance: Uint128::zero(),
        rsp_contract: deps.api.addr_validate(&msg.rsp_contract)?, 

    };

    // Store the state
    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

pub fn handle(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: HandleMsg,
) -> Result<Response, ContractError> {
    match msg {
        HandleMsg::WithdrawUSDC { amount } => try_withdraw_usdc(deps, env, info, amount),
    }
}


pub fn query_prices(deps: Deps) -> StdResult<PricesResponse> {
    // Replace the below with the actual address of the price oracle contract
    let oracle_address = Addr::unchecked("price_oracle_contract_address");

    let res: StdResult<PricesResponse> = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: oracle_address.to_string(),
        msg: to_binary(&PricesQueryMsg {})?,
    }));

    res
}

pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, StdError> {
    match msg {
        QueryMsg::GetCollateral { address } => to_binary(&query_collateral(deps, address)?),
        QueryMsg::GetLoan { address } => to_binary(&query_loan(deps, address)?),                                                                                                                                                                                            
    }
}

fn query_collateral(deps: Deps, address: Addr) -> StdResult<Uint128> {
    let collateral = read_collateral_balance(deps.storage, &address)?;
    Ok(collateral)
}

fn query_loan(deps: Deps, address: Addr) -> StdResult<Uint128> {
    let loan = read_loan_balance(deps.storage, &address)?;
    Ok(loan)
}

fn deposit_collateral(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, StdError> {
    // Load user's collateral from storage
    let atoken_hash = "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2".to_string();

    // Check if sent tokens are ATOM tokens with the specified hash
    if let Some(sent_token) = info.sent_funds.iter().find(|coin| coin.denom == atoken_hash) {
        if sent_token.amount != amount {
            return Err(StdError::generic_err("Amount mismatch"));
        }
    } else {
        return Err(StdError::generic_err("Only ATOM tokens can be deposited"));
    }

    let mut collateral = read_collateral_balance(deps.storage, &info.sender)?;

    // Add the deposited amount to the user's collateral
    collateral += amount;

    // Save the updated collateral amount to storage
    save_collateral_balance(deps.storage, &info.sender, collateral)?;

    // Return a successful response
    Ok(Response::new().add_attributes(vec![
        Attribute::new("action", "deposit_collateral"),
        Attribute::new("sender", &info.sender.to_string()),
        Attribute::new("collateral_amount", &amount.to_string()),
    ]))
}


pub fn withdraw_collateral(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, StdError> {
    let atoken_hash = "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2".to_string();
    let state = state(deps.storage).load()?;
    let liquidation_threshold = state.liquidation_threshold;

    let balances = balances_read(deps.storage);
    let current_collateral = balances.load(info.sender.as_bytes())?;

    // Query prices for USDC and ATOM
    let prices_response = query_prices(deps.as_ref())?;

    // Calculate the new collateral balance after withdrawal
    let new_collateral = current_collateral.checked_sub(amount)?;
    let new_collateral_usd = new_collateral * prices_response.atom_price;

    // Retrieve the borrower's loan balance
    let loans = loans_read(deps.storage);
    let loan_balance = loans.load(info.sender.as_bytes())?;
    let loan_balance_usd = loan_balance * prices_response.usdc_price;

    // Calculate the new collateralization ratio
    let new_collateralization_ratio = if loan_balance == Uint128::zero() {
        Decimal::one()
    } else {
        Decimal::from_ratio(new_collateral_usd, loan_balance_usd)
    };

    // Check if the new collateralization ratio is above the liquidation threshold
    if new_collateralization_ratio < liquidation_threshold {
        return Err(StdError::generic_err("Withdrawal would trigger liquidation"));
    }

    let withdraw_msg = CosmosMsg::Bank(BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: coins(amount.u128(), &atoken_hash),
    });

    Ok(Response::new().add_attributes(vec![
        Attribute::new("action", "withdraw_collateral"),
        Attribute::new("sender", info.sender),
        Attribute::new("collateral_amount", amount),
    ])

}


fn borrow(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, StdError> {
    // Load user's collateral and loan from storage
    let mut collateral = read_collateral_balance(deps.storage, &info.sender)?;
    let mut loan = read_collateral_balance(deps.storage, &info.sender)?;

    // Get the order_manager contract address
    let order_manager_address = Addr::from_str(&order_manager_contract(deps.storage)?)?;

    // Load the state
    let state = STATE.load(deps.storage)?;

    // Get the first ask price from the order_manager contract
    let query_msg = QueryMsg::GetAskOrderbook {};
    let ask_orderbook_binary: Binary = deps.querier.query_wasm_smart(
        &state.order_manager_contract,
        &query_msg
    )?;
    let ask_orderbook: Vec<Order> = from_binary(&ask_orderbook_binary)?;
    let first_ask_price = ask_orderbook.first().map(|order| order.price.clone());

    // Query prices for USDC and ATOM
    let prices_response = query_prices(deps.as_ref())?;
    let fyUSDC_USD = first_ask_price * prices_response.usdc_price

    // Convert loan balance and collateral balance to USD value
    let loan_balance_usd = loan_balance * fyUSDC_USD;
    let collateral_balance_usd = collateral_balance * prices_response.atom_price;

    // Calculate the maximum amount the user can borrow
    let max_borrow = collateral_balance_usd / state.liquidation_threshold;
    let max_fyusdc_borrowable = max_borrow * fyUSDC_USD

    // Check if the user can borrow the requested amount
    if loan_balance_usd + ( amount * fyUSDC_USD ) > max_borrow {
        return Err(StdError::generic_err("Insufficient collateral to borrow this amount"));
    }

    // Add the borrowed amount to the user's loan
    loan += amount;

    //Mint borrower amount number of fyUSDC * fyUSDC price, which we need to get from the order book
    // Mint the amount of fyUSDC tokens to the user
    let fyusdc_contract_address = deps.api.addr_humanize(&fyusdc_contract(deps.storage)?)?;
    let cw20_msg = Cw20ExecuteMsg::Mint {
        recipient: info.sender.to_string(),
        amount,
    };
    let cosmos_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: fyusdc_contract_address.to_string(),
        msg: to_binary(&cw20_msg)?,
        funds: vec![],
    });
    // Save the updated loan amount to storage
    save_loan_balance(deps.storage, &info.sender, loan)?;


    // Return a successful response
    Ok(Response::new()
        .add_message(cosmos_msg)
        .add_attribute("action", "borrow"))
}

fn repay_loan(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, StdError> {
    // Load user's loan from storage
   let mut loan = read_collateral_balance(deps.storage, &info.sender)?;

       // Check if the caller is the borrower
    if info.sender != borrower_addr {
        return Err(StdError::generic_err("Unauthorized: only the borrower can repay the loan"));
    }

    // Check if the user has a loan to repay
    if loan.is_zero() {
        return Err(StdError::generic_err("No outstanding loan to repay"));
    }

    // Use the provided USDC token hash
    let usdc_contract_hash = "D189335C6E4A68B513C10AB227BF1C1D38C74676";
    let usdc_contract_address = Addr::unchecked(format!("{}@{}", usdc_contract_hash, usdc_contract_address));

    // Check if the user has enough USDC balance
    // (assuming a `balance` query in the USDC token contract)
    let usdc_balance: Uint128 = deps.querier.query_balance(&env.contract.address, usdc_contract_address.clone())?;

    if usdc_balance < amount {
        return Err(StdError::generic_err("Insufficient USDC balance"));
    }

    // Transfer USDC from the user to the contract (assuming a `send` message in the USDC token contract)
    let transfer_msg = BankMsg::Send {
        to_address: env.contract.address.clone().into(),
        amount: coins(amount.u128(), "uusd"),
    };
    let transfer_response = deps.querier.send_msg(BankMsg::from(transfer_msg.into()), vec![])?;

    // Subtract the repaid amount from the user's loan
    if amount >= loan {
        // If the repaid amount is greater or equal to the outstanding loan, set the loan to zero
        loan = Uint128::zero();
    } else {
        // Otherwise, subtract the repaid amount from the loan
        loan -= amount;
    }

     // Save the updated loan amount to storage
    save_loan_balance(deps.storage, &info.sender, loan)?;

    //Save the repaid amount in the contract's storage
    let contract_usdc_balance = read_usdc_balance(deps.storage)?;
    save_usdc_balance(deps.storage, contract_usdc_balance + amount)?;

    Ok(Response::new()
        .add_attribute("action", "repay_loan")
}

        ExecuteMsg::Liquidate { borrower } => liquidate_collateral(deps, env, info, borrower)
pub fn liquidate_collateral(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    borrower: Addr,
) -> Result<Response, StdError> {

    let state = STATE.load(deps.storage)?;
    let liquidator_contract_address = state.liquidator;


    // Load loan and collateral balances
    let loan = LOANS.load(deps.storage, &borrower)?;
    let collateral_balance = COLLATERAL.load(deps.storage, &borrower)?;
        
    //Liquidation amount
    let amount = loan_balance * state.liquidation_penalty;

    // Query prices for USDC and ATOM
    let prices_response = query_prices(deps.as_ref())?;

    // Convert loan balance and collateral balance to USD value
    let loan_balance_usd = loan_balance * prices_response.usdc;
    let collateral_balance_usd = collateral_balance * prices_response.atom;


    // Calculate new collateral balance
    let new_collateral = collateral_balance.checked_sub(amount)?;

    // Calculate the new collateralization ratio
    let collateralization_ratio = if loan_balance == Uint128::zero() {
        Decimal::one()
    } else {
        Decimal::from_ratio(collateral_balance_usd, loan_balance)
    };

    // Check if the new collateralization ratio is below the liquidation threshold
    if collateralization_ratio >= state.liquidation_threshold && env.block.height <= state.liquidation_deadline.at_height() {
        return Err(StdError::generic_err("LiquidationThresholdNotReached");
    )
    }

    // Update the borrower's collateral balance
    save_collateral_balance(deps.storage, &borrower_addr, new_collateral)?;

    // Transfer the liquidated collateral directly to the liquidator
    let transfer_msg = ExecuteMsg::Transfer {
        recipient: liquidator_contract_address.clone(), // Transfer to the liquidator's address
        amount,
    };
    let transfer_response = deps.querier.execute_wasm_smart(
        &collateral_contract_address, // Use the collateral contract's address for the transfer
        &to_binary(&transfer_msg)?,
        None,
    )?;

    // NEW: Save the received USDC amount in the contract's storage
    let contract_usdc_balance = read_usdc_balance(deps.storage)?;
    save_usdc_balance(deps.storage, contract_usdc_balance + usdc_amount)?;

    
    Ok(Response::new()
        .add_attributes(vec![
            Attribute::new("action", "liquidate_collateral"),
            Attribute::new("borrower", borrower),
            Attribute::new("liquidated_collateral_amount", amount.to_string()),
            Attribute::new("received_usdc_amount", usdc_amount.to_string()),
    ]))

}

fn try_withdraw_usdc(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_amount: Uint128,
) -> StdResult<Response> {
    // Verify if the current block time is past the liquidation deadline
    let state = state.load(deps.storage)?;
    if env.block.time < state.liquidation_deadline {
        return Err(StdError::generic_err("Withdrawal is not allowed before the liquidation deadline"));
    }
    
    // Get the fyUSDC contract address from storage
    let fyusdc_contract_address = read_fyusdc_contract_address(deps.storage)?;

    // Verify that the sent tokens are from the fyUSDC contract
    if info.sender != fyusdc_contract_address {
        return Err(StdError::generic_err("Only fyUSDC tokens are accepted for withdrawal"));
    }

    // Check the contract's USDC balance to ensure it has enough tokens to cover the withdrawal
    let usdc_balance = read_usdc_balance(deps.storage)?;
    if usdc_balance < token_amount {
        return Err(StdError::generic_err("Not enough USDC tokens in the contract to cover the withdrawal"));
    }

    // Update the contract's USDC balance
    save_usdc_balance(deps.storage, usdc_balance - token_amount)?;


    // Send USDC tokens to the user
    let usdc_contract_address = deps.api.addr_validate("ibc/D189335C6E4A68B513C10AB227BF1C1D38C74676")?;
    let cw20_msg = Cw20ExecuteMsg::Transfer {
        recipient: info.sender.to_string(),
        amount: token_amount,
    };
    let cosmos_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: usdc_contract_address.to_string(),
        msg: to_binary(&cw20_msg)?,
        funds: vec![],
    });

    // Burn the fyUSDC tokens
    let cw20_burn_msg = Cw20ExecuteMsg::Burn {
        amount: token_amount,
    };
    let cosmos_burn_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: fyusdc_contract_address.to_string(),
        msg: to_binary(&cw20_burn_msg)?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_message(cosmos_msg)
        .add_message(cosmos_burn_msg)
        .add_attribute("action", "withdraw_usdc"))
}



fn calculate_blocks_until_deadline(current_time: u64) -> u64 {
    let deadline_time: u64 = 1_653_075_200; // June 1, 2024, in UNIX timestamp
    let seconds_in_a_block: u64 = 6; // Assuming 6 seconds per block for the CosmWasm chain
    let remaining_seconds = deadline_time.saturating_sub(current_time);
    remaining_seconds / seconds_in_a_block
}


pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, StdError> {
    match msg {
        ExecuteMsg::DepositCollateral { amount } => deposit_collateral(deps, env, info, amount),
        ExecuteMsg::WithdrawCollateral { amount } => withdraw_collateral(deps, env, info, amount),
        ExecuteMsg::Borrow { amount } => borrow(deps, env, info, amount),
        ExecuteMsg::RepayLoan { amount } => {
            repay_loan(deps, env, info, amount)
        },
        ExecuteMsg::LiquidateCollateral { borrower, amount } => {
            liquidate_collateral(deps, env, info, borrower, amount)
        },
        ExecuteMsg::WithdrawUSDC {amount} => try_withdraw_usdc(deps, env, info, amount),
    }
}