use super::super::ValidatorSourceDto;
use super::VALIDATOR_TYPE_FETCH_CONCURRENCY;
use crate::config::ChainConfig;
use anyhow::{Context, Result, bail};
use minik2::Transport;
use tokio::task::JoinSet;
use tracing::{debug, warn};
use tycho_types::cell::{Cell, Load};
use tycho_types::models::StdAddr;
use tycho_types::models::account::AccountState;

pub(super) async fn fetch_single_nominator_validator_sources(
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
                let result = discover_single_nominator_validator_source(&transport, &wallet).await;
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
                        "single nominator validator source not found"
                    );
                }
                Ok((wallet, Err(error))) => {
                    debug!(
                        chain_id = %chain.id,
                        wallet,
                        error = ?error,
                        "failed to discover single nominator validator source"
                    );
                }
                Err(error) => {
                    warn!(
                        chain_id = %chain.id,
                        error = ?error,
                        "single nominator validator source task failed"
                    );
                }
            }
        }
    }

    Ok(fetched)
}

async fn discover_single_nominator_validator_source(
    transport: &Transport,
    validator_wallet: &str,
) -> Result<Option<ValidatorSourceDto>> {
    let owner = single_nominator_owner_address(transport, validator_wallet).await?;

    Ok(Some(ValidatorSourceDto {
        address: owner,
        contract_type_hash: None,
    }))
}

async fn single_nominator_owner_address(
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

    parse_single_nominator_owner(data)
        .with_context(|| format!("failed to parse single nominator data for `{validator_wallet}`"))
}

fn parse_single_nominator_owner(data: &Cell) -> Result<String> {
    let (owner, _validator) = parse_single_nominator_roles(data)?;
    Ok(owner.to_string())
}

fn parse_single_nominator_roles(data: &Cell) -> Result<(StdAddr, StdAddr)> {
    let mut slice = data.as_slice()?;
    let owner = StdAddr::load_from(&mut slice)?;
    let validator = StdAddr::load_from(&mut slice)?;
    Ok((owner, validator))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tycho_types::boc::Boc;

    #[test]
    fn parses_single_nominator_roles_from_data_cell() -> Result<()> {
        let data = Boc::decode_base64(
            "te6cckEBAQEARQAAhYARhy+Ifz/haSyza6FGBWNSde+ZjHy+uBieS1O4PxaPVnP9CEmUYVriFyRPrPcFXVT+DNH+4BQnKkqMnMaShFV7h85i3xsK",
        )?;

        let (owner, validator) = parse_single_nominator_roles(&data)?;

        assert!(owner.anycast.is_none());
        assert!(validator.anycast.is_none());
        assert_ne!(owner, validator);
        Ok(())
    }

    #[test]
    fn parses_single_nominator_pool_roles_from_data_cell() -> Result<()> {
        let data = Boc::decode_hex(
            "b5ee9c720101010100450000858016cda32496b9f32026afea95e03abb31bb94bb289513729916578ab7ed2999a9f3fe6e7cdb9a31840ea60a0845ecea488af59a28349a22075b7c6acc76dbf322e802",
        )?;

        let (owner, validator) = parse_single_nominator_roles(&data)?;

        assert_eq!(
            owner.to_string(),
            "0:b66d1924b5cf9901357f54af01d5d98ddca5d944a89b94c8b2bc55bf694ccd4f"
        );
        assert_eq!(
            validator.to_string(),
            "-1:9b9f36e68c6103a98282117b3a9222bd668a0d268881d6df1ab31db6fcc8ba00"
        );
        Ok(())
    }
}
