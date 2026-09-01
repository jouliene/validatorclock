use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;

use super::AppState;
use super::json_store::JsonStore;
use crate::timeutil::{day_index, day_string, now_sec as now_seconds, parse_day_index};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnalyticsEventKind {
    PageOpen,
    Heartbeat,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PublicAnalytics {
    today: PublicAnalyticsToday,
    last_7_days: PublicAnalyticsWindow,
    last_30_days: PublicAnalyticsWindow,
    all_time: PublicAnalyticsAllTime,
}

#[derive(Debug, Clone, Serialize)]
struct PublicAnalyticsToday {
    online_now: u64,
    unique_visitors: u64,
    visits: u64,
}

#[derive(Debug, Clone, Serialize)]
struct PublicAnalyticsWindow {
    unique_visitors: u64,
    visits: u64,
}

#[derive(Debug, Clone, Serialize)]
struct PublicAnalyticsAllTime {
    visits: u64,
}

#[derive(Debug)]
pub(super) struct AnalyticsRuntime {
    store: JsonStore<AnalyticsDisk>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AnalyticsDisk {
    #[serde(default)]
    all_time: AnalyticsAllTime,
    #[serde(default)]
    days: BTreeMap<String, AnalyticsDay>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AnalyticsAllTime {
    #[serde(default)]
    pageviews: u64,
    #[serde(default)]
    visits: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AnalyticsDay {
    #[serde(default)]
    pageviews: u64,
    #[serde(default)]
    visits: u64,
    #[serde(default)]
    unique_visitors: u64,
}

pub(super) fn load_initial_runtime(path: &Path) -> AnalyticsRuntime {
    AnalyticsRuntime {
        store: JsonStore::load(path.to_path_buf(), "analytics"),
    }
}

impl AppState {
    pub(crate) async fn record_analytics_event(
        &self,
        event: AnalyticsEventKind,
        peer_addr: Option<SocketAddr>,
        headers: &HeaderMap,
    ) {
        if is_bot_request(headers) {
            return;
        }

        let Some(peer_addr) = peer_addr else {
            return;
        };

        // The visitor store decides what the event means, so the summary and
        // the per-address table always agree on what a visit and a visitor are.
        let visit = self.record_visitor(peer_addr.ip()).await;
        let counts_pageview = event == AnalyticsEventKind::PageOpen;
        if !visit.starts_visit && !counts_pageview {
            return;
        }

        let now = now_seconds();
        let today = day_string(day_index(now));

        let snapshot = {
            let mut runtime = self.analytics.lock().await;
            {
                let day = runtime.store.get_mut().days.entry(today).or_default();
                if visit.first_visit_today {
                    day.unique_visitors = day.unique_visitors.saturating_add(1);
                }
                if visit.starts_visit {
                    day.visits = day.visits.saturating_add(1);
                }
                if counts_pageview {
                    day.pageviews = day.pageviews.saturating_add(1);
                }
            }

            {
                let all_time = &mut runtime.store.get_mut().all_time;
                if visit.starts_visit {
                    all_time.visits = all_time.visits.saturating_add(1);
                }
                if counts_pageview {
                    all_time.pageviews = all_time.pageviews.saturating_add(1);
                }
            }

            runtime.store.take_snapshot(now, 0)
        };

        if let Some(snapshot) = snapshot {
            snapshot.write().await;
        }
    }

    pub(crate) async fn public_analytics(&self) -> PublicAnalytics {
        let now = now_seconds();
        let today_index = day_index(now);
        let today_key = day_string(today_index);
        let online_now = self.visitors_online().await;
        let runtime = self.analytics.lock().await;
        let today = runtime
            .store
            .get()
            .days
            .get(&today_key)
            .cloned()
            .unwrap_or_default();
        PublicAnalytics {
            today: PublicAnalyticsToday {
                online_now,
                unique_visitors: today.unique_visitors,
                visits: today.visits,
            },
            last_7_days: analytics_window(runtime.store.get(), today_index, 7),
            last_30_days: analytics_window(runtime.store.get(), today_index, 30),
            all_time: PublicAnalyticsAllTime {
                visits: runtime.store.get().all_time.visits,
            },
        }
    }
}

fn is_bot_request(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let lower = value.to_ascii_lowercase();
            [
                "bot",
                "crawler",
                "spider",
                "preview",
                "facebookexternalhit",
                "slackbot",
                "discordbot",
                "telegrambot",
                "whatsapp",
            ]
            .iter()
            .any(|needle| lower.contains(needle))
        })
        .unwrap_or(false)
}

fn analytics_window(disk: &AnalyticsDisk, today_index: i64, days: i64) -> PublicAnalyticsWindow {
    let first_day = today_index.saturating_sub(days.saturating_sub(1));
    let mut window = PublicAnalyticsWindow {
        unique_visitors: 0,
        visits: 0,
    };

    for (day, stats) in &disk.days {
        if parse_day_index(day).is_some_and(|day_index| day_index >= first_day) {
            window.unique_visitors = window.unique_visitors.saturating_add(stats.unique_visitors);
            window.visits = window.visits.saturating_add(stats.visits);
        }
    }

    window
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn known_crawlers_are_not_counted_as_traffic() {
        for agent in [
            "Mozilla/5.0 (compatible; Googlebot/2.1)",
            "Slackbot-LinkExpanding 1.0",
            "TelegramBot (like TwitterBot)",
            "WhatsApp/2.23",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(
                axum::http::header::USER_AGENT,
                HeaderValue::from_str(agent).unwrap(),
            );
            assert!(is_bot_request(&headers), "{agent} should be filtered out");
        }

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 Firefox/127.0"),
        );
        assert!(!is_bot_request(&headers));
        assert!(!is_bot_request(&HeaderMap::new()));
    }

    #[test]
    fn windows_add_up_the_days_they_cover() {
        let mut disk = AnalyticsDisk::default();
        let today = day_index(now_seconds());
        for (offset, visits) in [(0, 3), (2, 5), (10, 7), (40, 100)] {
            disk.days.insert(
                day_string(today - offset),
                AnalyticsDay {
                    pageviews: visits,
                    visits,
                    unique_visitors: 1,
                },
            );
        }

        assert_eq!(analytics_window(&disk, today, 7).visits, 8);
        assert_eq!(analytics_window(&disk, today, 30).visits, 15);
        assert_eq!(analytics_window(&disk, today, 30).unique_visitors, 3);
    }
}
