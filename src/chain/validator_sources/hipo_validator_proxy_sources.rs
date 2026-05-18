use super::super::ValidatorSourceDto;
use super::VALIDATOR_TYPE_FETCH_CONCURRENCY;
use super::provider::ValidatorSourceProvider;
use anyhow::{Context, Result, bail};
use tokio::task::JoinSet;
use tracing::{debug, warn};
use tycho_types::cell::{Cell, Load};
use tycho_types::models::StdAddr;

pub(super) async fn fetch_hipo_validator_proxy_sources(
    chain_id: &str,
    provider: &ValidatorSourceProvider,
    wallets: Vec<String>,
) -> Result<Vec<(String, ValidatorSourceDto)>> {
    let mut fetched = Vec::new();

    for chunk in wallets.chunks(VALIDATOR_TYPE_FETCH_CONCURRENCY) {
        let mut tasks = JoinSet::new();
        for wallet in chunk {
            let provider = provider.clone();
            let wallet = wallet.clone();
            tasks.spawn(async move {
                let result = discover_hipo_validator_proxy_source(&provider, &wallet).await;
                (wallet, result)
            });
        }

        while let Some(result) = tasks.join_next().await {
            match result {
                Ok((wallet, Ok(Some(source)))) => fetched.push((wallet, source)),
                Ok((wallet, Ok(None))) => {
                    debug!(
                        chain_id = %chain_id,
                        wallet,
                        "Hipo validator proxy source not found"
                    );
                }
                Ok((wallet, Err(error))) => {
                    debug!(
                        chain_id = %chain_id,
                        wallet,
                        error = ?error,
                        "failed to discover Hipo validator proxy source"
                    );
                }
                Err(error) => {
                    warn!(
                        chain_id = %chain_id,
                        error = ?error,
                        "Hipo validator proxy source task failed"
                    );
                }
            }
        }
    }

    Ok(fetched)
}

async fn discover_hipo_validator_proxy_source(
    provider: &ValidatorSourceProvider,
    validator_wallet: &str,
) -> Result<Option<ValidatorSourceDto>> {
    let treasury = hipo_validator_proxy_treasury_address(provider, validator_wallet).await?;

    Ok(Some(ValidatorSourceDto {
        address: treasury,
        contract_type_hash: None,
    }))
}

async fn hipo_validator_proxy_treasury_address(
    provider: &ValidatorSourceProvider,
    validator_wallet: &str,
) -> Result<String> {
    let data = provider.account_data(validator_wallet).await?;

    parse_hipo_validator_proxy_treasury(&data).with_context(|| {
        format!("failed to parse Hipo validator proxy data for `{validator_wallet}`")
    })
}

fn parse_hipo_validator_proxy_treasury(data: &Cell) -> Result<String> {
    let mut slice = data.as_slice()?;
    let elector = StdAddr::load_from(&mut slice)?;
    if elector.workchain != -1 || !elector.address.as_slice().iter().all(|byte| *byte == 0x33) {
        bail!("invalid Hipo validator proxy elector address `{elector}`");
    }

    let treasury = StdAddr::load_from(&mut slice)?;
    if treasury.workchain != 0 || treasury.anycast.is_some() || treasury.is_zero() {
        bail!("invalid Hipo treasury address `{treasury}`");
    }

    Ok(treasury.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tycho_types::boc::Boc;

    #[test]
    fn parses_hipo_validator_proxy_treasury_from_data_cell() -> Result<()> {
        let data = Boc::decode_base64(
            "te6cckEBAQEAawAA0Z/mZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZmZnACLyZHP4Xe8fpchQz76O+/RmUhaVc/9BAoGyJrwJrcbz4ATk2mc+8bueFS/IL7e5ZIHJeGvPCk9EQ2+FVUrd8Vh/+0/6eEQO7D22c=",
        )?;

        let parsed = parse_hipo_validator_proxy_treasury(&data)?;

        assert_eq!(
            parsed,
            "0:8bc991cfe177bc7e9721433efa3befd199485a55cffd040a06c89af026b71bcf"
        );
        Ok(())
    }
}
