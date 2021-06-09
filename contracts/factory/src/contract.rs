use cosmwasm_std::{
    Api, Binary, CosmosMsg, Env, Extern, HandleResponse, InitResponse,
    Querier, StdError, StdResult, Storage, WasmMsg, log, to_binary, HumanAddr
};
use amm_shared::{
    TokenPair,
    Pagination,
    msg::{
        exchange::InitMsg as ExchangeInitMsg,
        ido::{InitMsg as IdoInitMsg, InitConfig as IdoInitConfig},
        factory::{InitMsg, HandleMsg, QueryMsg, QueryResponse}
    },
    admin::{
        require_admin,
        admin::{
            DefaultHandleImpl as AdminHandle, DefaultQueryImpl as AdminQuery,
            save_admin, admin_handle, admin_query, assert_admin
        }
    }
};
use fadroma_scrt_callback::{ContractInstance, Callback};
use fadroma_scrt_storage::{load, save, remove};
use crate::state::{
    save_config, load_config, Config, pair_exists, store_exchange,
    get_address_for_pair, get_idos, store_ido_address, get_exchanges
};
use fadroma_scrt_migrate::get_status;

pub const EPHEMERAL_STORAGE_KEY: &[u8] = b"ephemeral_storage";

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    save_config(deps, &Config::from_init_msg(msg))?;
    save_admin(deps, &env.message.sender)?;
    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    with_status!(deps, env, match msg {
        HandleMsg::SetConfig { .. }          => set_config(deps, env, msg),
        HandleMsg::CreateExchange { pair }   => create_exchange(deps, env, pair),
        HandleMsg::CreateIdo { info }        => create_ido(deps, env, info),
        HandleMsg::RegisterIdo { signature } => register_ido(deps, env, signature),
        HandleMsg::RegisterExchange { pair, signature } =>
            register_exchange(deps, env, pair, signature),

        HandleMsg::Admin(msg) => admin_handle(deps, env, msg, AdminHandle),
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Status                       => to_binary(&get_status(deps)?),
        QueryMsg::GetConfig {}                 => get_config(deps),
        QueryMsg::GetExchangeAddress { pair }  => query_exchange_address(deps, pair),
        QueryMsg::ListExchanges { pagination } => list_exchanges(deps, pagination),
        QueryMsg::ListIdos { pagination }      => list_idos(deps, pagination),
        QueryMsg::GetExchangeSettings          => query_exchange_settings(deps),

        QueryMsg::Admin(msg) => admin_query(deps, msg, AdminQuery),
    }
}

#[require_admin]
pub fn set_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env:  Env,
    msg:  HandleMsg
) -> StdResult<HandleResponse> {
    if let HandleMsg::SetConfig {
        snip20_contract, lp_token_contract, pair_contract, ido_contract,
        exchange_settings
    } = msg {
        let mut config = load_config(&deps)?;

        if let Some(new_value) = snip20_contract   { config.snip20_contract   = new_value; }
        if let Some(new_value) = lp_token_contract { config.lp_token_contract = new_value; }
        if let Some(new_value) = pair_contract     { config.pair_contract     = new_value; }
        if let Some(new_value) = ido_contract      { config.ido_contract      = new_value; }
        if let Some(new_value) = exchange_settings { config.exchange_settings = new_value; }

        save_config(deps, &config)?;

        Ok(HandleResponse {
            messages: vec![],
            log: vec![
                log("action", "set_config")
            ],
            data: None
        })
    } else {
        unreachable!()
    }
}

pub fn get_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Binary> {
    let Config {
        snip20_contract, lp_token_contract, pair_contract, ido_contract,
        exchange_settings, ..
    } = load_config(deps)?;

    to_binary(&QueryResponse::Config {
        snip20_contract, lp_token_contract, pair_contract, ido_contract,
        exchange_settings
    })
}

pub fn create_exchange<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    pair: TokenPair<HumanAddr>
) -> StdResult<HandleResponse> {

    if pair.0 == pair.1 {
        return Err(StdError::generic_err("Cannot create an exchange with the same token."));
    }

    if pair_exists(deps, &pair)? {
        return Err(StdError::generic_err("Pair already exists"));
    }

    let config = load_config(deps)?;

    // We take advantage of the serialized execution model to create a signature
    // and remove it at the end of the transaction. This signature is passed to
    // the created pair which it then returns to HandleMsg::RegisterExchange so that
    // it can be compared to the one we stored. This way, we ensure that exchanges 
    // can only be created through this method.
    let signature = create_signature(&env)?;
    save(&mut deps.storage, EPHEMERAL_STORAGE_KEY, &signature)?;

    // Actually creating the exchange happens when the instantiated contract calls
    // us back via the HandleMsg::RegisterExchange so that we can get its address.

    Ok(HandleResponse{
        messages: vec![
            CosmosMsg::Wasm(
                WasmMsg::Instantiate {
                    code_id: config.pair_contract.id,
                    callback_code_hash: config.pair_contract.code_hash,
                    send: vec![],
                    label: format!(
                        "{}-{}-pair-{}-{}",
                        pair.0,
                        pair.1,
                        env.contract.address,
                        config.pair_contract.id
                    ),
                    msg: to_binary(
                        &ExchangeInitMsg {
                            pair: pair.clone(),
                            lp_token_contract: config.lp_token_contract.clone(),
                            factory_info: ContractInstance {
                                code_hash: env.contract_code_hash.clone(),
                                address:   env.contract.address.clone()
                            },
                            callback: Callback {
                                contract: ContractInstance {
                                    address:   env.contract.address,
                                    code_hash: env.contract_code_hash,
                                },
                                msg: to_binary(&HandleMsg::RegisterExchange {
                                    pair: pair.clone(),
                                    signature
                                })?,
                            }
                        }
                    )?
                }
            )
        ],
        log: vec![
            log("action", "create_exchange"),
            log("pair", pair),
        ],
        data: None
    })
}

fn register_exchange<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    pair: TokenPair<HumanAddr>,
    signature: Binary
) -> StdResult<HandleResponse> {
    ensure_correct_signature(&mut deps.storage, signature)?;
    store_exchange(deps, &pair, &env.message.sender)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "register_exchange"),
            log("address", env.message.sender),
            log("pair", pair)
        ],
        data: None
    })
}

fn query_exchange_address<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    pair: TokenPair<HumanAddr>
) -> StdResult<Binary> {
    let address = get_address_for_pair(deps, &pair)?;
    
    to_binary(&QueryResponse::GetExchangeAddress {
        address
    })
}

fn create_ido<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    info: IdoInitConfig
) -> StdResult<HandleResponse> {
    let signature = create_signature(&env)?;
    save(&mut deps.storage, EPHEMERAL_STORAGE_KEY, &signature)?;
    
    // Again, creating the IDO happens when the instantiated contract calls
    // us back via the HandleMsg::RegisterIdo so that we can get its address.
    
    let config = load_config(deps)?;

    Ok(HandleResponse {
        messages: vec![
            CosmosMsg::Wasm(WasmMsg::Instantiate {
                code_id: config.ido_contract.id,
                callback_code_hash: config.ido_contract.code_hash.clone(),
                send: vec![],
                label: format!(
                    "IDO for {}({}), id: {}",
                    info.snip20_init_info.name,
                    info.snip20_init_info.symbol,
                    config.ido_contract.id
                ),
                msg: to_binary(&IdoInitMsg {
                    info,
                    snip20_contract: config.snip20_contract,
                    callback: Callback {
                        contract: ContractInstance {
                            address:   env.contract.address,
                            code_hash: env.contract_code_hash,
                        },
                        msg: to_binary(&HandleMsg::RegisterIdo {
                            signature
                        })?
                    }
                })?
            })
        ],
        log: vec![
            log("action", "create_exchange")
        ],
        data: None
    })
}

fn register_ido<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    signature: Binary
) -> StdResult<HandleResponse> {
    ensure_correct_signature(&mut deps.storage, signature)?;

    let mut config = load_config(deps)?;

    store_ido_address(deps, &env.message.sender, &mut config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("created IDO", env.message.sender)
        ],
        data: None
    })
}

fn list_idos<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    pagination: Pagination
) -> StdResult<Binary> {
    let config = load_config(deps)?;
    let idos = get_idos(deps, &config, pagination)?;

    to_binary(&QueryResponse::ListIdos { idos })
}

fn list_exchanges<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    pagination: Pagination
) -> StdResult<Binary> {
    let exchanges = get_exchanges(deps, pagination)?;

    to_binary(&QueryResponse::ListExchanges { exchanges })
}

fn query_exchange_settings<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>
) -> StdResult<Binary> {
    let config = load_config(deps)?;

    Ok(to_binary(&QueryResponse::GetExchangeSettings {
        settings: config.exchange_settings
    })?)
}

pub fn create_signature(env: &Env) -> StdResult<Binary> {
    to_binary(&[
        env.message.sender.0.as_bytes(),
        &env.block.height.to_be_bytes(),
        &env.block.time.to_be_bytes()
    ].concat())
}

fn ensure_correct_signature(storage: &mut impl Storage, signature: Binary) -> StdResult<()> {
    let stored_signature: Binary =
        load(storage, EPHEMERAL_STORAGE_KEY)?.unwrap_or_default();

    if stored_signature != signature {
        return  Err(StdError::unauthorized());
    }

    remove(storage, EPHEMERAL_STORAGE_KEY);

    Ok(())
}