//! Network-independent measurements. Probe metadata is evidence, not ground truth.
use super::{
    Engine,
    model::{DAY, Entry, Observation, distance},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, net::IpAddr, time::Duration};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Job {
    pub id: Option<String>,
    pub started_at: u64,
    pub finished: bool,
    pub polls: u32,
    #[serde(default)]
    pub failures: u32,
    #[serde(default)]
    pub retry_at: u64,
    pub response: Option<Value>,
}

fn probe_point(v: &Value, at: u64) -> Option<Observation> {
    let text = |key| {
        v.get(key)
            .and_then(Value::as_str)
            .and_then(crate::geoip::sanitized_field)
    };
    let p = Observation {
        source: "globalping".into(),
        at,
        city: text("city")?,
        country: text("country")?,
        country_code: text("country")?,
        region: text("state").unwrap_or_default(),
        lat: v.get("latitude")?.as_f64()?,
        lon: v.get("longitude")?.as_f64()?,
        asn: None,
        isp: String::new(),
    };
    (p.valid() && !p.city.is_empty()).then_some(p)
}

/// Choose nearest available cities in each proposed country, never from an ASN allowlist.
/// Distance to a probe is only a trigger/candidate, never proof of target location.
pub fn locations(entry: &Entry, probes: &[Value]) -> Vec<Value> {
    let Some(base) = &entry.decision else {
        return vec![];
    };
    let nearest = |p: &Observation| {
        probes
            .iter()
            .filter_map(|v| probe_point(&v["location"], 0))
            .filter(|q| q.country_code == p.country_code)
            .min_by(|a, b| distance(p, a).total_cmp(&distance(p, b)))
    };
    let far = nearest(base).is_some_and(|q| distance(base, &q) > 100.0);
    if entry.reasons.is_empty() && !far {
        return vec![];
    }
    let mut cities = Vec::new();
    for p in entry
        .observations
        .values()
        .chain(entry.previous_location.iter())
        .chain(std::iter::once(base))
    {
        if let Some(q) = nearest(p)
            && !cities
                .iter()
                .any(|v: &Observation| v.city == q.city && v.country_code == q.country_code)
        {
            cities.push(q);
        }
    }
    cities.truncate(3);
    cities
        .into_iter()
        .flat_map(|city| city_probes(&city, probes, &BTreeSet::new(), 2))
        .collect()
}

fn city_probes(
    city: &Observation,
    probes: &[Value],
    excluded: &BTreeSet<u64>,
    limit: usize,
) -> Vec<Value> {
    // Access-network delay can dominate a short metro RTT. Prefer DC probes, but
    // retain other networks when there is insufficient coverage. No target-ASN rule.
    let mut networks = std::collections::BTreeMap::new();
    for v in probes {
        let p = &v["location"];
        if p["city"] != city.city || p["country"] != city.country_code {
            continue;
        }
        let Some(asn) = p["asn"]
            .as_u64()
            .filter(|n| *n > 0 && !excluded.contains(n))
        else {
            continue;
        };
        let dc = v["tags"]
            .as_array()
            .is_some_and(|tags| tags.iter().any(|t| t == "datacenter-network"));
        *networks.entry(asn).or_insert(false) |= dc;
    }
    let mut networks = networks.into_iter().collect::<Vec<_>>();
    networks.sort_by_key(|(asn, dc)| (!*dc, *asn));
    networks
        .into_iter()
        .take(limit)
        .map(|(asn, dc)| {
            let mut v = json!({"country":city.country_code,"city":city.city,"asn":asn,"limit":1});
            if dc {
                v["tags"] = json!(["datacenter-network"]);
            }
            v
        })
        .collect()
}

fn valid_responses(entry: &Entry, ip: IpAddr, now: u64, max_age: u64) -> Vec<(&Value, u64)> {
    entry
        .measurement_history
        .iter()
        .chain(entry.globalping.iter())
        .filter_map(|job| {
            if job.id.is_none()
                || !job.finished
                || now < job.started_at
                || now - job.started_at >= max_age
            {
                return None;
            }
            let body = job.response.as_ref()?;
            (body["target"]
                .as_str()
                .and_then(|s| s.parse::<IpAddr>().ok())
                == Some(ip)
                && body["type"] == "ping"
                && body["status"] == "finished"
                && body["id"].as_str() == job.id.as_deref())
            .then_some((body, job.started_at))
        })
        .collect()
}

fn short_replies(
    entry: &Entry,
    ip: IpAddr,
    now: u64,
    max_age: u64,
) -> Vec<(Observation, u64, f64)> {
    let mut good = Vec::new();
    for (body, at) in valid_responses(entry, ip, now, max_age) {
        for row in body["results"].as_array().into_iter().flatten() {
            let r = &row["result"];
            if r["status"] != "finished"
                || r["resolvedAddress"]
                    .as_str()
                    .and_then(|s| s.parse::<IpAddr>().ok())
                    != Some(ip)
                || r["stats"]["rcv"].as_u64().unwrap_or(0) < 3
            {
                continue;
            }
            let Some(ms) = r["stats"]["min"]
                .as_f64()
                .filter(|n| n.is_finite() && *n > 0.0 && *n <= 3.0)
            else {
                continue;
            };
            if let (Some(p), Some(asn)) = (
                probe_point(&row["probe"], at),
                row["probe"]["asn"].as_u64().filter(|n| *n > 0),
            ) {
                good.push((p, asn, ms));
            }
        }
    }
    good
}

/// One additional independent network when exactly one network supports a metro.
/// Never retry an already measured ASN or keep adding probes until an answer fits.
pub fn followup(entry: &Entry, probes: &[Value], ip: IpAddr, now: u64, max_age: u64) -> Vec<Value> {
    if !entry.measurement_history.is_empty()
        || !entry.globalping.as_ref().is_some_and(|j| j.finished)
    {
        return vec![];
    }
    let good = short_replies(entry, ip, now, max_age);
    if good
        .iter()
        .map(|(_, a, _)| *a)
        .collect::<BTreeSet<_>>()
        .len()
        != 1
    {
        return vec![];
    }
    let city = &good[0].0;
    if good
        .iter()
        .any(|(p, _, _)| p.country_code != city.country_code || distance(p, city) > 100.0)
    {
        return vec![];
    }
    let excluded = valid_responses(entry, ip, now, max_age)
        .into_iter()
        .flat_map(|(b, _)| b["results"].as_array().into_iter().flatten())
        .filter_map(|r| r["probe"]["asn"].as_u64())
        .collect();
    city_probes(city, probes, &excluded, 1)
}

/// Require two independent probe networks agreeing within 100 km at <=3 ms.
/// Reject remote low-RTT alternatives rather than collapsing an anycast target to one city.
pub fn apply(ip: IpAddr, entry: &mut Entry, now: u64, max_age: u64) -> bool {
    let mut good = short_replies(entry, ip, now, max_age);
    if good.iter().enumerate().any(|(i, (a, _, _))| {
        good.iter()
            .skip(i + 1)
            .any(|(b, _, _)| a.country_code != b.country_code || distance(a, b) > 100.0)
    }) {
        entry.confidence = "disputed".into();
        entry
            .reasons
            .push("incompatible Globalping low-RTT locations".into());
        return false;
    }
    if good
        .iter()
        .map(|(_, asn, _)| *asn)
        .collect::<BTreeSet<_>>()
        .len()
        < 2
    {
        return false;
    }
    good.sort_by(|a, b| a.2.total_cmp(&b.2));
    let mut chosen = good.remove(0).0;
    if let Some(base) = &entry.decision {
        chosen.asn = base.asn.clone();
        chosen.isp = base.isp.clone();
        if chosen.country_code == base.country_code {
            chosen.country = base.country.clone();
        }
    }
    entry.decision = Some(chosen);
    entry.confidence = "measured_metro".into();
    entry
        .reasons
        .push("Globalping: >=2 probe ASNs, >=3 replies each, <=3ms; approximate metro".into());
    true
}

impl Engine {
    pub(super) async fn global_probes(&mut self, now: u64) -> Result<Vec<Value>> {
        let path = self.directory.join("globalping-probes.json");
        if now
            >= *self
                .store
                .assets
                .get("globalping-probes-next")
                .unwrap_or(&0)
        {
            self.store
                .assets
                .insert("globalping-probes-next".into(), now + DAY);
            self.save()?;
            let url = format!(
                "{}/probes",
                self.config.measurement_base_url.trim_end_matches('/')
            );
            if let Some(bytes) = self
                .get_bytes("globalping-read", &url, now, 8 * 1024 * 1024, 20)
                .await?
                && let Ok(rows) = serde_json::from_slice::<Vec<Value>>(&bytes)
                && !rows.is_empty()
            {
                crate::fsutil::write_file_atomic(&path, &bytes, 0o600)?;
            }
        }
        Ok(std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default())
    }

    /// Persist remote IDs and source failures; fetching a result is not exempt from backoff.
    pub(super) async fn global_measure(
        &mut self,
        ip: IpAddr,
        entry: &mut Entry,
        locations: Vec<Value>,
        now: u64,
    ) -> Result<bool> {
        let base = self
            .config
            .measurement_base_url
            .trim_end_matches('/')
            .to_string();
        let mut previous_failures = 0;
        if let Some(job) = &entry.globalping {
            if job.finished && job.response.is_some() {
                return Ok(false);
            }
            if now < job.retry_at {
                return Ok(true);
            }
            // Retry an ambiguous submission only after its recorded failure delay.
            // Expired remote jobs are replaced, without resetting their failure count.
            if job.id.is_none() || now.saturating_sub(job.started_at) >= DAY || job.finished {
                previous_failures = job.failures;
                entry.globalping = None;
            }
        }
        if entry.globalping.is_none() {
            let Some(sent_at) = self.reserve_ready("globalping-create", now).await? else {
                return Ok(true);
            };
            entry.globalping = Some(Job {
                started_at: sent_at,
                failures: previous_failures,
                retry_at: sent_at.saturating_add(3600),
                ..Job::default()
            });
            self.persist_entry(ip, entry.clone())?;
            let response = crate::http::shared_client().post(format!("{base}/measurements"))
                .header("User-Agent","validatorclock-geolocation/1").timeout(Duration::from_secs(20))
                .json(&json!({"type":"ping","target":ip.to_string(),"locations":locations,"measurementOptions":{"packets":3}})).send().await;
            if let Ok(response) = response {
                self.rate_headers("globalping-create", &response, sent_at)?;
                if response.status().as_u16() == 429 {
                    entry.globalping = None;
                    self.persist_entry(ip, entry.clone())?;
                    return Ok(true);
                }
                if response.status().is_success()
                    && let Ok(v) = crate::http::json_within::<Value>(response, 64 * 1024).await
                    && let Some(id) = v["id"].as_str().filter(|s| {
                        !s.is_empty()
                            && s.len() <= 100
                            && s.chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                    })
                {
                    let job = entry.globalping.as_mut().unwrap();
                    job.id = Some(id.into());
                    job.failures = 0;
                    job.retry_at = 0;
                }
            }
            let job = entry.globalping.as_mut().unwrap();
            if job.id.is_none() {
                job.failures = job.failures.saturating_add(1);
                job.retry_at = sent_at.saturating_add(super::model::retry_delay(job.failures));
            }
            self.persist_entry(ip, entry.clone())?;
            return Ok(true);
        }
        let id = entry.globalping.as_ref().unwrap().id.clone().unwrap();
        let before = *self
            .store
            .budget
            .requests_by_source
            .get("globalping-read")
            .unwrap_or(&0);
        let bytes = self
            .get_bytes(
                "globalping-read",
                &format!("{base}/measurements/{id}"),
                now,
                512 * 1024,
                20,
            )
            .await?;
        let sent = *self
            .store
            .budget
            .requests_by_source
            .get("globalping-read")
            .unwrap_or(&0)
            > before;
        let response = bytes
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .filter(|v| {
                v["id"] == id
                    && v["type"] == "ping"
                    && v["target"].as_str().and_then(|s| s.parse::<IpAddr>().ok()) == Some(ip)
            });
        let job = entry.globalping.as_mut().unwrap();
        if let Some(v) =
            response.filter(|v| v["status"] == "finished" || v["status"] == "in-progress")
        {
            job.polls = job.polls.saturating_add(1);
            job.failures = 0;
            if v["status"] == "finished" {
                job.finished = true;
                job.response = Some(v);
                job.retry_at = 0;
            } else {
                job.retry_at = now.saturating_add(if job.polls == 1 {
                    30
                } else {
                    super::model::retry_delay(job.polls - 1)
                });
            }
        } else if sent {
            job.failures = job.failures.saturating_add(1);
            job.retry_at = now.saturating_add(super::model::retry_delay(job.failures));
        } else {
            job.retry_at = self
                .store
                .budget
                .not_before
                .get("globalping-read")
                .copied()
                .unwrap_or(0)
                .max(now.saturating_add(30));
        }
        self.persist_entry(ip, entry.clone())?;
        Ok(!entry.globalping.as_ref().unwrap().finished)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn probe(city: &str, lon: f64, asn: u64) -> Value {
        json!({"country":"NL","city":city,"latitude":52.36,"longitude":lon,"asn":asn})
    }
    fn entry() -> Entry {
        let p = probe_point(&probe("Amsterdam", 4.9, 1), 1).unwrap();
        let mut e = Entry {
            decision: Some(p.clone()),
            ..Entry::default()
        };
        e.observations.insert("ip-api".into(), p);
        e.reasons.push("conflict".into());
        e
    }
    fn response(asns: &[u64], lon: f64) -> Value {
        json!({"id":"test","target":"1.1.1.1","type":"ping","status":"finished", "results":asns.iter().map(|asn|json!({"probe":probe("Amsterdam",lon,*asn),"result":{"status":"finished","resolvedAddress":"1.1.1.1","stats":{"min":1.5,"rcv":3}}})).collect::<Vec<_>>()})
    }
    fn with_response(body: Value) -> Entry {
        let mut e = entry();
        e.globalping = Some(Job {
            id: Some("test".into()),
            started_at: 100,
            finished: true,
            response: Some(body),
            polls: 1,
            ..Job::default()
        });
        e
    }
    #[test]
    fn selection_is_independent_of_target_asn_and_uses_distinct_probe_networks() {
        let probes = vec![
            json!({"location":probe("Amsterdam",4.9,10)}),
            json!({"location":probe("Amsterdam",4.9,20)}),
        ];
        for asn in ["AS24940", "AS396356", "AS64501", ""] {
            let mut e = entry();
            e.decision.as_mut().unwrap().asn = Some(asn.into());
            let selected = locations(&e, &probes);
            assert_eq!(selected.len(), 2);
            assert_ne!(selected[0]["asn"], selected[1]["asn"]);
        }
        assert!(locations(&entry(), &[]).is_empty());
    }
    #[test]
    fn agreement_far_from_probes_is_only_a_measurement_trigger() {
        let mut e = entry();
        e.reasons.clear();
        let probes = vec![json!({"location":probe("Other",10.0,10)})];
        assert_eq!(locations(&e, &probes).len(), 1);
        assert_eq!(e.decision.unwrap().city, "Amsterdam");
    }
    #[test]
    fn rejects_wrong_target_old_partial_same_network_or_high_latency() {
        let ip = "1.1.1.1".parse().unwrap();
        assert!(!apply(
            ip,
            &mut with_response(response(&[10, 10], 4.9)),
            101,
            DAY
        ));
        for field in ["target", "id", "status", "type"] {
            let mut body = response(&[10, 20], 4.9);
            body[field] = json!("wrong");
            assert!(!apply(ip, &mut with_response(body), 101, DAY));
        }
        assert!(!apply(
            ip,
            &mut with_response(response(&[10, 20], 4.9)),
            100 + DAY,
            DAY
        ));
        for (key, value) in [("min", 20.0), ("rcv", 2.0)] {
            let mut body = response(&[10, 20], 4.9);
            body["results"][0]["result"]["stats"][key] = json!(value);
            assert!(!apply(ip, &mut with_response(body), 101, DAY));
        }
        let mut body = response(&[10, 20], 4.9);
        body["results"][0]["result"]["resolvedAddress"] = json!("8.8.8.8");
        assert!(!apply(ip, &mut with_response(body), 101, DAY));
    }
    #[test]
    fn accepts_two_networks_but_rejects_remote_low_latency() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut e = with_response(response(&[10, 20], 4.9));
        assert!(apply(ip, &mut e, 101, DAY));
        assert_eq!(e.confidence, "measured_metro");
        let mut body = response(&[10, 20], 4.9);
        body["results"][1]["probe"]["longitude"] = json!(30.0);
        let mut e = with_response(body);
        assert!(!apply(ip, &mut e, 101, DAY));
        assert_eq!(e.confidence, "disputed");
    }
    #[test]
    fn prefers_datacenter_probes_without_target_operator_allowlist() {
        let p = |asn, dc| json!({"location":probe("Amsterdam",4.9,asn),"tags":if dc {vec!["datacenter-network"]} else {vec!["eyeball-network"]}});
        let selected = locations(&entry(), &[p(1, false), p(20, true), p(30, true)]);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0]["asn"], 20);
        assert_eq!(selected[1]["asn"], 30);
        assert_eq!(selected[0]["tags"], json!(["datacenter-network"]));
    }
    #[test]
    fn one_additional_independent_probe_can_resolve_a_partial_measurement() {
        let ip = "1.1.1.1".parse().unwrap();
        let mut body = response(&[10, 20], 4.9);
        body["results"][1]["result"]["stats"]["min"] = json!(12.0);
        let mut e = with_response(body);
        let probes = (10..=30)
            .step_by(10)
            .map(|a| json!({"location":probe("Amsterdam",4.9,a),"tags":["datacenter-network"]}))
            .collect::<Vec<_>>();
        let extra = followup(&e, &probes, ip, 101, DAY);
        assert_eq!(extra.len(), 1);
        assert_eq!(extra[0]["asn"], 30);
        e.measurement_history.push(e.globalping.take().unwrap());
        e.globalping = with_response(response(&[30], 4.9)).globalping;
        assert!(apply(ip, &mut e, 102, DAY));
        assert!(followup(&e, &probes, ip, 102, DAY).is_empty());
        assert!(!apply(ip, &mut e, 100 + DAY, DAY));
    }
}
