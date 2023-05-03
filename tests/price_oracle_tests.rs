use cosmwasm_std::{
    testing::{mock_dependencies, mock_env, mock_info, MockApi, MockQuerier},
    Addr, Decimal, Env, MemoryStorage, OwnedDeps, StdError, from_binary,
};

mod price_oracle {
    include!("../src/price_oracle.rs");
}

use price_oracle::*;

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};

    fn setup_contract() -> (OwnedDeps<MemoryStorage, MockApi, MockQuerier>, Env, Addr) {
        let deps = mock_dependencies(&[]);
        let env = mock_env();
        let oracle = Addr::unchecked("oracle");

        let mut deps = OwnedDeps {
            storage: deps.storage,
            api: deps.api,
            querier: deps.querier,
        };

        let init_msg = InstantiateMsg {
            oracle: oracle.clone(),
            atom_price: Decimal::from_ratio(100u128, 1u128),
            usdc_price: Decimal::from_ratio(1u128, 1u128),
        };

        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), env.clone(), info, init_msg).unwrap();

        (deps, env, oracle)
    }

    #[test]
    fn test_instantiate() {
        let ( deps, _, oracle) = setup_contract();

        // Check if the state is saved correctly
        let state = config_read(deps.as_ref().storage).load().unwrap();
        assert_eq!(state.oracle, oracle);
        assert_eq!(state.atom_price, Decimal::from_ratio(100u128, 1u128));
        assert_eq!(state.usdc_price, Decimal::from_ratio(1u128, 1u128));
    }

    #[test]
    fn test_update_atom_price() {
        let (mut deps, env, oracle) = setup_contract();

        let info = mock_info(oracle.as_str(), &[]);
        let new_price = Decimal::from_ratio(200u128, 1u128);

        // Update the ATOM price
        let msg = ExecuteMsg::UpdateAtomPrice { new_price };
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Check if the ATOM price was updated
        let state = config_read(deps.as_ref().storage).load().unwrap();
        assert_eq!(state.atom_price, new_price);
    }

    #[test]
    fn test_update_usdc_price() {
        let (mut deps, env, oracle) = setup_contract();

        let info = mock_info(oracle.as_str(), &[]);
        let new_price = Decimal::from_ratio(2u128, 1u128);

        // Update the USDC price
        let msg = ExecuteMsg::UpdateUsdcPrice { new_price };
        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Check if the USDC price was updated
        let state = config_read(deps.as_ref().storage).load().unwrap();
        assert_eq!(state.usdc_price, new_price);
    }

    #[test]
    fn test_update_prices_unauthorized() {
        let (mut deps, env, _) = setup_contract();

        let info = mock_info("Unauthorized_user", &[]);
        let new_price = Decimal::from_ratio(200u128, 1u128);

        // Attempt to update the ATOM price
        let msg = ExecuteMsg::UpdateAtomPrice { new_price };
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
        assert_eq!(res.unwrap_err(), StdError::generic_err("Unauthorized"));


        // Attempt to update the USDC price
        let msg = ExecuteMsg::UpdateUsdcPrice { new_price };
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
        assert_eq!(res.unwrap_err(), StdError::generic_err("Unauthorized"));
    }

    #[test]
    fn test_query_prices() {
        let (deps, env, _) = setup_contract();

        // Query the prices
        let msg = QueryMsg::GetPrices;
        let res: PricesResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();


        // Check if the prices are correct
        assert_eq!(res.atom_price, Decimal::from_ratio(100u128, 1u128));
        assert_eq!(res.usdc_price, Decimal::from_ratio(1u128, 1u128));
    }
}