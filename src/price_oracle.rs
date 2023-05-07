use cosmwasm_std::{
    to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult,
};

use schemars::JsonSchema;
use cosmwasm_storage::{singleton, Singleton};
use cosmwasm_storage::ReadonlySingleton;
use serde::{Deserialize, Serialize};

// Price oracle contract state
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub oracle: Addr,
    pub atom_price: Decimal,
    pub usdc_price: Decimal,
}

const CONFIG_KEY: &[u8] = b"config";

pub fn config(storage: &mut dyn cosmwasm_std::Storage) -> Singleton<State> {
    singleton(storage, CONFIG_KEY)
}

pub fn config_read(storage: &dyn cosmwasm_std::Storage) -> ReadonlySingleton<State> {
    ReadonlySingleton::new(storage, CONFIG_KEY)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InstantiateMsg {
    pub oracle: Addr,
    pub atom_price: Decimal,
    pub usdc_price: Decimal,
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum QueryMsg {
    GetPrices,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PricesResponse {
    pub atom_price: Decimal,
    pub usdc_price: Decimal,
}

pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let state = State {
        oracle: msg.oracle,
        atom_price: msg.atom_price,
        usdc_price: msg.usdc_price,
    };
    config(deps.storage).save(&state)?;

    Ok(Response::new())
}

pub fn execute(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response> {
    match msg {
        ExecuteMsg::UpdateAtomPrice { new_price } => update_atom_price(deps, env, _info, new_price),
        ExecuteMsg::UpdateUsdcPrice { new_price } => update_usdc_price(deps, env, _info, new_price),
    }
}

// Functions to update prices
fn update_atom_price(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_price: Decimal,
) -> StdResult<Response> {
    let mut state = config(deps.storage).load()?;

    if info.sender != state.oracle {
        return Err(StdError::generic_err("Unauthorized"));
    }

    state.atom_price = new_price;
    config(deps.storage).save(&state)?;

    Ok(Response::new())
}

fn update_usdc_price(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_price: Decimal,
) -> StdResult<Response> {
    let mut state = config(deps.storage).load()?;

    if info.sender != state.oracle {
        return Err(StdError::generic_err("Unauthorized"));
    }

    state.usdc_price = new_price;
    config(deps.storage).save(&state)?;

}


pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPrices => to_binary(&query_prices(&deps)?),
    }
}

fn query_prices(deps: &Deps) -> StdResult<PricesResponse> {
    let state = config_read(deps.storage).load()?;
    Ok(PricesResponse {
        atom_price: state.atom_price,
        usdc_price: state.usdc_price,
    })
}


