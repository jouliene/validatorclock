use super::super::ValidatorSourceDto;
use super::super::util::masterchain_hash_address;
use super::VALIDATOR_TYPE_FETCH_CONCURRENCY;
use crate::config::ChainConfig;
use anyhow::{Context, Result, bail};
use minik2::Transport;
use tokio::task::JoinSet;
use tracing::{debug, warn};
use tycho_types::cell::{Cell, Load};
use tycho_types::models::account::AccountState;
use tycho_types::num::Tokens;

pub(super) async fn fetch_nominator_pool_validator_sources(
    chain: &ChainConfig,
    wallets: Vec<String>,
) -> Result<Vec<(String, ValidatorSourceDto)>> {
    let transport = Transport::jrpc(&chain.rpc)
        .with_context(|| format!("invalid RPC endpoint for `{}`", chain.id))?;
    let mut fetched = Vec::new();

    for chunk in wallets.chunks(VALIDATOR_TYPE_FETCH_CONCURRENCY) {
        let mut tasks = JoinSet::new();
        for wallet in chunk {
            let transport = transport.clone();
            let wallet = wallet.clone();
            tasks.spawn(async move {
                let result = discover_nominator_pool_validator_source(&transport, &wallet).await;
                (wallet, result)
            });
        }

        while let Some(result) = tasks.join_next().await {
            match result {
                Ok((wallet, Ok(Some(source)))) => fetched.push((wallet, source)),
                Ok((wallet, Ok(None))) => {
                    debug!(
                        chain_id = %chain.id,
                        wallet,
                        "nominator pool validator source not found"
                    );
                }
                Ok((wallet, Err(error))) => {
                    debug!(
                        chain_id = %chain.id,
                        wallet,
                        error = ?error,
                        "failed to discover nominator pool validator source"
                    );
                }
                Err(error) => {
                    warn!(
                        chain_id = %chain.id,
                        error = ?error,
                        "nominator pool validator source task failed"
                    );
                }
            }
        }
    }

    Ok(fetched)
}

async fn discover_nominator_pool_validator_source(
    transport: &Transport,
    validator_wallet: &str,
) -> Result<Option<ValidatorSourceDto>> {
    let validator = nominator_pool_validator_address(transport, validator_wallet).await?;

    Ok(Some(ValidatorSourceDto {
        address: validator,
        contract_type_hash: None,
    }))
}

async fn nominator_pool_validator_address(
    transport: &Transport,
    validator_wallet: &str,
) -> Result<String> {
    let state = transport.get_account_state(validator_wallet).await?;
    let account = state
        .account()
        .with_context(|| format!("account `{validator_wallet}` not found"))?;
    let AccountState::Active(state_init) = &account.state else {
        bail!("account `{validator_wallet}` is not active");
    };
    let data = state_init
        .data
        .as_ref()
        .with_context(|| format!("account `{validator_wallet}` has no data"))?;

    parse_nominator_pool_validator(data)
        .with_context(|| format!("failed to parse nominator pool data for `{validator_wallet}`"))
}

fn parse_nominator_pool_validator(data: &Cell) -> Result<String> {
    let mut slice = data.as_slice()?;
    slice.load_uint(8)?; // state
    slice.load_uint(16)?; // nominators_count
    Tokens::load_from(&mut slice)?; // stake_amount_sent
    Tokens::load_from(&mut slice)?; // validator_amount

    let mut config = slice.load_reference_as_slice()?;
    let validator_address = config.load_u256()?;
    Ok(masterchain_hash_address(&validator_address.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tycho_types::cell::{CellBuilder, CellFamily, Store};

    #[test]
    fn parses_nominator_pool_validator_address_from_data_cell() -> Result<()> {
        let validator_address = [0x11_u8; 32];
        let data = build_nominator_pool_data(validator_address)?;

        let parsed = parse_nominator_pool_validator(&data)?;

        assert_eq!(parsed, format!("-1:{}", "11".repeat(32)));
        Ok(())
    }

    fn build_nominator_pool_data(validator_address: [u8; 32]) -> Result<Cell> {
        let context = Cell::empty_context();

        let mut config = CellBuilder::new();
        config.store_raw(&validator_address, 256)?;
        config.store_u16(4000)?; // validator_reward_share
        config.store_u16(40)?; // max_nominators_count
        Tokens::new(500_000_000_000).store_into(&mut config, context)?;
        Tokens::new(10_000_000_000).store_into(&mut config, context)?;
        let config = config.build()?;

        let mut data = CellBuilder::new();
        data.store_u8(0)?; // state
        data.store_u16(1)?; // nominators_count
        Tokens::ZERO.store_into(&mut data, context)?;
        Tokens::new(1_000_000_000).store_into(&mut data, context)?;
        data.store_reference(config)?;
        Ok(data.build()?)
    }
}
