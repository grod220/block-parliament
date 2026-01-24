//! HTTP client abstraction for SSR and client-side
//! Uses reqwest on server (with connection pooling + caching), gloo-net on client

use serde::de::DeserializeOwned;

#[cfg(feature = "ssr")]
mod ssr {
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::time::{Duration, Instant};

    /// Shared HTTP client for connection pooling
    static HTTP_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

    /// Simple in-memory cache with TTL
    static CACHE: std::sync::OnceLock<RwLock<HashMap<String, CacheEntry>>> = std::sync::OnceLock::new();

    const CACHE_TTL: Duration = Duration::from_secs(60); // 1 minute cache
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(10); // 10 second timeout

    struct CacheEntry {
        data: String,
        expires_at: Instant,
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

    fn set_cached(url: &str, data: String) {
        if let Ok(mut cache) = get_cache().write() {
            // Clean up expired entries occasionally
            if cache.len() > 100 {
                let now = Instant::now();
                cache.retain(|_, v| v.expires_at > now);
            }

            cache.insert(
                url.to_string(),
                CacheEntry {
                    data,
                    expires_at: Instant::now() + CACHE_TTL,
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

        // Cache the raw response
        set_cached(url, text.clone());

        serde_json::from_str(&text)
            .map_err(|e| eprintln!("JSON parse error for {}: {}", url, e))
            .ok()
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

        // Cache the response
        set_cached(url, text.clone());

        Some(text)
    }

    pub async fn post_json<T: DeserializeOwned>(url: &str, body: &str) -> Option<T> {
        // POST requests are not cached
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

        response
            .json()
            .await
            .map_err(|e| eprintln!("JSON parse error for {}: {}", url, e))
            .ok()
    }
}

#[cfg(feature = "ssr")]
pub use ssr::*;

#[cfg(feature = "hydrate")]
pub async fn get_json<T: DeserializeOwned>(url: &str) -> Option<T> {
    let response = gloo_net::http::Request::get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    if !response.ok() {
        web_sys::console::error_1(&format!("HTTP error: {}", response.status()).into());
        return None;
    }

    response.json().await.ok()
}

#[cfg(feature = "hydrate")]
pub async fn get_text(url: &str) -> Option<String> {
    let response = gloo_net::http::Request::get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;

    if !response.ok() {
        web_sys::console::error_1(&format!("HTTP error: {}", response.status()).into());
        return None;
    }

    response.text().await.ok()
}

#[cfg(feature = "hydrate")]
pub async fn post_json<T: DeserializeOwned>(url: &str, body: &str) -> Option<T> {
    let response = gloo_net::http::Request::post(url)
        .header("Content-Type", "application/json")
        .body(body)
        .ok()?
        .send()
        .await
        .ok()?;

    if !response.ok() {
        web_sys::console::error_1(&format!("HTTP error: {}", response.status()).into());
        return None;
    }

    response.json().await.ok()
}

// Fallback for when neither feature is enabled (cargo check)
#[cfg(not(any(feature = "ssr", feature = "hydrate")))]
pub async fn get_json<T: DeserializeOwned>(_url: &str) -> Option<T> {
    None
}

#[cfg(not(any(feature = "ssr", feature = "hydrate")))]
pub async fn get_text(_url: &str) -> Option<String> {
    None
}

#[cfg(not(any(feature = "ssr", feature = "hydrate")))]
pub async fn post_json<T: DeserializeOwned>(_url: &str, _body: &str) -> Option<T> {
    None
}
