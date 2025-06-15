use crate::config::{Config, fetch_config};
use crate::dex_contracts::IUniswapV2Pair::getReservesReturn;
use crate::dex_contracts::{IUniswapV2Factory, IUniswapV2Pair};
use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use dotenvy::dotenv;
use futures::future::join_all;
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use std::env;
use std::num::NonZeroU32;
use std::str::FromStr;
use std::sync::Arc;
use tokio::fs;
use tracing::{Level, error};

mod config;
mod dex_contracts;

type ArcRateLimiter = Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let config = fetch_config().await?;
    let limiter = create_rate_limiter()?;
    let provider = create_provider().await?;
    let provider = Arc::new(provider);

    let all_pair_addresses = fetch_all_pair_addresses(
        provider.clone(),
        limiter.clone(),
        (config.input_token, config.output_token),
        config.factories.values().copied(),
    )
    .await;
    let all_pair_addresses = all_pair_addresses
        .into_iter()
        .inspect(|pair_address| {
            if let Err(err) = pair_address {
                error!(?err, "Failed to fetch pair address");
            }
        })
        .flatten();

    let all_reserves = fetch_all_reserves(provider, limiter, all_pair_addresses).await;
    let all_reserves = all_reserves
        .into_iter()
        .inspect(|pair_address| {
            if let Err(err) = pair_address {
                error!(?err, "Failed to fetch reserves");
            }
        })
        .flatten();

    let output = calculate_output(
        all_reserves,
        config.input_token,
        config.output_token,
        config.input_amount,
    );

    println!("{}", output);

    Ok(())
}

fn create_rate_limiter() -> anyhow::Result<ArcRateLimiter> {
    let quota = env::var("RATE_LIMIT_IN_SEC")?;
    let quota = u32::from_str(&quota)?;
    let quota = Quota::per_second(NonZeroU32::new(quota).unwrap());
    Ok(Arc::new(RateLimiter::direct(quota)))
}

async fn create_provider() -> anyhow::Result<impl Provider> {
    let ws_url = env::var("WS_RPC_URL")
        .expect("WS_RPC_URL env-var not set (e.g. wss://mainnet.infura.io/ws/v3/<KEY>)");

    let ws_connect = WsConnect::new(ws_url);
    let provider = ProviderBuilder::new().connect_ws(ws_connect).await?; // <-- only line that changed
    Ok(provider)
}

async fn fetch_all_pair_addresses(
    provider: Arc<impl Provider>,
    rate_limiter: ArcRateLimiter,
    pair: (Address, Address),
    factories: impl IntoIterator<Item = Address>,
) -> Vec<alloy::contract::Result<Address>> {
    let factory_calls = factories
        .into_iter()
        .map(|address| IUniswapV2Factory::new(address, provider.clone()))
        .map(|factory| {
            let rate_limiter = rate_limiter.clone();
            async move {
                rate_limiter.until_ready().await;
                factory.getPair(pair.0, pair.1).call().await
            }
        });

    join_all(factory_calls).await
}

async fn fetch_all_reserves(
    provider: Arc<impl Provider>,
    rate_limiter: ArcRateLimiter,
    pairs: impl IntoIterator<Item = Address>,
) -> Vec<alloy::contract::Result<getReservesReturn>> {
    let factory_calls = pairs
        .into_iter()
        .map(|address| IUniswapV2Pair::new(address, provider.clone()))
        .map(|factory| {
            let rate_limiter = rate_limiter.clone();
            async move {
                rate_limiter.until_ready().await;
                factory.getReserves().call().await
            }
        });

    join_all(factory_calls).await
}

fn calculate_output(
    reserves: impl IntoIterator<Item = getReservesReturn>,
    input_token: Address,
    output_token: Address,
    input_amount: U256,
) -> U256 {
    let reserves: Vec<_> = if input_token < output_token {
        reserves
            .into_iter()
            .map(|reserves| (U256::from(reserves.reserve0), U256::from(reserves.reserve1)))
            .collect()
    } else {
        reserves
            .into_iter()
            .map(|reserves| (U256::from(reserves.reserve1), U256::from(reserves.reserve0)))
            .collect()
    };
    let split = pool_algorithms::optimal_split(&reserves, input_amount);
    pool_algorithms::total_output(&reserves, &split, 3000)
}
