pub use cosmwasm_std::{
    StdResult, StdError, Extern, Storage, Api, Querier, Env, Binary, to_binary,
    HandleResponse, from_binary, HumanAddr,
    testing::{mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage}
};
pub use amm_shared::{
    Exchange, ExchangeSettings, Fee,
    TokenPair, TokenType,
    Pagination,
    msg::factory::{InitMsg, HandleMsg, QueryMsg, QueryResponse},
};
pub use fadroma_scrt_addr::{Humanize, Canonize};
pub use fadroma_scrt_callback::ContractInstantiationInfo;
pub use fadroma_scrt_storage::{load, save, remove};
pub use crate::{contract::*, state::*};

impl Into<InitMsg> for &Config<HumanAddr> {
    fn into (self) -> InitMsg {
        InitMsg {
            snip20_contract:   self.snip20_contract.clone(),
            lp_token_contract: self.lp_token_contract.clone(),
            pair_contract:     self.pair_contract.clone(),
            ido_contract:      self.ido_contract.clone(),
            exchange_settings: self.exchange_settings.clone()
        }
    }
}
impl Into<HandleMsg> for &Config<HumanAddr> {
    fn into (self) -> HandleMsg {
        HandleMsg::SetConfig {
            snip20_contract:   Some(self.snip20_contract.clone()),
            lp_token_contract: Some(self.lp_token_contract.clone()),
            pair_contract:     Some(self.pair_contract.clone()),
            ido_contract:      Some(self.ido_contract.clone()),
            exchange_settings: Some(self.exchange_settings.clone())
        }
    }
}
impl Into<QueryResponse> for &Config<HumanAddr> {
    fn into (self) -> QueryResponse {
        QueryResponse::Config {
            snip20_contract:   self.snip20_contract.clone(),
            lp_token_contract: self.lp_token_contract.clone(),
            pair_contract:     self.pair_contract.clone(),
            ido_contract:      self.ido_contract.clone(),
            exchange_settings: self.exchange_settings.clone()
        }
    }
}

fn mkenv (sender: impl Into<HumanAddr>) -> Env {
    mock_env(sender, &[])
}

fn mkdeps () -> Extern<impl Storage, impl Api, impl Querier> {
    mock_dependencies(10, &[])
}

fn mkconfig (id: u64) -> Config<HumanAddr> {
    Config::from_init_msg(InitMsg {
        snip20_contract:   ContractInstantiationInfo { id, code_hash: "snip20".into() },
        lp_token_contract: ContractInstantiationInfo { id, code_hash: "lptoken".into(), },
        pair_contract:     ContractInstantiationInfo { id, code_hash: "2341586789".into(), },
        ido_contract:      ContractInstantiationInfo { id,  code_hash: "348534835".into(), },
        exchange_settings: ExchangeSettings {
            swap_fee: Fee::new(28, 10000),
            sienna_fee: Fee::new(2, 10000),
            sienna_burner: None
        }
    })
}

fn assert_unauthorized(response: StdResult<HandleResponse>) {
    assert!(response.is_err());
    let err = response.unwrap_err();
    assert_eq!(err, StdError::unauthorized())
}

mod test_contract {
    use super::*;

    #[test] fn ok_init () -> StdResult<()> {
        let ref mut deps = mkdeps();
        let env = mkenv("admin");
        let config = mkconfig(0);
        assert!(init(deps, env, (&config).into()).is_ok());
        assert_eq!(config, load_config(deps)?);
        Ok(())
    }

    #[test] fn ok_get_set_config () -> StdResult<()> {
        let ref mut deps = mkdeps();
        let config1 = mkconfig(1);
        let env = mkenv("admin");
        // init with some config
        assert!(init(deps, env.clone(), (&config1).into()).is_ok());
        // get current config
        let response: QueryResponse = from_binary(&query(deps, QueryMsg::GetConfig {})?)?;
        assert_eq!(response, (&config1).into());
        // set config to something else
        let config2 = mkconfig(2);
        assert!(handle(deps, env, (&config2).into()).is_ok());
        // updated config is returned
        let response: QueryResponse = from_binary(&query(deps, QueryMsg::GetConfig {})?)?;
        assert_eq!(response, (&config2).into());
        Ok(())
    }

    #[test] fn no_unauthorized_set_config () -> StdResult<()> {
        let ref mut deps = mkdeps();
        let config1 = mkconfig(1);
        let env = mkenv("admin");
        // init with some config
        assert!(init(deps, env.clone(), (&config1).into()).is_ok());
        // someone else tries to set config
        let config2 = mkconfig(2);
        let env = mkenv("badman");
        assert!(handle(deps, env, (&config2).into()).is_err());
        // config remains unchanged
        let response: QueryResponse = from_binary(&query(deps, QueryMsg::GetConfig {})?)?;
        assert_eq!(response, (&config1).into());
        Ok(())
    }

    #[test] fn create_exchange_for_the_same_tokens_returns_error() -> StdResult<()> {
        let ref mut deps = mkdeps();

        let pair = TokenPair (
            TokenType::CustomToken {
                contract_addr: HumanAddr("token_addr".into()),
                token_code_hash: "13123adasd".into()
            },
            TokenType::CustomToken {
                contract_addr: HumanAddr("token_addr".into()),
                token_code_hash: "13123adasd".into()
            },
        );

        let result = create_exchange(deps, mkenv("sender"), pair);

        let error: StdError = result.unwrap_err();

        let result = match error {
            StdError::GenericErr { msg, .. } => {
                if msg.as_str() == "Cannot create an exchange with the same token." {
                    true
                } else {
                    false
                }
            }
            _ => false
        };

        assert!(result);

        let pair = TokenPair (
            TokenType::NativeToken {
                denom: "test1".into()
            },
            TokenType::NativeToken {
                denom: "test1".into()
            },
        );

        let result = create_exchange(deps, mkenv("sender"), pair);

        let error: StdError = result.unwrap_err();

        let result = match error {
            StdError::GenericErr { msg, .. } => {
                if msg.as_str() == "Cannot create an exchange with the same token." {
                    true
                } else {
                    false
                }
            }
            _ => false
        };

        assert!(result);

        Ok(())
    }

    #[test] fn test_register_exchange() -> StdResult<()> {
        let ref mut deps = mkdeps();

        let pair = TokenPair (
            TokenType::CustomToken {
                contract_addr: HumanAddr("token_addr".into()),
                token_code_hash: "13123adasd".into()
            },
            TokenType::NativeToken {
                denom: "test1".into()
            },
        );

        let sender_addr = HumanAddr("sender1111".into());

        let result = handle(
            deps,
            mkenv(sender_addr.clone()),
            HandleMsg::RegisterExchange {
                pair: pair.clone(),
                signature: to_binary("whatever")?
            }
        );

        assert_unauthorized(result);

        let config = mkconfig(0);
        save_config(deps, &config)?;

        let env = mkenv(sender_addr.clone());

        let signature = create_signature(&env)?;
        save(&mut deps.storage, EPHEMERAL_STORAGE_KEY, &signature)?;

        handle(
            deps,
            env,
            HandleMsg::RegisterExchange {
                pair: pair.clone(),
                signature
            }
        )?;

        //Ensure that the ephemeral storage is empty after the message
        let result: Option<Binary> = load(&deps.storage, EPHEMERAL_STORAGE_KEY)?;
        
        match result {
            None => { },
            _ => panic!("Ephemeral storage should be empty!")
        }

        Ok(())
    }

    #[test] fn test_register_ido() -> StdResult<()> {
        let ref mut deps = mkdeps();

        let sender_addr = HumanAddr("sender1111".into());

        let result = handle(
            deps,
            mkenv(sender_addr.clone()),
            HandleMsg::RegisterIdo {
                signature: to_binary("whatever")?
            }
        );

        assert_unauthorized(result);

        let config = mkconfig(0);
        save_config(deps, &config)?;

        let env = mkenv(sender_addr.clone());

        let signature = create_signature(&env)?;
        save(&mut deps.storage, EPHEMERAL_STORAGE_KEY, &signature)?;

        handle(
            deps,
            env,
            HandleMsg::RegisterIdo {
                signature
            }
        )?;
        //Ensure that the ephemeral storage is empty after the message
        let result: Option<Binary> = load(&deps.storage, EPHEMERAL_STORAGE_KEY)?;
        
        match result {
            None => { },
            _ => panic!("Ephemeral storage should be empty!")
        }

        Ok(())
    }

    #[test] fn query_exchange() -> StdResult<()> {
        let ref mut deps = mkdeps();

        let pair = TokenPair (
            TokenType::CustomToken {
                contract_addr: HumanAddr("token_addr".into()),
                token_code_hash: "13123adasd".into()
            },
            TokenType::NativeToken {
                denom: "test1".into()
            },
        );

        let config = mkconfig(0);
        save_config(deps, &config)?;

        let sender_addr = HumanAddr("sender1111".into());
        let env = mkenv(sender_addr.clone());

        let signature = create_signature(&env)?;
        save(&mut deps.storage, EPHEMERAL_STORAGE_KEY, &signature)?;

        handle(
            deps,
            env,
            HandleMsg::RegisterExchange {
                pair: pair.clone(),
                signature
            }
        ).unwrap();
        
        let result = query(
            deps,
            QueryMsg::GetExchangeAddress {
                pair: pair.clone()
            }
        )?;

        let response: QueryResponse = from_binary(&result)?;

        match response {
            QueryResponse::GetExchangeAddress { address } => assert_eq!(sender_addr, address),
            _ => return Err(StdError::generic_err("Wrong response. Expected: QueryResponse::GetExchangeAddress."))
        };
        
        Ok(())
    }
}

mod test_state {
    use super::*;

    fn swap_pair<A: Clone> (pair: &TokenPair<A>) -> TokenPair<A> {
        TokenPair(pair.1.clone(), pair.0.clone())
    }

    fn pagination(start: u64, limit: u8) -> Pagination {
        Pagination { start, limit }
    }

    fn mock_config() -> Config<HumanAddr> {
        Config::from_init_msg(InitMsg {
            snip20_contract: ContractInstantiationInfo {
                id: 1,
                code_hash: "snip20_contract".into()
            },
            lp_token_contract: ContractInstantiationInfo {
                id: 2,
                code_hash: "lp_token_contract".into()
            },
            ido_contract: ContractInstantiationInfo {
                id: 3,
                code_hash: "ido_contract".into()
            },
            pair_contract: ContractInstantiationInfo {
                id: 4,
                code_hash: "pair_contract".into()
            },
            exchange_settings: ExchangeSettings {
                swap_fee: Fee::new(28, 10000),
                sienna_fee: Fee::new(2, 10000),
                sienna_burner: None
            }
        })
    }

    #[test]
    fn generates_the_same_key_for_swapped_pairs() -> StdResult<()> {
        fn cmp_pair<S: Storage, A: Api, Q: Querier>(
            deps: &Extern<S, A, Q>,
            pair: TokenPair<HumanAddr>
        ) -> StdResult<()> {
            let stored_pair = pair.canonize(&deps.api)?;
            let key = generate_pair_key(&stored_pair);

            let pair = swap_pair(&pair);

            let stored_pair = pair.canonize(&deps.api)?;
            let swapped_key = generate_pair_key(&stored_pair);

            assert_eq!(key, swapped_key);

            Ok(())
        }

        let ref deps = mkdeps();

        cmp_pair(
            deps,
            TokenPair(
                TokenType::CustomToken {
                    contract_addr: HumanAddr("first_addr".into()),
                    token_code_hash: "13123adasd".into()
                },
                TokenType::CustomToken {
                    contract_addr: HumanAddr("scnd_addr".into()),
                    token_code_hash: "4534qwerqqw".into()
                }
            )
        )?;

        cmp_pair(
            deps,
            TokenPair(
                TokenType::NativeToken {
                    denom: "test1".into()
                },
                TokenType::NativeToken {
                    denom: "test2".into()
                },
            )
        )?;

        cmp_pair(
            deps,
            TokenPair(
                TokenType::NativeToken {
                    denom: "test3".into()
                },
                TokenType::CustomToken {
                    contract_addr: HumanAddr("third_addr".into()),
                    token_code_hash: "asd21312asd".into()
                }
            )
        )?;

        Ok(())
    }

    #[test]
    fn query_correct_exchange_info() -> StdResult<()> {
        let mut deps = mkdeps();

        let pair = TokenPair (
            TokenType::CustomToken {
                contract_addr: HumanAddr("first_addr".into()),
                token_code_hash: "13123adasd".into()
            },
            TokenType::CustomToken {
                contract_addr: HumanAddr("scnd_addr".into()),
                token_code_hash: "4534qwerqqw".into()
            }
        );

        let address = HumanAddr("ctrct_addr".into());

        store_exchange(&mut deps, &pair, &address)?;

        let retrieved_address = get_address_for_pair(&deps, &pair)?;

        assert!(pair_exists(&mut deps, &pair)?);
        assert_eq!(address, retrieved_address);

        Ok(())
    }

    #[test]
    fn only_one_exchange_per_factory() -> StdResult<()> {
        let ref mut deps = mkdeps();
        let pair = TokenPair (
            TokenType::CustomToken {
                contract_addr: HumanAddr("first_addr".into()),
                token_code_hash: "13123adasd".into()
            },
            TokenType::CustomToken {
                contract_addr: HumanAddr("scnd_addr".into()),
                token_code_hash: "4534qwerqqw".into()
            }
        );

        store_exchange(deps, &pair, &"first_addr".into())?;

        let swapped = swap_pair(&pair);

        match store_exchange(deps, &swapped, &"other_addr".into()) {
            Ok(_) => Err(StdError::generic_err("Exchange already exists")),
            Err(_) => Ok(())
        }
    }

    #[test]
    fn test_get_idos() -> StdResult<()> {
        let ref mut deps = mkdeps();
        let mut config = mock_config();

        save_config(deps, &config)?;

        let mut addresses = vec![];

        for i in 0..33 {
            let addr = HumanAddr::from(format!("addr_{}", i));

            store_ido_address(deps, &addr, &mut config)?;
            addresses.push(addr);
        }

        let mut config = load_config(deps)?;

        let result = get_idos(deps, &mut config, pagination(addresses.len() as u64, 20))?;
        assert_eq!(result.len(), 0);

        let result = get_idos(deps, &mut config, pagination((addresses.len() - 1) as u64, 20))?;
        assert_eq!(result.len(), 1);

        let result = get_idos(deps, &mut config, pagination(0, PAGINATION_LIMIT + 10))?;
        assert_eq!(result.len(), PAGINATION_LIMIT as usize);

        let result = get_idos(deps, &mut config, pagination(3, PAGINATION_LIMIT))?;
        assert_eq!(result, addresses[3..]);

        Ok(())
    }

    #[test]
    fn test_get_exchanges() -> StdResult<()> {
        let ref mut deps = mkdeps();

        let mut exchanges = vec![];

        for i in 0..33 {
            let pair = TokenPair (
                TokenType::CustomToken {
                    contract_addr: HumanAddr(format!("addr_{}", i)),
                    token_code_hash: format!("code_hash_{}", i)
                },
                TokenType::NativeToken {
                    denom: format!("denom_{}", i)
                },
            );
            let address = HumanAddr(format!("address_{}", i));

            store_exchange(deps, &pair, &address)?;

            exchanges.push(Exchange { pair, address });
        }

        let result = get_exchanges(deps, pagination(exchanges.len() as u64, 20))?;
        assert_eq!(result.len(), 0);

        let result = get_exchanges(deps, pagination((exchanges.len() - 1) as u64, 20))?;
        assert_eq!(result.len(), 1);

        let result = get_exchanges(deps, pagination(0, PAGINATION_LIMIT + 10))?;
        assert_eq!(result.len(), PAGINATION_LIMIT as usize);

        let result = get_exchanges(deps, pagination(3, PAGINATION_LIMIT))?;
        assert_eq!(result, exchanges[3..]);

        Ok(())
    }
}