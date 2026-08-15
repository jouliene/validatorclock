use crate::chain::graphql_client::GraphqlClient;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::json;
use tycho_types::boc::Boc;
use tycho_types::cell::Cell;

const ACCOUNT_TYPE_ACTIVE: u8 = 1;
const ACCOUNT_BATCH_SIZE: usize = 40;
const GRAPHQL_MAX_LIMIT: u8 = 50;

const ACCOUNTS_QUERY: &str = "query($ids:[String],$limit:Int){\
accounts(filter:{id:{in:$ids}},limit:$limit){id acc_type code_hash data}\
}";
const TRANSACTIONS_QUERY: &str = "query($account:String,$limit:Int){\
transactions(filter:{account_addr:{eq:$account}},orderBy:{path:\"lt\",direction:DESC},limit:$limit){boc}\
}";
const TRANSACTIONS_FROM_LT_QUERY: &str = "query($account:String,$lt:String,$limit:Int){\
transactions(filter:{account_addr:{eq:$account},lt:{le:$lt}},orderBy:{path:\"lt\",direction:DESC},limit:$limit){boc}\
}";

#[derive(Clone)]
pub(in crate::chain::validator_sources) struct GraphqlValidatorSourceProvider {
    client: GraphqlClient,
}

impl GraphqlValidatorSourceProvider {
    pub(super) fn new(chain_id: &str, endpoint: &str) -> Result<Self> {
        Ok(Self {
            client: GraphqlClient::new(endpoint)
                .with_context(|| format!("invalid GraphQL endpoint for `{chain_id}`"))?,
        })
    }

    pub(super) async fn account_code_hash(&self, account_address: &str) -> Result<String> {
        let accounts = self.fetch_accounts(&[account_address.to_owned()]).await?;
        let account = accounts
            .iter()
            .find(|account| account.matches(account_address))
            .with_context(|| format!("GraphQL account `{account_address}` not found"))?;
        account.code_hash()
    }

    pub(super) async fn account_code_hashes(
        &self,
        account_addresses: Vec<String>,
    ) -> Result<Vec<(String, String)>> {
        let mut fetched = Vec::new();
        for chunk in account_addresses.chunks(ACCOUNT_BATCH_SIZE) {
            let accounts = self.fetch_accounts(chunk).await?;
            for account_address in chunk {
                if let Some(code_hash) = accounts
                    .iter()
                    .find(|account| account.matches(account_address))
                    .and_then(|account| account.code_hash().ok())
                {
                    fetched.push((account_address.clone(), code_hash));
                }
            }
        }

        Ok(fetched)
    }

    pub(super) async fn account_data(&self, account_address: &str) -> Result<Cell> {
        let accounts = self.fetch_accounts(&[account_address.to_owned()]).await?;
        let account = accounts
            .iter()
            .find(|account| account.matches(account_address))
            .with_context(|| format!("GraphQL account `{account_address}` not found"))?;
        account.data_cell()
    }

    pub(super) async fn transaction_bocs(
        &self,
        account_address: &str,
        continuation_lt: Option<&str>,
        limit: u8,
    ) -> Result<Vec<String>> {
        let limit = limit.clamp(1, GRAPHQL_MAX_LIMIT);
        let response: TransactionsResponse = match continuation_lt {
            Some(lt) => {
                self.client
                    .query(
                        "transactions",
                        TRANSACTIONS_FROM_LT_QUERY,
                        json!({
                            "account": account_address,
                            "lt": decimal_lt_to_hex(lt)?,
                            "limit": limit,
                        }),
                    )
                    .await?
            }
            None => {
                self.client
                    .query(
                        "transactions",
                        TRANSACTIONS_QUERY,
                        json!({ "account": account_address, "limit": limit }),
                    )
                    .await?
            }
        };

        Ok(response
            .transactions
            .into_iter()
            .filter_map(|transaction| transaction.boc.filter(|boc| !boc.is_empty()))
            .collect())
    }

    async fn fetch_accounts(&self, account_addresses: &[String]) -> Result<Vec<AccountEntry>> {
        let response: AccountsResponse = self
            .client
            .query(
                "accounts",
                ACCOUNTS_QUERY,
                json!({
                    "ids": account_addresses,
                    "limit": GRAPHQL_MAX_LIMIT,
                }),
            )
            .await?;
        Ok(response.accounts)
    }
}

#[derive(Debug, Deserialize)]
struct AccountsResponse {
    #[serde(default)]
    accounts: Vec<AccountEntry>,
}

#[derive(Debug, Deserialize)]
struct AccountEntry {
    id: String,
    acc_type: u8,
    code_hash: Option<String>,
    data: Option<String>,
}

impl AccountEntry {
    fn matches(&self, account_address: &str) -> bool {
        self.id.eq_ignore_ascii_case(account_address)
    }

    fn code_hash(&self) -> Result<String> {
        self.ensure_active()?;
        self.code_hash
            .as_deref()
            .map(str::trim)
            .filter(|code_hash| !code_hash.is_empty())
            .map(|code_hash| code_hash.to_ascii_lowercase())
            .with_context(|| format!("GraphQL account `{}` has no code", self.id))
    }

    fn data_cell(&self) -> Result<Cell> {
        self.ensure_active()?;
        let data = self
            .data
            .as_deref()
            .filter(|data| !data.is_empty())
            .with_context(|| format!("GraphQL account `{}` has no data", self.id))?;
        Boc::decode_base64(data)
            .with_context(|| format!("failed to decode GraphQL account data `{}`", self.id))
    }

    fn ensure_active(&self) -> Result<()> {
        if self.acc_type != ACCOUNT_TYPE_ACTIVE {
            bail!("GraphQL account `{}` is not active", self.id);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct TransactionsResponse {
    #[serde(default)]
    transactions: Vec<TransactionEntry>,
}

#[derive(Debug, Deserialize)]
struct TransactionEntry {
    boc: Option<String>,
}

fn decimal_lt_to_hex(lt: &str) -> Result<String> {
    let lt = lt.trim();
    if let Some(hex) = lt.strip_prefix("0x") {
        return Ok(format!("0x{}", hex.to_ascii_lowercase()));
    }

    let lt: u64 = lt
        .parse()
        .with_context(|| format!("invalid transaction lt `{lt}`"))?;
    Ok(format!("0x{lt:x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(acc_type: u8, code_hash: Option<&str>, data: Option<&str>) -> AccountEntry {
        AccountEntry {
            id: "0:abc".to_owned(),
            acc_type,
            code_hash: code_hash.map(str::to_owned),
            data: data.map(str::to_owned),
        }
    }

    #[test]
    fn converts_decimal_lt_to_hex() {
        assert_eq!(
            decimal_lt_to_hex("74898490523394").unwrap(),
            format!("0x{:x}", 74_898_490_523_394_u64)
        );
        assert_eq!(
            decimal_lt_to_hex("74898490523394").unwrap(),
            "0x441ea9ebff02"
        );
        assert_eq!(decimal_lt_to_hex("0").unwrap(), "0x0");
        assert_eq!(decimal_lt_to_hex("0xABCD").unwrap(), "0xabcd");
        assert!(decimal_lt_to_hex("not-a-number").is_err());
    }

    #[test]
    fn matches_addresses_case_insensitively() {
        assert!(account(1, None, None).matches("0:ABC"));
        assert!(!account(1, None, None).matches("0:abd"));
    }

    #[test]
    fn reads_code_hash_of_active_accounts_only() {
        assert_eq!(
            account(1, Some("AABB"), None).code_hash().unwrap(),
            "aabb".to_owned()
        );
        assert!(
            account(0, Some("aabb"), None)
                .code_hash()
                .unwrap_err()
                .to_string()
                .contains("is not active")
        );
        assert!(
            account(1, None, None)
                .code_hash()
                .unwrap_err()
                .to_string()
                .contains("has no code")
        );
    }

    #[test]
    fn parses_transactions_response() {
        let response: TransactionsResponse = serde_json::from_value(json!({
            "transactions": [{ "boc": "te6" }, { "boc": null }]
        }))
        .unwrap();

        assert_eq!(response.transactions.len(), 2);
        assert_eq!(response.transactions[0].boc.as_deref(), Some("te6"));
    }
}
