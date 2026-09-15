//! Public pages read the existing cache only: crawling must never start an RPC refresh.
use crate::chain::ClockSnapshot;
use crate::config::ChainConfig;
use crate::state::AppState;
use crate::timeutil::{day_index, day_string, now_sec};
use axum::extract::State;
use axum::http::{Uri, header};
use axum::response::{Html, IntoResponse, Response};
use serde_json::json;
use std::fmt::Write;
use std::sync::Arc;

const DASHBOARD: &str = include_str!("../../public/index.html");
const DOCUMENT: &str = include_str!("../../public/content/document.html");
const INFORMATION: &[(&str, &str, &str, &str)] = &[
    (
        "/methodology/",
        "Data sources & methodology",
        "How Validator Clock reads validator sets, election timings, stakes, rewards and node locations, and what the data can and cannot tell you.",
        include_str!("../../public/content/methodology.html"),
    ),
    (
        "/guides/validator-elections/",
        "Understanding validator elections",
        "Learn how to read validator rounds, election windows and the Validator Clock dashboard for TON, Everscale and Tycho Testnet.",
        include_str!("../../public/content/elections.html"),
    ),
    (
        "/about/",
        "About Validator Clock",
        "Validator Clock is a dashboard for validator rounds, elections, stakes, rewards and node distribution, maintained by jouliene.",
        include_str!("../../public/content/about.html"),
    ),
];

pub(super) fn public_base(state: &AppState) -> String {
    let configured = state.config.tls.public_url.trim_end_matches('/');
    if configured.is_empty() {
        format!("http://{}", state.config.listen)
    } else {
        configured.to_owned()
    }
}

/// Only known public pages get slash redirects. Unknown URLs keep their real 404.
pub(super) fn canonical_path(state: &AppState, path: &str) -> Option<String> {
    if path == "/index.html" {
        return Some("/".to_owned());
    }
    let with_slash = format!("{path}/");
    if INFORMATION.iter().any(|(url, ..)| *url == with_slash)
        || state
            .config
            .chains
            .iter()
            .any(|chain| path == format!("/{}", chain.id))
    {
        return Some(with_slash);
    }
    None
}

pub(super) async fn page(State(state): State<Arc<AppState>>, uri: Uri) -> Response {
    let path = uri.path();
    let navigation = navigation(&state);
    let footer = footer();
    if let Some((_, heading, description, article)) =
        INFORMATION.iter().find(|(url, ..)| *url == path)
    {
        let html = DOCUMENT
            .replace(
                "__SEO_HEAD__",
                &head(&state, path, heading, description, false),
            )
            .replace("__PAGE_HEADING__", &escape(heading))
            .replace("__PAGE_DESCRIPTION__", &escape(description))
            .replace("__ARTICLE__", article)
            .replace("__PAGE_NAV__", &navigation)
            .replace("__FOOTER_LINKS__", &footer);
        return Html(super::assets::render_page(&html)).into_response();
    }
    let chain = state
        .config
        .chains
        .iter()
        .find(|chain| path == format!("/{}/", chain.id));
    if path != "/" && chain.is_none() {
        return super::responses::not_found().await;
    }
    let (heading, description) = if let Some(chain) = chain {
        (
            format!("{} Validator Clock", chain.name),
            chain_description(chain),
        )
    } else {
        let names = state
            .config
            .chains
            .iter()
            .map(|chain| chain.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        (
            "Validator Clock".to_owned(),
            format!(
                "Track validator rounds, election windows, stakes, rewards and node distribution for {names}."
            ),
        )
    };
    let title = chain.map_or_else(
        || "TON, Everscale & Tycho Validator Dashboard".to_owned(),
        |chain| format!("{} Validators, Rounds & Elections", chain.name),
    );
    let snapshot = if let Some(chain) = chain {
        network_snapshot(&state, chain, true).await
    } else {
        let mut overview = String::from(
            "<section id=\"network-snapshot\" class=\"content-section\"><h2>Network snapshots</h2><p>Recorded network data at page load. Open a network for its validator table and interactive clock.</p><div class=\"network-overview\">",
        );
        for chain in &state.config.chains {
            overview.push_str(&network_snapshot(&state, chain, false).await);
        }
        overview.push_str("</div></section>");
        overview
    };
    let html = DASHBOARD
        .replace(
            "__SEO_HEAD__",
            &head(&state, path, &title, &description, true),
        )
        .replace("__PAGE_HEADING__", &escape(&heading))
        .replace("__PAGE_DESCRIPTION__", &escape(&description))
        .replace("__PAGE_NAV__", &navigation)
        .replace(
            "__CHAIN_LINKS__",
            &chain_links(&state, chain.map(|chain| chain.id.as_str())),
        )
        .replace(
            "__CHAIN_ID__",
            &escape(chain.map_or("", |chain| chain.id.as_str())),
        )
        .replace("__NETWORK_SNAPSHOT__", &snapshot)
        .replace("__FOOTER_LINKS__", &footer);
    Html(super::assets::render_page(&html)).into_response()
}

fn chain_description(chain: &ChainConfig) -> String {
    match chain.id.as_str() {
        "ton" => "Monitor TON mainnet validator rounds, election windows, stakes and rewards. Explore the active validator set and the distribution of mapped nodes.".to_owned(),
        "everscale" => "Follow Everscale mainnet validator elections, active rounds and EVER stakes. Compare recorded rewards, validator participation and mapped node locations.".to_owned(),
        "tycho-testnet" => "Explore Tycho Testnet validator rounds, election timing and participation. This dashboard shows a test network, not Tycho mainnet.".to_owned(),
        _ => format!("Follow {} validator rounds, election windows, stakes and recorded network data.", chain.name),
    }
}

fn navigation(state: &AppState) -> String {
    let mut html = String::from(
        "<nav class=\"content-nav\" aria-label=\"Site navigation\"><a href=\"/\">Overview</a>",
    );
    for chain in &state.config.chains {
        let _ = write!(
            html,
            "<a href=\"/{}/\">{}</a>",
            escape(&chain.id),
            escape(&chain.name)
        );
    }
    html.push_str("<a href=\"/guides/validator-elections/\">Election guide</a><a href=\"/methodology/\">Methodology</a></nav>");
    html
}

fn chain_links(state: &AppState, selected: Option<&str>) -> String {
    let mut html = String::new();
    for chain in &state.config.chains {
        let current = if selected == Some(chain.id.as_str()) {
            " aria-current=\"page\""
        } else {
            ""
        };
        let _ = write!(
            html,
            "<a class=\"chain-tab\" href=\"/{}/\"{current}>{}</a>",
            escape(&chain.id),
            escape(&chain.name)
        );
    }
    html
}

fn footer() -> String {
    "<nav class=\"content-nav\" aria-label=\"Project information\"><a href=\"/about/\">About</a><a href=\"/methodology/\">Data sources &amp; methodology</a><a href=\"/guides/validator-elections/\">How to read the clock</a><a href=\"https://github.com/jouliene/validatorclock\">GitHub</a></nav>".to_owned()
}

fn head(state: &AppState, path: &str, title: &str, description: &str, dashboard: bool) -> String {
    let base = public_base(state);
    let canonical = format!("{base}{path}");
    let title = format!("{title} | Validator Clock");
    let mut graph = vec![
        json!({
            "@type": "WebSite", "@id": format!("{base}/#website"),
            "name": "Validator Clock", "url": format!("{base}/"), "inLanguage": "en"
        }),
        json!({
            "@type": "WebPage", "@id": canonical, "url": canonical,
            "name": title, "description": description, "inLanguage": "en",
            "isPartOf": {"@id": format!("{base}/#website")}
        }),
    ];
    if dashboard {
        graph.push(json!({
            "@type": "WebApplication", "name": "Validator Clock", "url": canonical,
            "applicationCategory": "UtilitiesApplication", "operatingSystem": "Web browser",
            "description": description, "isAccessibleForFree": true
        }));
    }
    if path != "/" {
        graph.push(json!({"@type": "BreadcrumbList", "itemListElement": [
            {"@type": "ListItem", "position": 1, "name": "Validator Clock", "item": format!("{base}/")},
            {"@type": "ListItem", "position": 2, "name": title, "item": canonical}
        ]}));
    }
    // JSON is an inert data block. Escape '<' so even hostile metadata cannot close it.
    let structured = json!({"@context": "https://schema.org", "@graph": graph})
        .to_string()
        .replace('<', "\\u003c");
    format!(
        r#"<title>{title}</title>
  <meta name="description" content="{description}">
  <link rel="canonical" href="{canonical}">
  <meta property="og:type" content="website">
  <meta property="og:site_name" content="Validator Clock">
  <meta property="og:title" content="{title}">
  <meta property="og:description" content="{description}">
  <meta property="og:url" content="{canonical}">
  <meta property="og:image" content="{base}/social-preview.png?v={version}">
  <meta property="og:image:alt" content="Validator Clock — validator rounds, elections and network data">
  <meta name="twitter:card" content="summary_large_image">
  <script type="application/ld+json">{structured}</script>"#,
        title = escape(&title),
        description = escape(description),
        canonical = escape(&canonical),
        base = escape(&base),
        version = env!("CARGO_PKG_VERSION")
    )
}

async fn network_snapshot(state: &AppState, chain: &ChainConfig, detailed: bool) -> String {
    // Read under the cache lock without copying or enriching validator records.
    state.with_cached_snapshot(&chain.id, |snapshot| snapshot_html(chain, snapshot, state.config.refresh_seconds, detailed))
        .await.unwrap_or_else(|| format!(
            "<section {}class=\"content-section\"><h2><a href=\"/{}/\">{} validators</a></h2><p>Network data is not available yet. The background collector will refresh it; missing values are not zero.</p></section>",
            if detailed { "id=\"network-snapshot\" " } else { "" }, escape(&chain.id), escape(&chain.name)))
}

fn snapshot_html(
    chain: &ChainConfig,
    snapshot: &ClockSnapshot,
    refresh_seconds: u64,
    detailed: bool,
) -> String {
    let set = &snapshot.current_set;
    let observed = snapshot.fetched_at;
    let start = set
        .utime_until
        .saturating_sub(snapshot.params15.elections_start_before);
    let end = set
        .utime_until
        .saturating_sub(snapshot.params15.elections_end_before);
    let phase = if observed < u64::from(start) {
        "Before elections"
    } else if observed < u64::from(end) {
        "Elections open"
    } else {
        "After elections"
    };
    let stale = now_sec().saturating_sub(observed) > refresh_seconds.saturating_mul(2)
        || now_sec() >= u64::from(set.utime_until);
    let freshness = if stale {
        "This snapshot is stale; use the interactive clock for updates."
    } else {
        "Snapshot at page load; the interactive clock refreshes separately."
    };
    let mut html = format!(
        "<section {}class=\"content-section\"><h2><a href=\"/{}/\">{} validator snapshot</a></h2><p>Recorded {}. {freshness}</p><dl class=\"snapshot-metrics\"><div><dt>Round ID</dt><dd>{}</dd></div><div><dt>Validators in the set</dt><dd>{}</dd></div><div><dt>Total stake ({})</dt><dd>{}</dd></div><div><dt>Election phase when recorded</dt><dd>{phase}</dd></div></dl><p>Round: {} – {}.</p><p>Election window for this round: {} – {}.</p>",
        if detailed {
            "id=\"network-snapshot\" "
        } else {
            ""
        },
        escape(&chain.id),
        escape(&chain.name),
        time_html(observed),
        set.round_id,
        set.total,
        escape(&chain.token_symbol),
        escape(set.total_stake.as_deref().unwrap_or("Unavailable")),
        time_html(u64::from(set.utime_since)),
        time_html(u64::from(set.utime_until)),
        time_html(u64::from(start)),
        time_html(u64::from(end))
    );
    if snapshot.warning.is_some() {
        html.push_str("<p>The collector reported a warning for this snapshot. Some data may be incomplete.</p>");
    }
    if detailed {
        html.push_str("<p>Dates in this snapshot use UTC. The interactive clock uses your browser's local time. <a href=\"/guides/validator-elections/\">How to read election timing</a> · <a href=\"/methodology/\">Sources and limitations</a>.</p><details class=\"snapshot-details\"><summary>Recorded validator table</summary><div class=\"snapshot-table-wrap\"><table class=\"snapshot-table\"><caption>Validators in the recorded set. A public key identifies a validator; it is not a wallet address.</caption><thead><tr><th scope=\"col\">Public key</th><th scope=\"col\">Stake</th><th scope=\"col\">Consensus weight</th></tr></thead><tbody>");
        for validator in &set.validators {
            let _ = write!(
                html,
                "<tr><td><code>{}</code></td><td>{} {}</td><td>{:.4}%</td></tr>",
                escape(&validator.public_key),
                escape(validator.stake.as_deref().unwrap_or("Unavailable")),
                if validator.stake.is_some() {
                    escape(&chain.token_symbol)
                } else {
                    String::new()
                },
                validator.weight_percent
            );
        }
        html.push_str("</tbody></table></div></details>");
    }
    html.push_str("</section>");
    html
}

fn time_html(timestamp: u64) -> String {
    let day = day_string(day_index(timestamp));
    let seconds = timestamp % 86_400;
    let clock = format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    );
    format!("<time datetime=\"{day}T{clock}Z\">{day} {clock} UTC</time>")
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(super) async fn robots(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Public rendering APIs and assets stay crawlable. Authentication protects stats.
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!(
            "User-agent: *\nAllow: /\n\nSitemap: {}/sitemap.xml\n",
            public_base(&state)
        ),
    )
}

pub(super) async fn sitemap(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let base = public_base(&state);
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">",
    );
    let paths = std::iter::once("/".to_owned())
        .chain(
            state
                .config
                .chains
                .iter()
                .map(|chain| format!("/{}/", chain.id)),
        )
        .chain(INFORMATION.iter().map(|(path, ..)| (*path).to_owned()));
    for path in paths {
        let _ = write!(
            xml,
            "<url><loc>{}</loc></url>",
            escape(&format!("{base}{path}"))
        );
    }
    // Omit lastmod until we have a reliable modification time for the entire page.
    xml.push_str("</urlset>");
    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        xml,
    )
}

pub(super) async fn social_preview() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_bytes!("../../public/brands/social-preview.png").as_slice(),
    )
}

pub(super) async fn content_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        concat!(
            include_str!("../../public/shared/analytics_client.js"),
            "\n",
            include_str!("../../public/app/analytics.js"),
            "\nstartAnalytics();\n"
        ),
    )
}
