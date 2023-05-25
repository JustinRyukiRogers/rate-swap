#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128,
};
use cw20_base::contract::{
    execute_burn, execute_mint, execute_send, execute_transfer, query_balance, query_token_info,
};
use cw20_base::allowances::{
    execute_approve, execute_burn_from, execute_increase_allowance, execute_transfer_from, query_allowance,
};
use cw20_base::msg::{InstantiateMsg, ExecuteMsg, QueryMsg};
use cw20_base::state::{TokenInfo, TOKEN_INFO};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// The rest of your imports

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, StdError> {
    // Set contract version as with the CW20 crate

    // Store token info using CW20 format
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply: Uint128::zero(),
        mint: Some(MinterData {
            minter: info.sender.clone(),
            cap: None,
        }),
    };
    TOKEN_INFO.save(deps.storage, &data)?;

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
        ExecuteMsg::Transfer { recipient, amount } => {
            Ok(execute_transfer(deps, env, info, recipient.into(), amount)?)
        }
        ExecuteMsg::Burn { amount } => Ok(execute_burn(deps, env, info, amount)?),
        ExecuteMsg::Mint { recipient, amount } => Ok(execute_mint(deps, env, info, recipient.into(), amount)?),
        ExecuteMsg::Approve { spender, amount } => Ok(execute_increase_allowance(
            deps, env, info, spender.into(), amount, None,
        )?),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => Ok(execute_transfer_from(
            deps, env, info, owner.into(), recipient.into(), amount,
        )?),
        // Implement other CW20 functions as needed
        ExecuteMsg::Receive(msg) => {
            let msg = Cw20ReceiveMsg::deserialize_binary(&msg.msg)?;
            // Call the receive function in the contract you want to interact with
        }
    }
}
