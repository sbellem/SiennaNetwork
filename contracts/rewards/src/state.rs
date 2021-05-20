use cosmwasm_std::{
    Api, CanonicalAddr, Extern, HumanAddr, Querier, StdResult,
    Storage, StdError, Uint128, Binary
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_utils::{ContractInfo, ContractInfoStored};
use cosmwasm_utils::storage::{load, save, ns_load, ns_save, ns_remove};
use cosmwasm_utils::viewing_key::ViewingKey;

use crate::data::*;

const CONFIG_KEY: &[u8] = b"config";
const POOLS_KEY: &[u8] = b"pools";
const POOL_INDEX: &[u8] = b"pools_index";
const INACTIVE_POOLS_KEY: &[u8] = b"inactive_pools";
const ACCOUNTS_KEY: &[u8] = b"accounts";

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct Config {
    pub reward_token: ContractInfo,
    pub this_contract: ContractInfo,
    pub token_decimals: u8,
    pub viewing_key: ViewingKey,
    pub prng_seed: Binary,
    pub claim_interval: u64
}

#[derive(Serialize, Deserialize, Debug)]
struct ConfigStored {
    pub reward_token: ContractInfoStored,
    pub this_contract: ContractInfoStored,
    pub token_decimals: u8,
    pub viewing_key: ViewingKey,
    pub prng_seed: Binary,
    pub claim_interval: u64
}

pub(crate) fn save_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    config: &Config
) -> StdResult<()> {
    let config = ConfigStored {
        reward_token: config.reward_token.to_stored(&deps.api)?,
        this_contract: config.this_contract.to_stored(&deps.api)?,
        token_decimals: config.token_decimals,
        viewing_key: config.viewing_key.clone(),
        prng_seed: config.prng_seed.clone(),
        claim_interval: config.claim_interval
    };

    save(&mut deps.storage, CONFIG_KEY, &config)
}

pub(crate) fn load_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>
) -> StdResult<Config> {
    let config: ConfigStored = load(&deps.storage, CONFIG_KEY)?.unwrap();

    Ok(Config {
        reward_token: config.reward_token.to_normal(&deps.api)?,
        this_contract: config.this_contract.to_normal(&deps.api)?,
        token_decimals: config.token_decimals,
        viewing_key: config.viewing_key,
        prng_seed: config.prng_seed,
        claim_interval: config.claim_interval
    })
}

pub(crate) fn replace_active_pools<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    pools: &Vec<RewardPool>
) -> StdResult<()> {
    let mut index = Vec::with_capacity(pools.len());
    let mut pools_stored = Vec::with_capacity(pools.len());

    // Keep sizes for pools that stay
    for pool in pools.iter() {
        let mut pool = pool.to_stored(&deps.api)?;
        index.push(pool.lp_token.address.clone());

        let stored_pool: Option<RewardPoolStored> = 
            ns_load(&deps.storage, POOLS_KEY, pool.lp_token.address.as_slice())?;

        if let Some(p) = stored_pool {
            pool.size = p.size;
        }

        pools_stored.push(pool);
    }

    // Delete all the current pools
    set_current_pools_inactive(deps)?;

    // Finally, save/update the new ones and ensure they are not inactive
    for pool in pools_stored {
        ns_save(
            &mut deps.storage,
            POOLS_KEY,
            pool.lp_token.address.as_slice(),
            &pool
        )?;

        ns_remove(&mut deps.storage, INACTIVE_POOLS_KEY, &pool.lp_token.address.as_slice());
    }

    save(&mut deps.storage, POOL_INDEX, &index)
}

pub(crate) fn get_pools<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Vec<RewardPool>> {
    let index: Vec<CanonicalAddr> = 
        load(&deps.storage, POOL_INDEX)?.unwrap_or(vec![]);

    let mut result = Vec::with_capacity(index.len());

    for addr in index {
        let pool: Option<RewardPoolStored> = 
            ns_load(&deps.storage, POOLS_KEY, addr.as_slice())?;

        if let Some(p) = pool {
            result.push(p.to_normal(&deps.api)?)
        }
    }

    Ok(result)
}

pub(crate) fn get_pool<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: &HumanAddr
) -> StdResult<Option<RewardPool>> {
    let address = deps.api.canonical_address(address)?;

    let result: Option<RewardPoolStored> = 
        ns_load(&deps.storage, POOLS_KEY, address.as_slice())?;
    
    if let Some(pool) = result {
        Ok(Some(pool.to_normal(&deps.api)?))
    } else {
        Ok(None)
    }
}

pub(crate) fn get_inactive_pool<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: &HumanAddr
) -> StdResult<Option<RewardPool>> {
    let address = deps.api.canonical_address(address)?;

    let result: Option<RewardPoolStored> = 
        ns_load(&deps.storage, INACTIVE_POOLS_KEY, address.as_slice())?;
    
    if let Some(pool) = result {
        Ok(Some(pool.to_normal(&deps.api)?))
    } else {
        Ok(None)
    }
}

pub(crate) fn save_pool<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    pool: &RewardPool
) -> StdResult<()> {
    let pool = pool.to_stored(&deps.api)?;

    ns_save(
        &mut deps.storage,
        POOLS_KEY,
        pool.lp_token.address.as_slice(),
        &pool
    )
}

pub(crate) fn save_account<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    account: &Account
) -> StdResult<()> {
    let account = account.to_stored(&deps.api)?;
    let key = generate_account_key(&account.owner, &account.lp_token_addr);

    ns_save(
        &mut deps.storage,
        ACCOUNTS_KEY,
        &key,
        &account
    )
}

pub(crate) fn get_or_create_account<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: &HumanAddr,
    lp_token_addr: &HumanAddr
) -> StdResult<Account> {
    let result: Option<Account> = get_account(deps, address, lp_token_addr)?;

    if let Some(acc) = result {
        Ok(acc)
    } else {
        Ok(Account::new(address.clone(), lp_token_addr.clone()))
    }
}

pub(crate) fn get_account<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: &HumanAddr,
    lp_token_addr: &HumanAddr
) -> StdResult<Option<Account>> {
    let addr_raw = deps.api.canonical_address(&address)?;
    let lp_token_raw = deps.api.canonical_address(&lp_token_addr)?;

    let key = generate_account_key(&addr_raw, &lp_token_raw);
    let result: Option<AccountStored> = ns_load(&deps.storage, ACCOUNTS_KEY, &key)?;

    if let Some(acc) = result {
        Ok(Some(acc.to_normal(&deps.api)?))
    } else {
        Ok(None)
    }
}

pub(crate) fn delete_account<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    account: &Account
) -> StdResult<()> {
    let account = account.to_stored(&deps.api)?;
    let key = generate_account_key(&account.owner, &account.lp_token_addr);

    ns_remove(
        &mut deps.storage,
        ACCOUNTS_KEY,
        &key
    );

    Ok(())
}

fn set_current_pools_inactive<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>
) -> StdResult<()> {
    let index: Vec<CanonicalAddr> = 
        load(&mut deps.storage, POOL_INDEX)?.unwrap_or(vec![]);

    for addr in index {
        let mut pool: RewardPoolStored = 
            ns_load(&mut deps.storage, POOLS_KEY, &addr.as_slice())?
            .ok_or_else(||
                StdError::generic_err(
                    format!("Pool {} doesn't exist in active pool index.", addr)
                )
            )?;
        
        pool.share = Uint128::zero();
        pool.size = Uint128::zero();

        ns_save(
            &mut deps.storage,
            INACTIVE_POOLS_KEY,
            pool.lp_token.address.as_slice(),
            &pool
        )?;
            
        ns_remove(&mut deps.storage, POOLS_KEY, &addr.as_slice());
    }

    save(&mut deps.storage, POOL_INDEX, &Vec::<CanonicalAddr>::new())
}

fn generate_account_key(
    owner: &CanonicalAddr,
    lp_token_addr: &CanonicalAddr
) -> Vec<u8> {
    [ owner.as_slice(), lp_token_addr.as_slice() ].concat()
}
