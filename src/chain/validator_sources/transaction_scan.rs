//! Walking an account's transactions backwards, looking for the one that says
//! where a stake came from.
//!
//! Two kinds of intermediary are read this way - a validator's proxy and a
//! stEver strategy - and the walk is the same for both: a page of
//! transactions newest first, each one offered to a parser, then the page
//! before it. Only what the parser recognises differs.

use super::provider::ValidatorSourceProvider;
use anyhow::Result;
use tycho_types::boc::BocRepr;
use tycho_types::models::Transaction;

/// How many transactions to ask for at a time.
const PAGE: u8 = 100;
/// How far back to walk before giving the account up. A stake is set up once
/// and the transaction that did it is usually near the top; this is the point
/// past which it is not worth reading an account's whole life.
const MAX_PAGES: usize = 40;

pub(super) async fn scan_back_for_source(
    provider: &ValidatorSourceProvider,
    wallet: &str,
    recognise: impl Fn(&Transaction) -> Result<Option<String>>,
) -> Result<Option<String>> {
    let mut continuation_lt = None::<String>;

    for _ in 0..MAX_PAGES {
        let tx_bocs = provider
            .transaction_bocs(wallet, continuation_lt.as_deref(), PAGE)
            .await?;
        if tx_bocs.is_empty() {
            break;
        }

        let mut next_continuation = None;
        for tx_boc in tx_bocs {
            let transaction: Transaction = BocRepr::decode_base64(tx_boc)?;
            next_continuation = Some(transaction.prev_trans_lt.to_string());
            if let Some(source) = recognise(&transaction)? {
                return Ok(Some(source));
            }
        }

        // The end of the account's history, or a page that did not move:
        // either way there is nothing further back to read.
        if next_continuation.as_deref() == Some("0") || next_continuation == continuation_lt {
            break;
        }
        continuation_lt = next_continuation;
    }

    Ok(None)
}
