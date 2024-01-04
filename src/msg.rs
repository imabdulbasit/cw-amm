use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Uint128;

use cw20::Denom;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]

pub struct InstantiateMsg {
    pub token1: Denom,
    pub token2: Denom,
    pub owner: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Deposit {
        token1: Uint128,
        token2: Uint128,
    },
    Withdraw {
        lp_amount: Uint128,
        token1_amount: Uint128,
        token2_amount: Uint128,
    },
    Swap {},
}

#[derive(Debug, Serialize, Deserialize)]
pub enum QueryMsg {
    Reserves {},
    Share { address: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReservesResponse {
    pub token1_reserve: Uint128,
    pub token2_reserve: Uint128,
    pub lp_token_supply: Uint128,
}
