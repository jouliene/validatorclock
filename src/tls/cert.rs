use crate::config::TlsConfig;
use anyhow::{Context, Result, anyhow, bail};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use tokio_rustls::TlsAcceptor;

pub(crate) fn load_tls_acceptor(tls: &TlsConfig) -> Result<TlsAcceptor> {
    let certs = load_cert_chain(&tls.cert_path)?;
    let key = load_private_key(&tls.key_path)?;
    let config = build_server_config(certs, key)?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

pub(super) fn build_server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<ServerConfig> {
    let mut config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .context("failed to configure TLS protocol versions")?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .context("failed to load TLS certificate")?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];

    Ok(config)
}

pub(super) fn load_cert_chain(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let mut reader = BufReader::new(
        File::open(path).with_context(|| format!("failed to open cert {}", path.display()))?,
    );
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse cert {}", path.display()))?;
    if certs.is_empty() {
        bail!("cert {} contains no certificates", path.display());
    }
    Ok(certs)
}

pub(super) fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(
        File::open(path)
            .with_context(|| format!("failed to open private key {}", path.display()))?,
    );
    rustls_pemfile::private_key(&mut reader)
        .with_context(|| format!("failed to parse private key {}", path.display()))?
        .ok_or_else(|| anyhow!("private key {} contains no key", path.display()))
}
