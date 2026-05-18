use super::super::ValidatorSourceDto;
use super::provider::ValidatorSourceProvider;
use super::wallet_tasks::fetch_wallet_tasks;
use anyhow::{Context, Result};
use tycho_types::cell::{Cell, Load};
use tycho_types::models::StdAddr;

pub(super) async fn fetch_single_nominator_validator_sources(
    chain_id: &str,
    provider: &ValidatorSourceProvider,
    wallets: Vec<String>,
) -> Result<Vec<(String, ValidatorSourceDto)>> {
    Ok(fetch_wallet_tasks(
        chain_id,
        provider,
        wallets,
        Some("single nominator validator source not found"),
        "failed to discover single nominator validator source",
        "single nominator validator source task failed",
        |provider, wallet| async move {
            discover_single_nominator_validator_source(&provider, &wallet).await
        },
    )
    .await)
}

async fn discover_single_nominator_validator_source(
    provider: &ValidatorSourceProvider,
    validator_wallet: &str,
) -> Result<Option<ValidatorSourceDto>> {
    let owner = single_nominator_owner_address(provider, validator_wallet).await?;

    Ok(Some(ValidatorSourceDto {
        address: owner,
        contract_type_hash: None,
    }))
}

async fn single_nominator_owner_address(
    provider: &ValidatorSourceProvider,
    validator_wallet: &str,
) -> Result<String> {
    let data = provider.account_data(validator_wallet).await?;

    parse_single_nominator_owner(&data)
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
