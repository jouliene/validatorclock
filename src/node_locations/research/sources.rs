use super::{
    Engine,
    model::{DAY, Observation},
};
use anyhow::{Result, bail};
use ipnet::IpNet;
use serde_json::Value;
use std::{io::Read, net::IpAddr, path::Path};

pub fn observation(value: &Value, source: &str, at: u64) -> Option<Observation> {
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .and_then(crate::geoip::sanitized_field)
            .unwrap_or_default()
    };
    let (lat, lon, code) = if source == "ip-api" {
        if text("status") != "success" {
            return None;
        }
        (
            value.get("lat")?.as_f64()?,
            value.get("lon")?.as_f64()?,
            text("countryCode"),
        )
    } else {
        if value.get("success") != Some(&Value::Bool(true)) {
            return None;
        }
        (
            value.get("latitude")?.as_f64()?,
            value.get("longitude")?.as_f64()?,
            text("country_code"),
        )
    };
    let mut result = Observation {
        source: source.into(),
        at,
        city: text("city"),
        country: text("country"),
        country_code: code,
        region: text("regionName"),
        lat,
        lon,
        asn: crate::geoip::parse_asn(&text("as")),
        isp: text("isp"),
    };
    if source == "ipwho.is" {
        result.region = text("region");
        result.asn = value
            .pointer("/connection/asn")
            .and_then(Value::as_u64)
            .map(|a| format!("AS{a}"));
        result.isp = value
            .pointer("/connection/isp")
            .and_then(Value::as_str)
            .and_then(crate::geoip::sanitized_field)
            .unwrap_or_default();
    }
    result.valid().then_some(result)
}

pub fn database_observation(
    reader: &maxminddb::Reader<Vec<u8>>,
    ip: IpAddr,
    at: u64,
) -> Option<Observation> {
    let data: Value = reader.lookup(ip).ok()?.decode().ok()??;
    let name = |path: &str| {
        data.pointer(path)
            .and_then(Value::as_str)
            .and_then(crate::geoip::sanitized_field)
            .unwrap_or_default()
    };
    let point = Observation {
        source: "dbip".into(),
        at,
        city: name("/city/names/en"),
        country: name("/country/names/en"),
        country_code: name("/country/iso_code"),
        region: name("/subdivisions/0/names/en"),
        lat: data.pointer("/location/latitude")?.as_f64()?,
        lon: data.pointer("/location/longitude")?.as_f64()?,
        asn: None,
        isp: String::new(),
    };
    point.valid().then_some(point)
}

#[derive(Clone, Debug)]
pub struct FeedRow {
    pub network: IpNet,
    pub code: String,
    pub region: String,
    pub city: String,
}
pub fn parse_feed(body: &str, latitude_html: bool) -> Vec<FeedRow> {
    // Explicit adapter for the reviewed Latitude feed; never parse arbitrary HTML as CSV.
    let text = if latitude_html {
        body.replace("<p>", "")
            .replace("</p>", "")
            .replace("<br />", "\n")
            .replace("<br>", "\n")
    } else {
        body.to_owned()
    };
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|line| {
            let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
            if fields.len() < 4 || fields[1].len() != 2 {
                return None;
            }
            let city = crate::geoip::sanitized_field(fields[3])?;
            // Latitude has region-only labels in the city column. They cannot resolve a city.
            if city == "New South Wales" || city.starts_with("Region ") {
                return None;
            }
            Some(FeedRow {
                network: fields[0].parse().ok()?,
                code: fields[1].into(),
                region: fields[2].into(),
                city,
            })
        })
        .collect()
}
pub fn feed_match(rows: &[FeedRow], ip: IpAddr) -> Option<&FeedRow> {
    rows.iter()
        .filter(|row| row.network.contains(&ip))
        .max_by_key(|row| row.network.prefix_len())
}

pub fn open_database(path: &Path) -> Result<maxminddb::Reader<Vec<u8>>> {
    if std::fs::metadata(path)?.len() > 300 * 1024 * 1024 {
        bail!("City database exceeds size limit");
    }
    let reader = maxminddb::Reader::open_readfile(path)?;
    if reader.metadata().database_type != "DBIP-City-Lite" {
        bail!("unexpected City database type");
    }
    Ok(reader)
}

impl Engine {
    pub async fn assets(&mut self, now: u64, hetzner: bool, latitude: bool) -> Result<()> {
        if self.database.is_none() && self.database_path().exists() {
            self.database = open_database(&self.database_path()).ok();
        }
        let month = crate::timeutil::day_string(crate::timeutil::day_index(now))[..7].to_owned();
        let dbkey = format!("dbip-{month}");
        if self.config.download_database
            && (self.database.is_none() || !self.store.assets.contains_key(&dbkey))
            && now >= *self.store.assets.get("dbip-retry").unwrap_or(&0)
        {
            self.store.assets.insert("dbip-retry".into(), now + DAY);
            self.save()?;
            let url = format!("https://download.db-ip.com/free/dbip-city-lite-{month}.mmdb.gz");
            if let Some(bytes) = self
                .get_bytes("dbip-download", &url, now, 160 * 1024 * 1024, 120)
                .await?
            {
                let mut uncompressed = Vec::new();
                let decoded = flate2::read::GzDecoder::new(bytes.as_slice())
                    .take(300 * 1024 * 1024 + 1)
                    .read_to_end(&mut uncompressed);
                if decoded.is_ok() && uncompressed.len() <= 300 * 1024 * 1024 {
                    // Validate before replacing a working database.
                    if maxminddb::Reader::from_source(uncompressed.as_slice())
                        .is_ok_and(|reader| reader.metadata().database_type == "DBIP-City-Lite")
                    {
                        crate::fsutil::write_file_atomic(
                            &self.database_path(),
                            &uncompressed,
                            0o600,
                        )?;
                        self.database = Some(maxminddb::Reader::from_source(uncompressed)?);
                        self.store.assets.insert(dbkey, now);
                        self.save()?;
                    }
                }
            }
        }
        for (name, url, wanted) in [
            ("hetzner", "https://www.hetzner.com/geofeed.csv", hetzner),
            ("latitude", "https://geofeed.latitude.sh/", latitude),
        ] {
            if !wanted {
                continue;
            }
            let path = self.directory.join(format!("{name}.csv"));
            if !self.feeds.contains_key(name)
                && path.exists()
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                self.feeds
                    .insert(name.into(), parse_feed(&text, name == "latitude"));
            }
            if now < *self.store.assets.get(name).unwrap_or(&0) {
                continue;
            }
            self.store.assets.insert(name.into(), now + DAY); // Failed download: at most daily.
            self.save()?;
            if let Some(bytes) = self.get_bytes(name, url, now, 16 * 1024 * 1024, 30).await?
                && let Ok(text) = String::from_utf8(bytes)
            {
                let rows = parse_feed(&text, name == "latitude");
                if !rows.is_empty() {
                    crate::fsutil::write_file_atomic(&path, text.as_bytes(), 0o600)?;
                    self.feeds.insert(name.into(), rows);
                    self.store.assets.insert(name.into(), now + 30 * DAY);
                    self.save()?;
                }
            }
        }
        Ok(())
    }
    pub async fn get_bytes(
        &mut self,
        source: &str,
        url: &str,
        now: u64,
        limit: usize,
        timeout: u64,
    ) -> Result<Option<Vec<u8>>> {
        if !self.reserve(source, now)? {
            return Ok(None);
        }
        let response = crate::http::shared_client()
            .get(url)
            .timeout(std::time::Duration::from_secs(timeout))
            .send()
            .await;
        let Ok(mut response) = response else {
            return Ok(None);
        };
        self.rate_headers(source, &response, now)?;
        if !response.status().is_success()
            || response.content_length().is_some_and(|n| n > limit as u64)
        {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        loop {
            // Body truncation/timeouts are failed attempts, not worker failures.
            // Propagating them would skip the IP's retry journal and query it every cycle.
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if bytes.len() + chunk.len() > limit {
                        return Ok(None);
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(_) => return Ok(None),
            }
        }
        Ok(Some(bytes))
    }
}
