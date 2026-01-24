//! HTTP client for SSR
//! Uses reqwest on server with connection pooling and caching.
//! All data fetching uses server functions, so no client-side HTTP is needed.

#[cfg(feature = "ssr")]
mod ssr {
    use serde::de::DeserializeOwned;
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::time::{Duration, Instant};

    /// Shared HTTP client for connection pooling
    static HTTP_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

    /// Simple in-memory cache with TTL
    static CACHE: std::sync::OnceLock<RwLock<HashMap<String, CacheEntry>>> = std::sync::OnceLock::new();

    // Cache configuration
    const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(60); // 1 minute default
    const RPC_CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes for heavy RPC calls
    const SFDP_CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour for SFDP (rarely changes)
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
    const MAX_CACHE_ENTRIES: usize = 50; // Hard limit to prevent DoS

    struct CacheEntry {
        data: String,
        expires_at: Instant,
        inserted_at: Instant, // For LRU eviction
    }

    /// Determine cache TTL based on URL patterns
    fn get_ttl_for_url(url: &str) -> Duration {
        if url.contains("api.mainnet-beta.solana.com") {
            RPC_CACHE_TTL
        } else if url.contains("api.solana.org") && url.contains("sfdp") {
            SFDP_CACHE_TTL
        } else {
            DEFAULT_CACHE_TTL
        }
    }

    fn get_client() -> &'static reqwest::Client {
        HTTP_CLIENT.get_or_init(|| {
            reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .pool_max_idle_per_host(5)
                .build()
                .expect("failed to create HTTP client")
        })
    }

    fn get_cache() -> &'static RwLock<HashMap<String, CacheEntry>> {
        CACHE.get_or_init(|| RwLock::new(HashMap::new()))
    }

    fn get_cached(url: &str) -> Option<String> {
        let cache = get_cache().read().ok()?;
        let entry = cache.get(url)?;
        if entry.expires_at > Instant::now() {
            Some(entry.data.clone())
        } else {
            None
        }
    }

    fn set_cached(url: &str, data: String, ttl: Duration) {
        if let Ok(mut cache) = get_cache().write() {
            let now = Instant::now();

            // Remove expired entries first
            cache.retain(|_, v| v.expires_at > now);

            // If still over limit, evict oldest entries (LRU)
            while cache.len() >= MAX_CACHE_ENTRIES {
                if let Some(oldest_key) = cache.iter().min_by_key(|(_, v)| v.inserted_at).map(|(k, _)| k.clone()) {
                    cache.remove(&oldest_key);
                } else {
                    break;
                }
            }

            cache.insert(
                url.to_string(),
                CacheEntry {
                    data,
                    expires_at: now + ttl,
                    inserted_at: now,
                },
            );
        }
    }

    pub async fn get_json<T: DeserializeOwned>(url: &str) -> Option<T> {
        // Check cache first
        if let Some(cached) = get_cached(url) {
            return serde_json::from_str(&cached).ok();
        }

        let response = get_client()
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| eprintln!("HTTP request failed for {}: {}", url, e))
            .ok()?;

        if !response.status().is_success() {
            eprintln!("HTTP error for {}: {}", url, response.status());
            return None;
        }

        let text = response
            .text()
            .await
            .map_err(|e| eprintln!("Failed to read response body: {}", e))
            .ok()?;

        // Parse JSON first - only cache if parsing succeeds
        let parsed: T = serde_json::from_str(&text)
            .map_err(|e| eprintln!("JSON parse error for {}: {}", url, e))
            .ok()?;

        // Cache only after successful parse
        let ttl = get_ttl_for_url(url);
        set_cached(url, text, ttl);

        Some(parsed)
    }

    pub async fn get_text(url: &str) -> Option<String> {
        // Check cache first
        if let Some(cached) = get_cached(url) {
            return Some(cached);
        }

        let response = get_client()
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| eprintln!("HTTP request failed for {}: {}", url, e))
            .ok()?;

        if !response.status().is_success() {
            eprintln!("HTTP error for {}: {}", url, response.status());
            return None;
        }

        let text = response
            .text()
            .await
            .map_err(|e| eprintln!("Failed to read response body: {}", e))
            .ok()?;

        // Basic validation: don't cache HTML error pages
        if text.starts_with("<!DOCTYPE") || text.starts_with("<html") {
            eprintln!("Received HTML instead of JSON for {}", url);
            return None;
        }

        // Cache the response
        let ttl = get_ttl_for_url(url);
        set_cached(url, text.clone(), ttl);

        Some(text)
    }

    pub async fn post_json<T: DeserializeOwned>(url: &str, body: &str) -> Option<T> {
        let response = get_client()
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| eprintln!("HTTP POST failed for {}: {}", url, e))
            .ok()?;

        if !response.status().is_success() {
            eprintln!("HTTP error for {}: {}", url, response.status());
            return None;
        }

        let text = response
            .text()
            .await
            .map_err(|e| eprintln!("Failed to read response body: {}", e))
            .ok()?;

        // Parse and cache POST responses (they're idempotent RPC calls)
        let parsed: T = serde_json::from_str(&text)
            .map_err(|e| eprintln!("JSON parse error for {}: {}", url, e))
            .ok()?;

        // Cache RPC POST responses
        let cache_key = format!("{}:{}", url, body);
        let ttl = get_ttl_for_url(url);
        set_cached(&cache_key, text, ttl);

        Some(parsed)
    }

    /// Check POST cache (for RPC calls)
    pub async fn post_json_cached<T: DeserializeOwned>(url: &str, body: &str) -> Option<T> {
        let cache_key = format!("{}:{}", url, body);

        // Check cache first
        if let Some(cached) = get_cached(&cache_key) {
            return serde_json::from_str(&cached).ok();
        }

        post_json(url, body).await
    }
}

#[cfg(feature = "ssr")]
pub use ssr::*;
