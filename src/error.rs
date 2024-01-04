use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("StdError")]
    Std(#[from] StdError),

    #[error("Cw20 Error")]
    Cw20Error(#[from] cw20_base::ContractError),

    #[error("Failed to instantiate lp token")]
    LPTokenError,

    #[error("Amount needed is more than the amount provided")]
    InsufficientTokenAmount,

    #[error("LP token withdrawal amount > available")]
    InvalidLPTokenAmount,

    #[error("Insufficent Liquidity")]
    InsufficentLiquidity,
}
