use lend_shared::{
    fadroma::{auth::Permit, ensemble::MockEnv, Decimal256, StdError, Uint256},
    interfaces::overseer::*,
};

use crate::setup::Lend;
use crate::ADMIN;

#[test]
fn whitelist() {
    let mut lend = Lend::new();

    lend.ensemble
        .execute(
            &HandleMsg::Whitelist {
                market: Market {
                    contract: lend.markets[0].clone(),
                    symbol: "SIENNA".into(),
                    ltv_ratio: Decimal256::percent(90),
                },
            },
            MockEnv::new(ADMIN, lend.overseer.clone()),
        )
        .unwrap();

    // cannot list a market a second time
    let res = lend.ensemble.execute(
        &HandleMsg::Whitelist {
            market: Market {
                contract: lend.markets[0].clone(),
                symbol: "SIENNA".into(),
                ltv_ratio: Decimal256::percent(90),
            },
        },
        MockEnv::new(ADMIN, lend.overseer.clone()),
    );

    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("Token is already registered as collateral.")
    );

    // can list two different markets
    lend.ensemble
        .execute(
            &HandleMsg::Whitelist {
                market: Market {
                    contract: lend.markets[1].clone(),
                    symbol: "ATOM".into(),
                    ltv_ratio: Decimal256::percent(90),
                },
            },
            MockEnv::new(ADMIN, lend.overseer.clone()),
        )
        .unwrap();
    let res = lend
        .ensemble
        .query(
            lend.overseer.address,
            QueryMsg::Markets {
                pagination: Pagination {
                    start: 0,
                    limit: 10,
                },
            },
        )
        .unwrap();

    if let QueryResponse::Markets { whitelist } = res {
        assert_eq!(whitelist.len(), 2)
    }
}

#[test]
fn liquidity() {
    // fails if a market is not listed
    let mut lend = Lend::new();
    let atom_market = lend.markets[1].clone();
    let res = lend.ensemble.execute(
        &HandleMsg::Enter {
            markets: vec![atom_market.address.clone()],
        },
        MockEnv::new("Borrower", lend.overseer.clone()),
    );

    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("Market is not listed.")
    );

    // can list two different markets
    lend.ensemble
        .execute(
            &HandleMsg::Whitelist {
                market: Market {
                    contract: lend.markets[1].clone(),
                    symbol: "ATOM".into(),
                    ltv_ratio: Decimal256::percent(90),
                },
            },
            MockEnv::new(ADMIN, lend.overseer.clone()),
        )
        .unwrap();

    // not in market yet, should have no effect
    let res = lend
        .ensemble
        .query(
            lend.overseer.address.clone(),
            QueryMsg::AccountLiquidity {
                permit: Permit::<OverseerPermissions>::new(
                    "Borrower",
                    vec![OverseerPermissions::AccountInfo],
                    vec![lend.overseer.address.clone()],
                    "balance",
                ),
                market: Some(atom_market.address.clone()),
                redeem_amount: Uint256::from(1u128),
                borrow_amount: Uint256::from(1u128),
            },
        )
        .unwrap();

    if let QueryResponse::AccountLiquidity { liquidity } = res {
        assert_eq!(Uint256::from(0u128), liquidity.liquidity);
        assert_eq!(Uint256::from(0u128), liquidity.shortfall);
    }

    lend.ensemble
        .execute(
            &HandleMsg::Enter {
                markets: vec![atom_market.address.clone()],
            },
            MockEnv::new("Borrower", lend.overseer.clone()),
        )
        .unwrap();

    // total account liquidity after supplying `amount`
    let res = lend
        .ensemble
        .query(
            lend.overseer.address.clone(),
            QueryMsg::AccountLiquidity {
                permit: Permit::<OverseerPermissions>::new(
                    "Borrower",
                    vec![OverseerPermissions::AccountInfo],
                    vec![lend.overseer.address.clone()],
                    "balance",
                ),
                market: Some(atom_market.address.clone()),
                redeem_amount: Uint256::from(0u128),
                borrow_amount: Uint256::from(0u128),
            },
        )
        .unwrap();

    if let QueryResponse::AccountLiquidity { liquidity } = res {
        assert_eq!(
            Uint256::from(1u128)
                .decimal_mul(Decimal256::percent(50))
                .unwrap(),
            liquidity.liquidity
        );
        assert_eq!(Uint256::from(0u128), liquidity.shortfall);
    }
}
