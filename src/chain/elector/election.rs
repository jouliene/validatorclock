use super::super::util::{hex_lower, masterchain_hash_address};
use super::super::{ElectionCandidateDto, ElectionDto};
use anyhow::Result;
use minik2::{Config, Elector, Transport};

pub(super) async fn fetch_election(transport: &Transport, config: &Config) -> Result<ElectionDto> {
    let elector = Elector::from_config(transport, config)?;
    let data = elector.get_data().await?;
    let Some(current) = data.current_election() else {
        return Ok(ElectionDto::default());
    };

    Ok(ElectionDto {
        candidates: current
            .members
            .iter()
            .map(|(public_key, member)| ElectionCandidateDto {
                public_key: hex_lower(&public_key.0),
                stake: member.msg_value.to_string(),
                stake_raw: member.msg_value.0.to_string(),
                created_at: member.created_at,
                stake_factor: member.stake_factor,
                wallet: masterchain_hash_address(&member.src_addr.0),
                source: None,
                contract_type: None,
                contract_type_hash: None,
                adnl_addr: hex_lower(&member.adnl_addr.0),
                history: Vec::new(),
            })
            .collect(),
    })
}
