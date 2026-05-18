use super::super::ValidatorSourceDto;
use super::provider::ValidatorSourceProvider;
use super::wallet_tasks::fetch_wallet_tasks;
use anyhow::{Context, Result};
use tycho_types::cell::{Cell, Load};
use tycho_types::models::StdAddr;

pub(super) async fn fetch_whales_pool_proxy_sources(
    chain_id: &str,
    provider: &ValidatorSourceProvider,
    wallets: Vec<String>,
) -> Result<Vec<(String, ValidatorSourceDto)>> {
    Ok(
        fetch_wallet_tasks(
            chain_id,
            provider,
            wallets,
            Some("whales pool proxy source not found"),
            "failed to discover whales pool proxy source",
            "whales pool proxy source task failed",
            |provider, wallet| async move {
                discover_whales_pool_proxy_source(&provider, &wallet).await
            },
        )
        .await,
    )
}

async fn discover_whales_pool_proxy_source(
    provider: &ValidatorSourceProvider,
    validator_wallet: &str,
) -> Result<Option<ValidatorSourceDto>> {
    let pool = whales_pool_proxy_pool_address(provider, validator_wallet).await?;

    Ok(Some(ValidatorSourceDto {
        address: pool,
        contract_type_hash: None,
    }))
}

async fn whales_pool_proxy_pool_address(
    provider: &ValidatorSourceProvider,
    validator_wallet: &str,
) -> Result<String> {
    let data = provider.account_data(validator_wallet).await?;

    parse_whales_pool_proxy_pool(&data)
        .with_context(|| format!("failed to parse whales pool proxy data for `{validator_wallet}`"))
}

fn parse_whales_pool_proxy_pool(data: &Cell) -> Result<String> {
    let mut slice = data.as_slice()?;
    let first_address = StdAddr::load_from(&mut slice)?;
    let second_address = StdAddr::load_from(&mut slice)?;

    let pool = [first_address, second_address]
        .into_iter()
        .find(|address| address.workchain == 0 && address.anycast.is_none() && !address.is_zero())
        .context("whales pool proxy source address not found")?;

    Ok(pool.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tycho_types::boc::Boc;

    #[test]
    fn parses_whales_pool_address_from_proxy_data_cell() -> Result<()> {
        let data = Boc::decode_hex(
            "b5ee9c7201010101004d0000959fe66666666666666666666666666666666666666666666666666666666666666670023a3e301235d4479f4aab429e5d4268b98e6201cba3926c091ac617eea3e72133296a057a6c63be22",
        )?;

        let parsed = parse_whales_pool_proxy_pool(&data)?;

        assert_eq!(
            parsed,
            "0:8e8f8c048d7511e7d2aad0a797509a2e63988072e8e49b0246b185fba8f9c84c"
        );
        Ok(())
    }

    #[test]
    fn parses_whales_pool_address_when_stored_before_elector() -> Result<()> {
        let data = Boc::decode_hex(
            "b5ee9c7201010101004d00009580011dd8fab1897e99603abb24d9ff7adc2e864f9f33a49a834e2668b4af3c7ec7f3fcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccce17100210aa9af8de",
        )?;

        let parsed = parse_whales_pool_proxy_pool(&data)?;

        assert!(parsed.starts_with("0:"));
        Ok(())
    }
}
