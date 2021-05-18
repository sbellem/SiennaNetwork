use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use composable_admin::admin::{AdminHandleMsg, AdminQueryMsg};
use composable_auth::AuthHandleMsg;
use cosmwasm_utils::ContractInfo;

use crate::data::RewardPool;

pub(crate) const OVERFLOW_MSG: &str = "Upper bound overflow detected.";

#[derive(Serialize, Deserialize, Debug, PartialEq, JsonSchema)]
/// Represents a pair that is eligible for rewards.
pub struct RewardPoolConfig {
    pub lp_token: ContractInfo,
    /// The reward amount allocated to this pool.
    pub share: Uint128,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub admin: Option<HumanAddr>,
    pub reward_token: ContractInfo,
    pub reward_pools: Option<Vec<RewardPoolConfig>>,
    pub claim_interval: u64,
    pub prng_seed: Binary,
    pub entropy: Binary
}

#[derive(Serialize, Deserialize, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    LockTokens { 
        amount: Uint128,
        lp_token: HumanAddr
    },
    RetrieveTokens {
        amount: Uint128,
        lp_token: HumanAddr
    },
    Claim {
        /// The address of the LP tokens pools to claim from.
        lp_tokens: Vec<HumanAddr>
    },
    AddPools { 
        pools: Vec<RewardPoolConfig>
    },
    RemovePools {
        /// The addresses of the LP tokens of the pools to be removed.
        lp_tokens: Vec<HumanAddr>
    },
    Admin(AdminHandleMsg),
    Auth(AuthHandleMsg)
}

#[derive(Serialize, Deserialize, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    ClaimSimulation {
        /// The address of the LP tokens pools to claim from.
        lp_tokens: Vec<HumanAddr>,
        viewing_key: String,
        address: HumanAddr,
        /// Unix time in seconds.
        current_time: u64
    },
    Admin(AdminQueryMsg)
}

#[derive(Serialize, Deserialize, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsgResponse {
    ClaimSimulation(ClaimSimulationResult)
}

#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Debug)]
pub struct ClaimSimulationResult {
    pub results: Vec<ClaimResult>,
    pub total_rewards_amount: Uint128,
    pub actual_claimed: Uint128
}

#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Debug)]
pub struct ClaimResult {
    pub lp_token_addr: HumanAddr,
    pub reward_amount: Uint128,
    pub success: bool,
    pub error: Option<ClaimError>
}

#[derive(Serialize, Deserialize, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum ClaimError {
    PoolEmpty,
    AccountZeroLocked,
    AccountZeroReward,
    EarlyClaim {
        time_to_wait: u64
    }
}

impl ClaimResult {
    pub fn success(lp_token_addr: HumanAddr, reward_amount: Uint128) -> Self {
        Self {
            lp_token_addr,
            reward_amount,
            success: true,
            error: None
        }
    }

    pub fn error(lp_token_addr: HumanAddr, error: ClaimError) -> Self {
        Self {
            lp_token_addr,
            reward_amount: Uint128::zero(),
            success: false,
            error: Some(error)
        }
    }
}

impl Into<RewardPool> for RewardPoolConfig {
    fn into(self) -> RewardPool {
        RewardPool {
            lp_token: self.lp_token,
            share: self.share.u128(),
            size: 0
        }
    }
}
