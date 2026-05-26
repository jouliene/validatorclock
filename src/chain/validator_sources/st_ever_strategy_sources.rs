use super::super::ValidatorSourceDto;
use super::contract_types::account_contract_code_hash;
use super::provider::ValidatorSourceProvider;
use super::wallet_tasks::fetch_wallet_tasks;
use anyhow::Result;
use tracing::debug;
use tycho_types::boc::BocRepr;
use tycho_types::cell::CellSlice;
use tycho_types::models::{MsgInfo, Transaction};

const ST_EVER_STRATEGY_SOURCE_TX_SCAN_LIMIT: u8 = 100;
const ST_EVER_STRATEGY_SOURCE_TX_SCAN_MAX_PAGES: usize = 40;
const ST_EVER_STRATEGY_CONTROLLER_FUNCTION_IDS: &[u32] = &[0xb74e_7374, 0x6335_b11a, 0xf0fd_2250];

pub(super) async fn fetch_st_ever_strategy_sources(
    chain_id: &str,
    provider: &ValidatorSourceProvider,
    wallets: Vec<String>,
) -> Result<Vec<(String, ValidatorSourceDto)>> {
    Ok(fetch_wallet_tasks(
        chain_id,
        provider,
        wallets,
        Some("StEver strategy source not found"),
        "failed to discover StEver strategy source",
        "StEver strategy source task failed",
        |provider, wallet| async move {
            discover_st_ever_strategy_source(&provider, &wallet).await
        },
    )
    .await)
}

async fn discover_st_ever_strategy_source(
    provider: &ValidatorSourceProvider,
    strategy_wallet: &str,
) -> Result<Option<ValidatorSourceDto>> {
    let Some(address) = scan_st_ever_strategy_source_address(provider, strategy_wallet).await?
    else {
        return Ok(None);
    };
    let contract_type_hash = match account_contract_code_hash(provider, &address).await {
        Ok(repr_hash) => Some(repr_hash),
        Err(error) => {
            debug!(
                strategy_wallet,
                source = address,
                error = ?error,
                "failed to fetch StEver strategy controller contract hash"
            );
            None
        }
    };

    Ok(Some(ValidatorSourceDto::new(address, contract_type_hash)))
}

async fn scan_st_ever_strategy_source_address(
    provider: &ValidatorSourceProvider,
    strategy_wallet: &str,
) -> Result<Option<String>> {
    let mut continuation_lt = None::<String>;

    for _ in 0..ST_EVER_STRATEGY_SOURCE_TX_SCAN_MAX_PAGES {
        let tx_bocs = provider
            .transaction_bocs(
                strategy_wallet,
                continuation_lt.as_deref(),
                ST_EVER_STRATEGY_SOURCE_TX_SCAN_LIMIT,
            )
            .await?;
        if tx_bocs.is_empty() {
            break;
        }

        let mut next_continuation = None;
        for tx_boc in tx_bocs {
            let transaction: Transaction = BocRepr::decode_base64(tx_boc)?;
            next_continuation = Some(transaction.prev_trans_lt.to_string());
            if let Some(source) = parse_st_ever_strategy_controller_source(&transaction)? {
                return Ok(Some(source));
            }
        }

        if next_continuation.as_deref() == Some("0") || next_continuation == continuation_lt {
            break;
        }
        continuation_lt = next_continuation;
    }

    Ok(None)
}

fn parse_st_ever_strategy_controller_source(transaction: &Transaction) -> Result<Option<String>> {
    let Some(message) = transaction.load_in_msg()? else {
        return Ok(None);
    };

    let MsgInfo::Int(info) = message.info else {
        return Ok(None);
    };
    let Some(source) = info.src.as_std() else {
        return Ok(None);
    };
    if source.workchain != 0 || !is_st_ever_strategy_controller_call(message.body) {
        return Ok(None);
    }

    Ok(Some(source.to_string()))
}

fn is_st_ever_strategy_controller_call(mut body: CellSlice<'_>) -> bool {
    let Ok(function_id) = body.load_u32() else {
        return false;
    };

    ST_EVER_STRATEGY_CONTROLLER_FUNCTION_IDS.contains(&function_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tycho_types::cell::CellBuilder;

    #[test]
    fn parses_controller_source_from_st_ever_strategy_transaction() -> Result<()> {
        let transaction: Transaction = BocRepr::decode_base64(
            "te6ccgECCQEAAg0AA7d5XKySa6aRGajWYy4/6QPIY4rKQZJCoH2Zwjk7AuMzqMAAAlDZLRaQOSacS0GEp6uxHTw+VL3cCObcb7wIFkU8BW6tug8pv56AAAJQajrCeHahSK6QADSBEZxoKAUEAQIdBO/xG8kO5rKAGIDzSEYRAwIAb8nMS0BMy3KIAAAAAAACAAAAAAAC93nNgEEBVTc1tOYywDZ/LDbs30hXUZGF0y9Oz4TNmshAUBYMAJ5GOmwGGoAAAAAAAAAAAU0AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIJy2YypYI/OiFNUxZ3kb5Vhl0HUdC0lQk9G1ReLtgnWa1J5I10D1XoYLHcQsO8d2cXH8e2iOxKBTSb7ufn+BrmXygIB4AgGAQHfBwCxSf8rlZJNdNIjNRrMZcf9IHkMcVlIMkhUD7M4RydgXGZ1GQADq6Z+r3tabQ8Am7uVXSOssS6oixtlgg1DHl/yu0idAlDNn8VABstzvAAAShslotII1CkV0kAAyWgAHV0z9Xva02h4BN3cqukdZYl1RFjbLBBqGPL/ldpE6BM/5XKySa6aRGajWYy4/6QPIY4rKQZJCoH2Zwjk7AuMzqMQ7msoAAbLc7wAAEobJYRNhNQpFdJ4fpEoAAAAzzAfTO/A",
        )?;

        let parsed = parse_st_ever_strategy_controller_source(&transaction)?;

        assert_eq!(
            parsed.as_deref(),
            Some("0:0eae99fabded69b43c026eee55748eb2c4baa22c6d9608350c797fcaed227409")
        );
        Ok(())
    }

    #[test]
    fn parses_controller_source_from_st_ever_strategy_followup_transaction() -> Result<()> {
        let transaction: Transaction = BocRepr::decode_base64(
            "te6ccgECCgEAAkYAA7d5XKySa6aRGajWYy4/6QPIY4rKQZJCoH2Zwjk7AuMzqMAAAlBqIur0ORiZ9g8xk62s/eSt2KaWBKnLu7O7isgPseSnwe5E6l7wAAJQZhRd0DahRQGgADSBMiBmCAUEAQIdBMGyyIkO5rKAGIEoenARAwIAb8npl0BNGZI4AAAAAAACAAAAAAADrdnudr8KUQSPEHvQtWIHSdIyR3/vyClitddFfZD/h+ZAkB7sAJ5HlwwGGoAAAAAAAAAAAZUAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIJydrhRHwEte3lTLM4tyaGHlcBZxEfG0efMfta2nTMUvINRknxF+zTmKonBOqS6rql+zNbKQ8HdpmfHuHZdLLiQ7wIB4AkGAQHfBwGxaf8rlZJNdNIjNRrMZcf9IHkMcVlIMkhUD7M4RydgXGZ1GQACTfSqAmWJo22tIz838Isk0yFaeqNcPbwXffT/4bGtMRDGDyAABxmT5AAASg1EXV6I1CigNMAIAEfmQsllAAABnl9Y53hgkYTnKgAHAc/XoNXgAAAAAGRSIigUNOgA7WgAHV0z9Xva02h4BN3cqukdZYl1RFjbLBBqGPL/ldpE6BM/5XKySa6aRGajWYy4/6QPIY4rKQZJCoH2Zwjk7AuMzqMQ7msoAAbLc7wAAEoNRD7aBNQooDQxmtiNAAAAzy+sc7wwSMJzlQADgOfr0GrwAAAAADJA",
        )?;

        let parsed = parse_st_ever_strategy_controller_source(&transaction)?;

        assert_eq!(
            parsed.as_deref(),
            Some("0:0eae99fabded69b43c026eee55748eb2c4baa22c6d9608350c797fcaed227409")
        );
        Ok(())
    }

    #[test]
    fn recognizes_st_ever_strategy_controller_function_ids() -> Result<()> {
        for function_id in ST_EVER_STRATEGY_CONTROLLER_FUNCTION_IDS {
            let mut cell = CellBuilder::new();
            cell.store_u32(*function_id)?;
            let cell = cell.build()?;

            assert!(is_st_ever_strategy_controller_call(cell.as_slice()?));
        }
        Ok(())
    }

    #[test]
    fn ignores_st_ever_strategy_non_controller_call() -> Result<()> {
        let mut cell = CellBuilder::new();
        cell.store_u32(0x1690_c604)?;
        let cell = cell.build()?;

        assert!(!is_st_ever_strategy_controller_call(cell.as_slice()?));
        Ok(())
    }
}
