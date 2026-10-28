//! `obscura carousell`: search Carousell and print listings as JSON or a table.

use std::sync::Arc;

use obscura_browser::{BrowserContext, Page};
use serde::{Deserialize, Serialize};
use tokio::time::{timeout, Duration};

const EXTRACT_JS: &str = include_str!("carousell.js");

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Eq)]
pub enum Region {
    Sg,
    My,
    Hk,
    Ph,
    Tw,
}

impl Region {
    fn origin(self) -> &'static str {
        match self {
            Region::Sg => "https://www.carousell.sg",
            Region::My => "https://www.carousell.com.my",
            Region::Hk => "https://www.carousell.com.hk",
            Region::Ph => "https://www.carousell.ph",
            Region::Tw => "https://tw.carousell.com",
        }
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Eq)]
pub enum Sort {
    /// Carousell's default ranking.
    Best,
    Recent,
    PriceLow,
    PriceHigh,
}

impl Sort {
    fn param(self) -> Option<&'static str> {
        match self {
            Sort::Best => None,
            Sort::Recent => Some("3"),
            Sort::PriceLow => Some("4"),
            Sort::PriceHigh => Some("5"),
        }
    }
}

#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Jsonl,
    Table,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Listing {
    pub id: Option<String>,
    pub title: Option<String>,
    pub price: Option<String>,
    pub original_price: Option<String>,
    pub condition: Option<String>,
    pub seller: Option<String>,
    pub posted: Option<String>,
    pub url: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Extracted {
    #[serde(default)]
    blocked: bool,
    #[serde(default)]
    title: String,
    #[serde(default)]
    listings: Vec<Listing>,
}

pub struct SearchOptions {
    pub query: String,
    pub region: Region,
    pub sort: Sort,
    pub min_price: Option<u64>,
    pub max_price: Option<u64>,
    pub limit: usize,
    pub format: OutputFormat,
    pub output: Option<std::path::PathBuf>,
    pub timeout_secs: u64,
    pub wait_secs: u64,
    pub quiet: bool,
}

/// Builds the search URL, e.g. https://www.carousell.sg/search/iphone%2015?sort_by=3
pub fn search_url(
    region: Region,
    query: &str,
    sort: Sort,
    min_price: Option<u64>,
    max_price: Option<u64>,
) -> anyhow::Result<url::Url> {
    let query = query.trim();
    if query.is_empty() {
        anyhow::bail!("Search query is empty.");
    }
    let mut url = url::Url::parse(region.origin())?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid Carousell origin"))?
        .push("search")
        .push(query);
    {
        let mut qp = url.query_pairs_mut();
        if let Some(s) = sort.param() {
            qp.append_pair("sort_by", s);
        }
        if let Some(min) = min_price {
            qp.append_pair("price_start", &min.to_string());
        }
        if let Some(max) = max_price {
            qp.append_pair("price_end", &max.to_string());
        }
    }
    if url.query() == Some("") {
        url.set_query(None);
    }
    Ok(url)
}

pub async fn run(
    opts: SearchOptions,
    proxy: Option<String>,
    stealth: bool,
    user_agent: Option<String>,
    storage_dir: Option<std::path::PathBuf>,
    obey_robots: bool,
) -> anyhow::Result<()> {
    if let (Some(min), Some(max)) = (opts.min_price, opts.max_price) {
        if min > max {
            anyhow::bail!("--min-price ({}) is greater than --max-price ({}).", min, max);
        }
    }
    let url = search_url(opts.region, &opts.query, opts.sort, opts.min_price, opts.max_price)?;

    let mut context = BrowserContext::with_storage_and_network(
        "carousell".to_string(),
        proxy,
        stealth,
        user_agent.clone(),
        storage_dir,
        false,
    );
    context.obey_robots = obey_robots;
    let context = Arc::new(context);
    let mut page = Page::new("carousell-page".to_string(), context.clone());
    page.set_navigation_timeout(Duration::from_secs(opts.timeout_secs));
    if let Some(ref ua) = user_agent {
        page.http_client.set_user_agent(ua).await;
    }

    // Same backstop as `fetch`: a synchronous hang under a Rust op cannot be
    // cancelled by tokio, so force-exit if the whole run overruns.
    {
        let hard = Duration::from_secs(
            opts.timeout_secs
                .saturating_mul(2)
                .saturating_add(opts.wait_secs)
                .saturating_add(10),
        );
        std::thread::spawn(move || {
            std::thread::sleep(hard);
            eprintln!("obscura: hard timeout exceeded ({}s); forcing exit", hard.as_secs());
            std::process::exit(124);
        });
    }

    if !opts.quiet {
        eprintln!("Searching {}...", url);
    }

    match timeout(
        Duration::from_secs(opts.timeout_secs),
        page.navigate_with_wait(url.as_str(), obscura_browser::lifecycle::WaitUntil::Load),
    )
    .await
    {
        Ok(result) => result.map_err(|e| anyhow::anyhow!("Failed to load {}: {}", url, e))?,
        Err(_) => anyhow::bail!("Timed out loading {} after {}s", url, opts.timeout_secs),
    }

    // Results are rendered client side; poll until cards show up, the page
    // looks like a challenge, or the wait budget runs out.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(opts.wait_secs);
    let mut extracted = extract(&mut page, opts.timeout_secs)?;
    while extracted.listings.is_empty() && tokio::time::Instant::now() < deadline {
        page.settle(500).await;
        extracted = extract(&mut page, opts.timeout_secs)?;
    }
    context.save_storage();

    if extracted.listings.is_empty() {
        if extracted.blocked {
            anyhow::bail!(
                "Carousell served a bot challenge (\"{}\") instead of results. \
                 Try --stealth, a residential network, or --proxy.",
                extracted.title
            );
        }
        if !opts.quiet {
            eprintln!("No listings found (page title: \"{}\").", extracted.title);
        }
    }

    let mut listings = extracted.listings;
    listings.truncate(opts.limit);
    if !opts.quiet {
        eprintln!("Found {} listing(s).", listings.len());
    }

    let rendered = format_listings(&listings, opts.format);
    if let Some(path) = opts.output {
        tokio::fs::write(&path, rendered)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to write {}: {}", path.display(), e))?;
    } else {
        println!("{}", rendered);
    }
    Ok(())
}

fn extract(page: &mut Page, timeout_secs: u64) -> anyhow::Result<Extracted> {
    let value = page.evaluate_with_timeout(EXTRACT_JS, Duration::from_secs(timeout_secs));
    parse_extracted(&value)
}

fn parse_extracted(value: &serde_json::Value) -> anyhow::Result<Extracted> {
    match value {
        serde_json::Value::String(s) => serde_json::from_str(s)
            .map_err(|e| anyhow::anyhow!("Could not parse listings from page: {}", e)),
        other => anyhow::bail!("Listing extraction returned unexpected value: {}", other),
    }
}

pub fn format_listings(listings: &[Listing], format: OutputFormat) -> String {
    match format {
        OutputFormat::Json => {
            serde_json::to_string_pretty(listings).unwrap_or_else(|_| "[]".to_string())
        }
        OutputFormat::Jsonl => listings
            .iter()
            .filter_map(|l| serde_json::to_string(l).ok())
            .collect::<Vec<_>>()
            .join("\n"),
        OutputFormat::Table => {
            let mut out = String::new();
            for l in listings {
                let title = truncate_chars(l.title.as_deref().unwrap_or("-"), 60);
                out.push_str(&format!(
                    "{:<12} {:<60}  {:<14} {}\n",
                    l.price.as_deref().unwrap_or("-"),
                    title,
                    l.condition.as_deref().unwrap_or("-"),
                    l.url.as_deref().unwrap_or("-"),
                ));
            }
            out.trim_end().to_string()
        }
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_basic_search_url() {
        let url = search_url(Region::Sg, "iphone 15", Sort::Best, None, None).unwrap();
        assert_eq!(url.as_str(), "https://www.carousell.sg/search/iphone%2015");
    }

    #[test]
    fn builds_filtered_search_url() {
        let url = search_url(Region::My, "ps5", Sort::PriceLow, Some(100), Some(900)).unwrap();
        assert_eq!(
            url.as_str(),
            "https://www.carousell.com.my/search/ps5?sort_by=4&price_start=100&price_end=900"
        );
    }

    #[test]
    fn escapes_slashes_in_query() {
        let url = search_url(Region::Hk, "a/b ?c", Sort::Recent, None, None).unwrap();
        assert_eq!(
            url.as_str(),
            "https://www.carousell.com.hk/search/a%2Fb%20%3Fc?sort_by=3"
        );
    }

    #[test]
    fn rejects_empty_query() {
        assert!(search_url(Region::Sg, "   ", Sort::Best, None, None).is_err());
    }

    #[test]
    fn parses_extractor_output() {
        let raw = serde_json::Value::String(
            r#"{"blocked":false,"title":"t","url":"u","listings":[{"id":"1","title":"Cam","price":"S$10","original_price":null,"condition":"Like new","seller":"bob","posted":"1 day ago","url":"https://www.carousell.sg/p/cam-1/","image":null}]}"#
                .to_string(),
        );
        let e = parse_extracted(&raw).unwrap();
        assert!(!e.blocked);
        assert_eq!(e.listings.len(), 1);
        assert_eq!(e.listings[0].price.as_deref(), Some("S$10"));
    }

    #[test]
    fn table_truncates_long_titles() {
        let l = Listing {
            title: Some("x".repeat(100)),
            price: Some("S$1".into()),
            ..Default::default()
        };
        let t = format_listings(&[l], OutputFormat::Table);
        assert!(t.contains('…'));
        assert!(t.starts_with("S$1"));
    }
}
