use super::super::ValidatorSourceDto;
use super::provider::ValidatorSourceProvider;
use super::wallet_tasks::fetch_wallet_tasks;
use anyhow::{Context, Result, bail};
use tycho_types::cell::{Cell, Load};
use tycho_types::models::StdAddr;

pub(super) async fn fetch_validator_controller_sources(
    chain_id: &str,
    provider: &ValidatorSourceProvider,
    wallets: Vec<String>,
) -> Result<Vec<(String, ValidatorSourceDto)>> {
    Ok(
        fetch_wallet_tasks(
            chain_id,
            provider,
            wallets,
            Some("validator controller source not found"),
            "failed to discover validator controller source",
            "validator controller source task failed",
            |provider, wallet| async move {
                discover_validator_controller_source(&provider, &wallet).await
            },
        )
        .await,
    )
}

async fn discover_validator_controller_source(
    provider: &ValidatorSourceProvider,
    validator_wallet: &str,
) -> Result<Option<ValidatorSourceDto>> {
    let pool = validator_controller_pool_address(provider, validator_wallet).await?;

    Ok(Some(ValidatorSourceDto {
        address: pool,
        contract_type_hash: None,
    }))
}

async fn validator_controller_pool_address(
    provider: &ValidatorSourceProvider,
    validator_wallet: &str,
) -> Result<String> {
    let data = provider.account_data(validator_wallet).await?;

    parse_validator_controller_pool(&data).with_context(|| {
        format!("failed to parse validator controller data for `{validator_wallet}`")
    })
}

fn parse_validator_controller_pool(data: &Cell) -> Result<String> {
    let mut slice = data.as_slice()?;
    let mut roles = slice.load_reference_as_slice()?;
    roles.load_u32()?; // controller role cell prefix
    StdAddr::load_from(&mut roles)?; // validator address
    let pool = StdAddr::load_from(&mut roles)?;

    if pool.workchain != 0 || pool.anycast.is_some() || pool.is_zero() {
        bail!("invalid validator controller pool address `{pool}`");
    }

    Ok(pool.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tycho_types::boc::Boc;

    #[test]
    fn parses_validator_controller_pool_address_from_data_cell() -> Result<()> {
        let data = Boc::decode_hex(
            "b5ee9c72010203010001070001a5035c22135b440398d00001a8013c21cff0c750cc07a5bfaa7c4b6329dfb4b7fa619378bf57caa7b22b232f1c0094e4040001a801440c000000020001c220ab39660fed80001a7ff498c00000000000000000080101d1000000009ff98d7cc52b0fbf040e61c776a33330cb7ebb6b7879e9fb08a6baa1ea0a0684cd9002916c5fca10248a6de0d838ca41083c4f93f883e435f8afe312b19c0c96788bea004493230238b92a02599a423d9af26e70ff08bf68245f1e6313e2bfc57e94622640020085801828c8392b8937d23ca78d0868ca0b40108579448b2bb5616d3619b098252b3d1002ce8bb0922c22eefb1db577f26b5a7fc6747009655af89d6b3bb6c11ec785d7de",
        )?;

        let parsed = parse_validator_controller_pool(&data)?;

        assert_eq!(
            parsed,
            "0:a45b17f28409229b78360e3290420f13e4fe20f90d7e2bf8c4ac6703259e22fa"
        );
        Ok(())
    }

    #[test]
    fn parses_tonstakers_validator_controller_pool_address_from_data_cell() -> Result<()> {
        let data = Boc::decode_hex(
            "b5ee9c720102030100010300019e035c1cca9bb2afaf600001a7fd3c21cff0c750cc07a5bfaa7c4b6329dfb4b4080001a801406c000000020001c1c70f1e7dc90240001a7fb50d8000000000000002cec00000001000000000000000000101d1000000009ff4aaf668d845920fb5036e4fc37f8280008b9be461010e03cbf7f75c73df8a0e9003b16d8fea7b8fd73a292a83620988f6c61a836a7d1c06e50b43c307e28edd06de001e939b33f54a12858b5468a3186985d35ea35356252c45419f8555eecc3cd187c00200858007a4e6ccfd5284a162d51a28c61a6174d7a8d4d5894b115067e1557bb30f3461f000f49cd99faa50942c5aa34518c34c2e9af51a9ab129622a0cfc2aaf7661e68c3e",
        )?;

        let parsed = parse_validator_controller_pool(&data)?;

        assert_eq!(
            parsed,
            "0:ec5b63fa9ee3f5ce8a4aa0d882623db186a0da9f4701b942d0f0c1f8a3b741b7"
        );
        Ok(())
    }

    #[test]
    fn parses_alternate_tonstakers_validator_controller_pool_address_from_data_cell() -> Result<()>
    {
        let data = Boc::decode_base64(
            "te6cckECAwEAAQMAAZ4DXBPm+zjp22wAAaf9PCHP8MdQzAelv6p8S2Mp37S0CAABqAFEqAAAAAIAAcE9jmSkX71AABp/tCRAAAAAAAAAAs7AAAAAEAAAAAAAAAAAAQHRAAAAAJ/5DciI7Su9IkV0fLqJk54FZfx5ceC6Gd8HS1eJ1S3/qPAA9HwEZD/tONfVz6lJS0PVKR5viEiEGyj9AuQewGQVnXIAHpObM/VKEoWLVGijGGmF016jU1YlLEVBn4VV7sw80YfAAgCFgAek5sz9UoShYtUaKMYaYXTXqNTViUsRUGfhVXuzDzRh8AD0nNmfqlCULFqjRRjDTC6a9RqasSliKgz8Kq92YeaMPgXhspg=",
        )?;

        let parsed = parse_validator_controller_pool(&data)?;

        assert_eq!(
            parsed,
            "0:3d1f01190ffb4e35f573ea5252d0f54a479be2122106ca3f40b907b01905675c"
        );
        Ok(())
    }
}
