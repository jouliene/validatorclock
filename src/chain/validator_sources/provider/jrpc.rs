use anyhow::{Context, Result, bail};
use minik2::{JrpcTransport, Transport};
use serde_json::json;
use tycho_types::cell::Cell;
use tycho_types::models::account::AccountState;

#[derive(Clone)]
pub(in crate::chain::validator_sources) struct JrpcValidatorSourceProvider {
    rpc: JrpcTransport,
    transport: Transport,
}

impl JrpcValidatorSourceProvider {
    pub(super) fn new(chain_id: &str, endpoint: &str) -> Result<Self> {
        let rpc = JrpcTransport::new(endpoint)
            .with_context(|| format!("invalid RPC endpoint for `{chain_id}`"))?;
        let transport = Transport::from(&rpc);
        Ok(Self { rpc, transport })
    }

    pub(super) async fn account_code_hash(&self, account_address: &str) -> Result<String> {
        let state = self.transport.get_account_state(account_address).await?;
        let account = state
            .account()
            .with_context(|| format!("account `{account_address}` not found"))?;
        let AccountState::Active(state_init) = &account.state else {
            bail!("account `{account_address}` is not active");
        };
        let code = state_init
            .code
            .as_ref()
            .with_context(|| format!("account `{account_address}` has no code"))?;
        Ok(code.repr_hash().to_string())
    }

    pub(super) async fn account_data(&self, account_address: &str) -> Result<Cell> {
        let state = self.transport.get_account_state(account_address).await?;
        let account = state
            .account()
            .with_context(|| format!("account `{account_address}` not found"))?;
        let AccountState::Active(state_init) = &account.state else {
            bail!("account `{account_address}` is not active");
        };
        state_init
            .data
            .clone()
            .with_context(|| format!("account `{account_address}` has no data"))
    }

    pub(super) async fn transaction_bocs(
        &self,
        account_address: &str,
        continuation_lt: Option<&str>,
        limit: u8,
    ) -> Result<Vec<String>> {
        let mut params = json!({
            "account": account_address,
            "limit": limit,
        });
        if let Some(lt) = continuation_lt {
            params["lastTransactionLt"] = json!(lt);
        }

        self.rpc.call("getTransactionsList", params).await
    }
}
