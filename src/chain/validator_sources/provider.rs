use crate::chain::toncenter_client::is_toncenter_json_rpc_endpoint;
use anyhow::Result;
use tycho_types::cell::Cell;

mod jrpc;
mod toncenter;

use jrpc::JrpcValidatorSourceProvider;
use toncenter::TonCenterValidatorSourceProvider;

#[derive(Clone)]
pub(super) enum ValidatorSourceProvider {
    Jrpc(JrpcValidatorSourceProvider),
    TonCenter(TonCenterValidatorSourceProvider),
}

impl ValidatorSourceProvider {
    pub(super) fn new(chain_id: &str, endpoint: &str) -> Result<Self> {
        if chain_id == "ton" && is_toncenter_json_rpc_endpoint(endpoint) {
            return Ok(Self::TonCenter(TonCenterValidatorSourceProvider::new(
                endpoint,
            )?));
        }

        Ok(Self::Jrpc(JrpcValidatorSourceProvider::new(
            chain_id, endpoint,
        )?))
    }

    pub(super) async fn account_code_hash(&self, account_address: &str) -> Result<String> {
        match self {
            Self::Jrpc(provider) => provider.account_code_hash(account_address).await,
            Self::TonCenter(provider) => provider.account_code_hash(account_address).await,
        }
    }

    pub(super) async fn account_code_hashes(
        &self,
        account_addresses: Vec<String>,
    ) -> Result<Option<Vec<(String, String)>>> {
        match self {
            Self::Jrpc(_) => Ok(None),
            Self::TonCenter(provider) => provider
                .account_code_hashes(account_addresses)
                .await
                .map(Some),
        }
    }

    pub(super) async fn account_data(&self, account_address: &str) -> Result<Cell> {
        match self {
            Self::Jrpc(provider) => provider.account_data(account_address).await,
            Self::TonCenter(provider) => provider.account_data(account_address).await,
        }
    }

    pub(super) async fn transaction_bocs(
        &self,
        account_address: &str,
        continuation_lt: Option<&str>,
        limit: u8,
    ) -> Result<Vec<String>> {
        match self {
            Self::Jrpc(provider) => {
                provider
                    .transaction_bocs(account_address, continuation_lt, limit)
                    .await
            }
            Self::TonCenter(provider) => {
                provider
                    .transaction_bocs(account_address, continuation_lt, limit)
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_toncenter_json_rpc_endpoint() {
        assert!(is_toncenter_json_rpc_endpoint(
            "https://toncenter.com/api/v2/jsonRPC"
        ));
        assert!(!is_toncenter_json_rpc_endpoint(
            "https://jrpc-ton.broxus.com"
        ));
    }
}
