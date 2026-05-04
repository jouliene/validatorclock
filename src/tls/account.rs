use crate::config::AcmeConfig;
use crate::fsutil::write_file_atomic;
use anyhow::{Context, Result};
use instant_acme::{Account as AcmeAccount, AccountCredentials, NewAccount};
use std::fs;

pub(super) async fn load_or_create_acme_account(acme: &AcmeConfig) -> Result<AcmeAccount> {
    let builder = acme_account_builder()?;
    if acme.account_path.exists() {
        let data = fs::read_to_string(&acme.account_path)
            .with_context(|| format!("failed to read {}", acme.account_path.display()))?;
        let credentials: AccountCredentials =
            serde_json::from_str(&data).context("failed to parse ACME account credentials")?;
        return builder
            .from_credentials(credentials)
            .await
            .context("failed to restore ACME account");
    }

    let contacts = acme.contact.iter().map(String::as_str).collect::<Vec<_>>();
    let (account, credentials) = builder
        .create(
            &NewAccount {
                contact: &contacts,
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            acme.directory_url(),
            None,
        )
        .await
        .context("failed to create ACME account")?;

    let data = serde_json::to_vec_pretty(&credentials)?;
    write_file_atomic(&acme.account_path, &data, 0o600)?;
    Ok(account)
}

fn acme_account_builder() -> Result<instant_acme::AccountBuilder> {
    AcmeAccount::builder().context("failed to create ACME account builder")
}
