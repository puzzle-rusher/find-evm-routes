use alloy::primitives::private::serde::Deserialize;
use alloy::primitives::{Address, U256};
use std::collections::HashMap;
use tokio::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub factories: HashMap<String, Address>,
    pub input_token: Address,
    pub output_token: Address,
    pub input_amount: U256,
}

pub async fn fetch_config() -> anyhow::Result<Config> {
    let raw = fs::read_to_string("config.toml").await?;
    toml::from_str(&raw).map_err(Into::into)
}
