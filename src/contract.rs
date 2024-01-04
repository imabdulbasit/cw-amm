#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Reply, Response,
    StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw0::parse_reply_instantiate_data;
use cw2::set_contract_version;
use cw20::MinterResponse;
use cw20::{Cw20ExecuteMsg, Denom::Cw20};
use cw20_base::contract::query_balance;
use num::integer::Roots;
use std::vec;

use crate::msg::{QueryMsg, ReservesResponse};
use crate::{
    error::Error,
    msg::{ExecuteMsg, InstantiateMsg},
    state::{Token, LP_TOKEN, OWNER, TOKEN1, TOKEN2},
};

const CONTRACT_NAME: &str = "amm";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MINIMUM_LIQUIDITY: Uint128 = Uint128::new(10000);

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, Error> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let token1 = Token {
        reserve: Uint128::zero(),
        denom: msg.token1.clone(),
    };
    TOKEN1.save(deps.storage, &token1)?;

    let token2 = Token {
        reserve: Uint128::zero(),
        denom: msg.token1.clone(),
    };
    TOKEN2.save(deps.storage, &token2)?;

    let owner = deps.api.addr_validate(&msg.owner)?;
    OWNER.save(deps.storage, &Some(owner))?;

    let lp_token_msg = WasmMsg::Instantiate {
        code_id: 0,
        funds: vec![],
        admin: None,
        label: "lp_token".to_string(),
        msg: to_json_binary(&cw20_base::msg::InstantiateMsg {
            name: "lp_token".into(),
            symbol: "LP".into(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: env.contract.address.into(),
                cap: None,
            }),
            marketing: None,
        })?,
    };

    Ok(Response::new().add_submessage(SubMsg::reply_on_success(lp_token_msg, 200)))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, Error> {
    let res = parse_reply_instantiate_data(msg);
    match res {
        Ok(res) => {
            let cw20_addr = deps.api.addr_validate(&res.contract_address)?;
            LP_TOKEN.save(deps.storage, &cw20_addr)?;

            Ok(Response::new())
        }
        Err(_) => Err(Error::LPTokenError),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, Error> {
    match msg {
        ExecuteMsg::Deposit { token1, token2 } => deposit(deps, &info, env, token1, token2),
        ExecuteMsg::Withdraw {
            lp_amount,
            token1_amount,
            token2_amount,
        } => withdraw(deps, &info, env, lp_amount, token1_amount, token2_amount),
        ExecuteMsg::Swap {} => todo!(),
    }
}

pub fn deposit(
    deps: DepsMut,
    info: &MessageInfo,
    env: Env,
    token1_amount: Uint128,
    token2_amount: Uint128,
) -> Result<Response, Error> {
    let token1 = TOKEN1.load(deps.storage)?;
    let token2 = TOKEN2.load(deps.storage)?;
    let lp_token_addr = LP_TOKEN.load(deps.storage)?;

    let lp_contract_state_query: cw20::TokenInfoResponse = deps.querier.query_wasm_smart(
        lp_token_addr.clone(),
        &cw20_base::msg::QueryMsg::TokenInfo {},
    )?;
    let lp_token_total_supply = lp_contract_state_query.total_supply;

    let liquidity: Uint128 = if lp_token_total_supply == Uint128::zero() {
        MINIMUM_LIQUIDITY
            .checked_sub(Uint128::from(
                ((token1_amount
                    .checked_mul(token2_amount)
                    .map_err(StdError::overflow)?)
                .u128())
                .sqrt(),
            ))
            .map_err(StdError::overflow)?

        // lock liquidity by sending to zero address
    } else {
        std::cmp::min(
            token1_amount
                .checked_mul(lp_token_total_supply)
                .map_err(StdError::overflow)?
                .checked_div(token1.reserve)
                .map_err(StdError::divide_by_zero)?,
            token2_amount
                .checked_mul(lp_token_total_supply)
                .map_err(StdError::overflow)?
                .checked_div(token2.reserve)
                .map_err(StdError::divide_by_zero)?,
        )
    };

    if liquidity == Uint128::zero() {
        return Err(Error::InsufficentLiquidity);
    }

    // calculate the token2 amount needed

    let token2_amount_needed = token1_amount
        .checked_mul(token2.reserve)
        .map_err(StdError::overflow)?
        .checked_div(token1.reserve)
        .map_err(StdError::divide_by_zero)?;

    if token2_amount_needed > token2_amount {
        return Err(Error::InsufficientTokenAmount);
    }

    // Issue LP Token

    let mut instructions = Vec::new();
    instructions.push(WasmMsg::Execute {
        contract_addr: lp_token_addr.to_string(),
        msg: to_json_binary(&cw20_base::msg::ExecuteMsg::Mint {
            recipient: info.sender.clone().into(),
            amount: liquidity,
        })?,
        funds: Vec::new(),
    });

    // Transfer Token 1 and Token 2

    if let Cw20(addr) = token1.denom {
        instructions.push(WasmMsg::Execute {
            contract_addr: addr.into(),
            msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: info.sender.clone().into(),
                recipient: env.contract.address.clone().into(),
                amount: token1_amount,
            })?,
            funds: Vec::new(),
        })
    }

    if let Cw20(addr) = token2.denom {
        instructions.push(WasmMsg::Execute {
            contract_addr: addr.into(),
            msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: info.sender.clone().into(),
                recipient: env.contract.address.into(),
                amount: token2_amount_needed,
            })?,
            funds: Vec::new(),
        })
    }

    TOKEN1.update(deps.storage, |mut token1| -> Result<_, Error> {
        token1.reserve += token1_amount;
        Ok(token1)
    })?;
    TOKEN2.update(deps.storage, |mut token2| -> Result<_, Error> {
        token2.reserve += token2_amount;
        Ok(token2)
    })?;

    Ok(Response::new().add_messages(instructions))
}

pub fn withdraw(
    deps: DepsMut,
    info: &MessageInfo,
    _env: Env,
    lp_amount: Uint128,
    token1_amount: Uint128,
    token2_amount: Uint128,
) -> Result<Response, Error> {
    // Check lp balance is >= lp_amount
    // Check what the lp amount represents in tokens
    // burn the lp tokens
    // check if the token amounts are >= input token amounts
    // transfer these token amounts

    let lp_token_address = LP_TOKEN.load(deps.storage)?;
    let token_1 = TOKEN1.load(deps.storage)?;
    let token_2 = TOKEN2.load(deps.storage)?;

    let lp_token_balance_query: cw20::BalanceResponse = deps.querier.query_wasm_smart(
        lp_token_address.clone(),
        &cw20_base::msg::QueryMsg::Balance {
            address: info.sender.to_string(),
        },
    )?;

    let lp_balance = lp_token_balance_query.balance;

    if lp_balance < lp_amount {
        return Err(Error::InvalidLPTokenAmount);
    }

    let amount1 = lp_amount
        .checked_mul(token_1.reserve)
        .map_err(StdError::overflow)?
        .checked_div(lp_balance)
        .map_err(StdError::divide_by_zero)?;

    let amount2 = lp_amount
        .checked_mul(token_2.reserve)
        .map_err(StdError::overflow)?
        .checked_div(lp_balance)
        .map_err(StdError::divide_by_zero)?;

    if amount1 == Uint128::zero() || amount2 == Uint128::zero() {
        return Err(Error::InsufficentLiquidity);
    }

    let sender = &info.sender;
    let mut instructions: Vec<CosmosMsg> = Vec::new();

    // Burn lp tokens

    instructions.push(
        WasmMsg::Execute {
            contract_addr: lp_token_address.to_string(),
            msg: to_json_binary(&cw20_base::msg::ExecuteMsg::BurnFrom {
                owner: sender.to_string(),
                amount: lp_amount,
            })?,
            funds: Vec::new(),
        }
        .into(),
    );

    // The tokens can be a native token or a cw20
    // transfer token 1

    match token_1.denom {
        cw20::Denom::Native(denom) => {
            instructions.push(
                cosmwasm_std::BankMsg::Send {
                    to_address: sender.into(),
                    amount: vec![Coin {
                        denom,
                        amount: amount1,
                    }],
                }
                .into(),
            );
        }
        Cw20(address) => {
            instructions.push(
                WasmMsg::Execute {
                    contract_addr: address.into(),
                    msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: sender.into(),
                        amount: amount1,
                    })?,
                    funds: Vec::new(),
                }
                .into(),
            );
        }
    }

    TOKEN1.update(deps.storage, |mut token1| -> Result<_, Error> {
        token1.reserve = token1
            .reserve
            .checked_sub(token1_amount)
            .map_err(StdError::overflow)?;
        Ok(token1)
    })?;

    // transfer token 2

    match token_2.denom {
        cw20::Denom::Native(denom) => {
            instructions.push(
                cosmwasm_std::BankMsg::Send {
                    to_address: sender.into(),
                    amount: vec![Coin {
                        denom,
                        amount: amount2,
                    }],
                }
                .into(),
            );
        }
        Cw20(address) => {
            instructions.push(
                WasmMsg::Execute {
                    contract_addr: address.into(),
                    msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: sender.into(),
                        amount: amount2,
                    })?,
                    funds: Vec::new(),
                }
                .into(),
            );
        }
    }

    TOKEN2.update(deps.storage, |mut token2| -> Result<_, Error> {
        token2.reserve = token2
            .reserve
            .checked_sub(token2_amount)
            .map_err(StdError::overflow)?;
        Ok(token2)
    })?;

    Ok(Response::new().add_messages(instructions))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Reserves {} => to_json_binary(&query_reserves(deps)?),
        QueryMsg::Share { address } => to_json_binary(&query_balance(deps, address)?),
    }
}

pub fn query_reserves(deps: Deps) -> StdResult<ReservesResponse> {
    let token1 = TOKEN1.load(deps.storage)?;
    let token2 = TOKEN2.load(deps.storage)?;
    let lp_token_address = LP_TOKEN.load(deps.storage)?;

    let lp_token_state_query: cw20::TokenInfoResponse = deps
        .querier
        .query_wasm_smart(lp_token_address, &cw20_base::msg::QueryMsg::TokenInfo {})?;

    Ok(ReservesResponse {
        token1_reserve: token1.reserve,
        token2_reserve: token2.reserve,
        lp_token_supply: lp_token_state_query.total_supply,
    })
}
