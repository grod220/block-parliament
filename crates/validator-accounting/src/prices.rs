//! Historical SOL/USD price fetching (CoinGecko → Binance → Dune → hardcoded fallback)

use anyhow::Result;
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

use crate::constants;
use crate::dune;
use crate::transactions::{EpochReward, SolTransfer};

/// Price cache mapping date strings to USD prices
pub type PriceCache = HashMap<String, f64>;

/// CoinGecko market chart response
#[derive(Debug, Deserialize)]
struct MarketChartResponse {
    prices: Vec<[f64; 2]>, // [timestamp_ms, price]
}

/// CoinGecko simple price response
#[derive(Debug, Deserialize)]
struct SimplePriceResponse {
    solana: Option<SolanaPrice>,
}

#[derive(Debug, Deserialize)]
struct SolanaPrice {
    usd: f64,
}

/// Fetch historical prices for all dates in rewards and transfers.
/// If `existing_prices` is provided, skip dates that are already cached.
pub async fn fetch_historical_prices(
    rewards: &[EpochReward],
    transfers: &[SolTransfer],
    api_key: &str,
    dune_api_key: Option<&str>,
) -> Result<PriceCache> {
    fetch_historical_prices_with_cache(rewards, transfers, api_key, dune_api_key, None).await
}

/// Fetch historical prices, skipping dates already in `existing_prices`.
pub async fn fetch_historical_prices_with_cache(
    rewards: &[EpochReward],
    transfers: &[SolTransfer],
    api_key: &str,
    dune_api_key: Option<&str>,
    existing_prices: Option<&PriceCache>,
) -> Result<PriceCache> {
    let mut cache = PriceCache::new();

    // Collect all unique dates we need prices for
    let mut date_set = std::collections::HashSet::<NaiveDate>::new();

    for reward in rewards {
        if let Some(date) = &reward.date
            && let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        {
            if existing_prices.is_some_and(|p| p.contains_key(date)) {
                continue;
            }
            date_set.insert(d);
        }
    }

    for transfer in transfers {
        if let Some(date) = &transfer.date
            && let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        {
            if existing_prices.is_some_and(|p| p.contains_key(date)) {
                continue;
            }
            date_set.insert(d);
        }
    }

    let mut dates: Vec<NaiveDate> = date_set.into_iter().collect();

    if dates.is_empty() {
        // No dates to fetch, get current price if not cached
        let today = Utc::now().format("%Y-%m-%d").to_string();
        if existing_prices.is_none_or(|p| !p.contains_key(&today))
            && let Ok(price) = fetch_current_price(api_key).await
        {
            cache.insert(today, price);
        }
        return Ok(cache);
    }

    // Sort dates to find range
    dates.sort();
    let min_date = dates.first().unwrap();
    let max_date = dates.last().unwrap();

    // Fetch historical prices from CoinGecko
    println!("    Fetching prices from {} to {}", min_date, max_date);

    match fetch_price_range(*min_date, *max_date, api_key, dune_api_key).await {
        Ok(prices) => {
            for (date, price) in prices {
                cache.insert(date, price);
            }
        }
        Err(e) => {
            eprintln!("    ⚠️  WARNING: Failed to fetch historical prices: {}", e);
            eprintln!(
                "    ⚠️  Using fallback price of ${:.2} for {} dates",
                constants::FALLBACK_SOL_PRICE,
                dates.len()
            );
            eprintln!("    ⚠️  Financial reports may be inaccurate!");
            // Use fallback price
            for date in &dates {
                cache.insert(date.format("%Y-%m-%d").to_string(), constants::FALLBACK_SOL_PRICE);
            }
        }
    }

    // Ensure current price is available
    if let Ok(price) = fetch_current_price(api_key).await {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        cache.insert(today, price);
    }

    Ok(cache)
}

/// Fetch price range — tries CoinGecko → Binance → Dune → fallback
async fn fetch_price_range(
    from: NaiveDate,
    to: NaiveDate,
    api_key: &str,
    dune_api_key: Option<&str>,
) -> Result<Vec<(String, f64)>> {
    match fetch_price_range_coingecko(from, to, api_key).await {
        Ok(prices) => return Ok(prices),
        Err(cg_err) => {
            eprintln!("    ⚠️  CoinGecko failed ({}), trying Binance...", cg_err);
        }
    }

    match fetch_price_range_binance(from, to).await {
        Ok(prices) => return Ok(prices),
        Err(bn_err) => {
            eprintln!("    ⚠️  Binance failed ({})", bn_err);
        }
    }

    if let Some(dune_key) = dune_api_key {
        eprintln!("    ⚠️  Trying Dune prices.usd...");
        match fetch_price_range_dune(from, to, dune_key).await {
            Ok(prices) => return Ok(prices),
            Err(dune_err) => {
                eprintln!("    ⚠️  Dune price fetch failed ({})", dune_err);
            }
        }
    }

    anyhow::bail!("All price sources failed (CoinGecko, Binance, Dune)")
}

/// Fetch price range from CoinGecko
async fn fetch_price_range_coingecko(from: NaiveDate, to: NaiveDate, api_key: &str) -> Result<Vec<(String, f64)>> {
    let client = reqwest::Client::new();

    let from_ts = from.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
    let to_ts = (to + ChronoDuration::days(1))
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp();

    let url = format!(
        "{}{}&from={}&to={}",
        constants::COINGECKO_API_BASE,
        constants::COINGECKO_MARKET_CHART,
        from_ts,
        to_ts
    );

    let max_retries = 3;
    let mut last_error = None;
    let mut data: Option<MarketChartResponse> = None;

    for attempt in 0..max_retries {
        if attempt > 0 {
            let delay = Duration::from_secs(2u64.pow(attempt as u32));
            sleep(delay).await;
        }

        match client
            .get(&url)
            .header("Accept", "application/json")
            .header("x-cg-demo-api-key", api_key)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<MarketChartResponse>().await {
                        Ok(d) => {
                            data = Some(d);
                            break;
                        }
                        Err(e) => {
                            last_error = Some(anyhow::anyhow!("Parse error: {}", e));
                        }
                    }
                } else if response.status().as_u16() == 429 {
                    last_error = Some(anyhow::anyhow!("Rate limited (429)"));
                    continue;
                } else {
                    last_error = Some(anyhow::anyhow!("CoinGecko API returned status: {}", response.status()));
                }
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("Request failed: {}", e));
            }
        }
    }

    let data =
        data.ok_or_else(|| last_error.unwrap_or_else(|| anyhow::anyhow!("Failed after {} retries", max_retries)))?;

    let mut daily_prices: HashMap<String, f64> = HashMap::new();
    for [timestamp_ms, price] in data.prices {
        let timestamp = timestamp_ms as i64 / 1000;
        if let Some(dt) = chrono::DateTime::from_timestamp(timestamp, 0) {
            daily_prices.insert(dt.format("%Y-%m-%d").to_string(), price);
        }
    }

    Ok(daily_prices.into_iter().collect())
}

/// Fetch price range from Binance (no API key required).
/// Klines endpoint returns up to 1000 daily candles per request.
async fn fetch_price_range_binance(from: NaiveDate, to: NaiveDate) -> Result<Vec<(String, f64)>> {
    let client = reqwest::Client::new();
    let mut all_prices: Vec<(String, f64)> = Vec::new();

    // Paginate in chunks of 1000 days (Binance klines limit)
    let mut cursor = from;
    while cursor <= to {
        let from_ms = cursor.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() * 1000;
        let to_ms = (to + ChronoDuration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp()
            * 1000;

        let url = format!(
            "{}{}&startTime={}&endTime={}&limit=1000",
            constants::BINANCE_API_BASE,
            constants::BINANCE_KLINES,
            from_ms,
            to_ms
        );

        let response = client.get(&url).header("Accept", "application/json").send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Binance API returned status: {}", response.status());
        }

        // Kline format: [open_time, open, high, low, close, volume, close_time, ...]
        // Prices are returned as strings
        let klines: Vec<Vec<serde_json::Value>> = response.json().await?;

        if klines.is_empty() {
            break;
        }

        for kline in &klines {
            if kline.len() < 5 {
                continue;
            }
            let open_time_ms = kline[0].as_i64().unwrap_or(0);
            let close_price = kline[4].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);

            if close_price > 0.0
                && let Some(dt) = chrono::DateTime::from_timestamp(open_time_ms / 1000, 0)
            {
                all_prices.push((dt.format("%Y-%m-%d").to_string(), close_price));
            }
        }

        // Advance cursor past the last kline; stop if fewer than 1000 returned
        if klines.len() < 1000 {
            break;
        }
        if let Some(last) = klines.last() {
            let last_ts = last[0].as_i64().unwrap_or(0) / 1000;
            if let Some(dt) = chrono::DateTime::from_timestamp(last_ts, 0) {
                cursor = dt.date_naive() + ChronoDuration::days(1);
            } else {
                break;
            }
        }
    }

    if all_prices.is_empty() {
        anyhow::bail!("Binance returned no price data");
    }

    println!("    ✓ Binance fallback: fetched {} daily prices", all_prices.len());
    Ok(all_prices)
}

/// Fetch price range from Dune `prices.usd` table (works from cloud IPs).
/// Queries daily average SOL/USD prices for the given date range.
async fn fetch_price_range_dune(from: NaiveDate, to: NaiveDate, dune_api_key: &str) -> Result<Vec<(String, f64)>> {
    let sql = format!(
        r#"
        SELECT
          DATE(minute) as price_date,
          AVG(price) as avg_price
        FROM prices.usd
        WHERE blockchain = 'solana'
          AND symbol = 'SOL'
          AND minute >= TIMESTAMP '{from} 00:00:00'
          AND minute < TIMESTAMP '{to_next} 00:00:00'
        GROUP BY DATE(minute)
        ORDER BY price_date
        "#,
        from = from.format("%Y-%m-%d"),
        to_next = (to + ChronoDuration::days(1)).format("%Y-%m-%d"),
    );

    let rows = dune::execute_sql(dune_api_key, &sql).await?;

    let mut prices: Vec<(String, f64)> = Vec::new();
    for row in &rows {
        let date = row.get("price_date").and_then(|v| v.as_str()).map(|s| s.to_string());
        let price = row.get("avg_price").and_then(|v| v.as_f64());

        if let (Some(d), Some(p)) = (date, price) {
            // Dune may return full timestamps; normalize to YYYY-MM-DD
            let date_str = if d.len() > 10 { d[..10].to_string() } else { d };
            prices.push((date_str, p));
        }
    }

    if prices.is_empty() {
        anyhow::bail!("Dune returned no price data");
    }

    println!("    ✓ Dune fallback: fetched {} daily prices", prices.len());
    Ok(prices)
}

/// Fetch current SOL price — tries CoinGecko first, falls back to Binance
pub async fn fetch_current_price(api_key: &str) -> Result<f64> {
    match fetch_current_price_coingecko(api_key).await {
        Ok(price) => Ok(price),
        Err(_) => fetch_current_price_binance().await,
    }
}

/// Fetch current SOL price from CoinGecko
async fn fetch_current_price_coingecko(api_key: &str) -> Result<f64> {
    let client = reqwest::Client::new();
    let url = format!("{}{}", constants::COINGECKO_API_BASE, constants::COINGECKO_SIMPLE_PRICE);

    let max_retries = 3;
    let mut last_error = None;

    for attempt in 0..max_retries {
        if attempt > 0 {
            let delay = Duration::from_secs(2u64.pow(attempt as u32));
            sleep(delay).await;
        }

        match client
            .get(&url)
            .header("Accept", "application/json")
            .header("x-cg-demo-api-key", api_key)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<SimplePriceResponse>().await {
                        Ok(data) => {
                            return data
                                .solana
                                .map(|s| s.usd)
                                .ok_or_else(|| anyhow::anyhow!("No SOL price in response"));
                        }
                        Err(e) => {
                            last_error = Some(anyhow::anyhow!("Parse error: {}", e));
                        }
                    }
                } else if response.status().as_u16() == 429 {
                    last_error = Some(anyhow::anyhow!("Rate limited (429)"));
                    continue;
                } else {
                    last_error = Some(anyhow::anyhow!("API returned status: {}", response.status()));
                }
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("Request failed: {}", e));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Failed after {} retries", max_retries)))
}

/// Fetch current SOL price from Binance (no API key required)
async fn fetch_current_price_binance() -> Result<f64> {
    let client = reqwest::Client::new();
    let url = format!("{}{}", constants::BINANCE_API_BASE, constants::BINANCE_TICKER);

    let response = client.get(&url).header("Accept", "application/json").send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Binance ticker returned status: {}", response.status());
    }

    // Response: {"symbol":"SOLUSDT","price":"172.50000000"}
    let data: serde_json::Value = response.json().await?;
    data["price"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| anyhow::anyhow!("No price in Binance response"))
}

/// Get price for a specific date from cache, with fallback
pub fn get_price(cache: &PriceCache, date: &str) -> f64 {
    cache.get(date).copied().unwrap_or_else(|| {
        // Try to find closest date
        if let Ok(target) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            let mut closest_price = constants::FALLBACK_SOL_PRICE;
            let mut closest_diff = i64::MAX;

            for (d, p) in cache {
                if let Ok(cached_date) = NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                    let diff = (target - cached_date).num_days().abs();
                    if diff < closest_diff {
                        closest_diff = diff;
                        closest_price = *p;
                    }
                }
            }

            closest_price
        } else {
            constants::FALLBACK_SOL_PRICE
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_price_cache_type() {
        // Basic type check - actual API tests require credentials
        use super::PriceCache;
        let cache: PriceCache = Default::default();
        assert!(cache.is_empty());
    }
}
