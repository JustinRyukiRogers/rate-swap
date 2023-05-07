use cosmwasm_std::{
    to_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Uint128,
};

use std::str::FromStr;
use std::convert::TryInto;
use cosmwasm_storage::{singleton, Singleton};
use cosmwasm_storage::ReadonlySingleton;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use osmosis_std::types::osmosis::gamm::v1beta1::{GammQuerier, PoolAsset};


// Price oracle contract state
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub atom_osmo_pool_id: u64,
    pub usdc_osmo_pool_id: u64,
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
    pub atom_osmo_pool_id: u64,
    pub usdc_osmo_pool_id: u64,
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
        atom_osmo_pool_id: msg.atom_osmo_pool_id,
        usdc_osmo_pool_id: msg.usdc_osmo_pool_id,
    };
    config(deps.storage).save(&state)?;

    Ok(Response::new())
}

fn query_pool_assets_and_weights(
    deps: &Deps,
    pool_id: u64,
) -> StdResult<Vec<PoolAsset>> {
    let pool: osmosis_std::types::osmosis::gamm::v1beta1::Pool = query_pool(deps, pool_id)?;
    Ok(pool.pool_assets)
}

fn query_pool(
    deps: &Deps,
    pool_id: u64,
) -> StdResult<osmosis_std::types::osmosis::gamm::v1beta1::Pool> {
    let res = GammQuerier::new(&deps.querier).pool(pool_id)?;
    res.pool
        .ok_or_else(|| StdError::NotFound {
            kind: "pool".to_string(),
        })?
        .try_into() // convert `Any` to `osmosis_std::types::osmosis::gamm::v1beta1::Pool`
        .map_err(|e| StdError::generic_err(format!("Failed to parse Pool: {}", e)))
}


fn query_asset_relative_price(
    deps: &Deps,
    pool_id: u64,
    asset_index1: usize,
    asset_index2: usize,
) -> StdResult<Uint128> {
    let pool_assets = query_pool_assets_and_weights(deps, pool_id)?;

    if asset_index1 >= pool_assets.len() || asset_index2 >= pool_assets.len() {
        return Err(StdError::GenericErr {
            msg: "Asset index out of range".to_string(),
        });
    }

    let asset1 = &pool_assets[asset_index1];
    let asset2 = &pool_assets[asset_index2];

    let asset1_amount = Uint128::from_str(&asset1.token.as_ref().ok_or_else(|| StdError::NotFound {
        kind: "asset1_token".to_string(),
    })?.amount)?;

    let asset2_amount = Uint128::from_str(&asset2.token.as_ref().ok_or_else(|| StdError::NotFound {
        kind: "asset2_token".to_string(),
    })?.amount)?;

    let asset1_weight = Decimal::from_str(&asset1.weight)?;
    let asset2_weight = Decimal::from_str(&asset2.weight)?;

    // Calculate relative_price
    let relative_price = Decimal::from_ratio(asset1_amount * asset2_weight, asset1_weight * asset2_amount);

    // Scale relative_price by 10^18 to preserve 18 decimal places of precision
    let scaled_relative_price = relative_price * Uint128::new(10u128.pow(18));

    // Convert scaled_relative_price to Uint128
    Ok(scaled_relative_price)

}



pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPrices => to_binary(&query_prices(&deps)?),
    }
}

fn query_prices(deps: &Deps) -> StdResult<PricesResponse> {
    let state = config_read(deps.storage).load()?;

    // Calculate ATOM/OSMO price
    let atom_osmo_price = query_asset_relative_price(deps, state.atom_osmo_pool_id, 0, 1)?;

    // Calculate USDC/OSMO price
    let usdc_osmo_price = query_asset_relative_price(deps, state.usdc_osmo_pool_id, 0, 1)?;

    // Calculate ATOM/USDC price
    let atom_usdc_price = atom_osmo_price * usdc_osmo_price;

    Ok(PricesResponse {
        atom_price: Decimal::from_ratio(atom_usdc_price, Uint128::from(1u128)),
        usdc_price: Decimal::one(),
    })
}

