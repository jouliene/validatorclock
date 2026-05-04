mod account;
mod acme;
mod cert;
mod names;

pub(crate) use acme::{acme_renewal_loop, ensure_acme_certificate};
pub(crate) use cert::load_tls_acceptor;
pub(crate) use names::acme_identifier;

pub(crate) fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
