use crate::config::AcmeConfig;
use anyhow::{Result, bail};
use instant_acme::Identifier;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use x509_parser::certificate::X509Certificate;
use x509_parser::extensions::GeneralName;

pub(super) fn certificate_subject_names(
    certificate: &X509Certificate<'_>,
) -> Result<HashSet<String>> {
    let mut names = HashSet::new();
    let Some(subject_alternative_name) =
        certificate.subject_alternative_name().map_err(|error| {
            anyhow::anyhow!("invalid subject alternative name extension: {error:?}")
        })?
    else {
        return Ok(names);
    };

    for name in &subject_alternative_name.value.general_names {
        match name {
            GeneralName::DNSName(name) => {
                names.insert(normalize_dns_name(name));
            }
            GeneralName::IPAddress(bytes) => {
                if let Some(address) = ip_address_from_san(bytes) {
                    names.insert(address.to_string());
                }
            }
            _ => {}
        }
    }

    Ok(names)
}

pub(super) fn missing_certificate_identifiers(
    certificate_names: &HashSet<String>,
    acme: &AcmeConfig,
) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    for identifier in acme.identifier_values() {
        let normalized = normalize_certificate_identifier(identifier)?;
        if !certificate_names.contains(&normalized) {
            missing.push(normalized);
        }
    }
    Ok(missing)
}

fn normalize_certificate_identifier(value: &str) -> Result<String> {
    match acme_identifier(value)? {
        Identifier::Dns(name) => Ok(normalize_dns_name(&name)),
        Identifier::Ip(address) => Ok(address.to_string()),
        _ => bail!("unsupported ACME identifier type"),
    }
}

fn normalize_dns_name(name: &str) -> String {
    name.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn ip_address_from_san(bytes: &[u8]) -> Option<IpAddr> {
    match bytes.len() {
        4 => Some(IpAddr::V4(Ipv4Addr::new(
            bytes[0], bytes[1], bytes[2], bytes[3],
        ))),
        16 => {
            let mut octets = [0; 16];
            octets.copy_from_slice(bytes);
            Some(IpAddr::V6(Ipv6Addr::from(octets)))
        }
        _ => None,
    }
}

pub(crate) fn acme_identifier(value: &str) -> Result<Identifier> {
    let name = value.trim();
    if let Ok(address) = name.parse::<IpAddr>() {
        return Ok(Identifier::Ip(address));
    }

    if name.is_empty() {
        bail!("ACME identifier cannot be empty");
    }
    if name.contains('/') || name.starts_with('[') {
        bail!("ACME identifier must be a host or IP address without scheme, path, or brackets");
    }
    if name.rsplit_once(':').is_some_and(|(prefix, port)| {
        !prefix.contains(':') && port.chars().all(|ch| ch.is_ascii_digit())
    }) {
        bail!("ACME identifier must not include a port");
    }
    Ok(Identifier::Dns(name.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_certificate_identifiers() {
        assert_eq!(
            normalize_certificate_identifier("Example.COM.").unwrap(),
            "example.com"
        );
        assert_eq!(
            normalize_certificate_identifier("203.0.113.10").unwrap(),
            "203.0.113.10"
        );
    }

    #[test]
    fn detects_missing_certificate_identifiers() {
        let certificate_names = HashSet::from(["validatorsclock.xyz".to_owned()]);
        let acme = AcmeConfig {
            enabled: true,
            identifier: "validatorsclock.xyz".to_owned(),
            extra_identifiers: vec!["www.validatorsclock.xyz".to_owned()],
            ..AcmeConfig::default()
        };

        assert_eq!(
            missing_certificate_identifiers(&certificate_names, &acme).unwrap(),
            vec!["www.validatorsclock.xyz"]
        );
    }

    #[test]
    fn parses_ip_subject_alternative_names() {
        assert_eq!(
            ip_address_from_san(&[203, 0, 113, 10]).unwrap(),
            "203.0.113.10".parse::<IpAddr>().unwrap()
        );
        assert_eq!(ip_address_from_san(&[0, 1, 2]).as_ref(), None::<&IpAddr>);
    }
}
