//! Smoke test: run the full financials pipeline against local data.
//!
//! Requires: ./data/cache.sqlite and ./data/config.toml

#[cfg(feature = "ssr")]
#[tokio::test]
async fn generate_report_produces_valid_html() {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());

    // Check data exists
    let cache_path = format!("{}/cache.sqlite", data_dir);
    let config_path = format!("{}/config.toml", data_dir);
    if !std::path::Path::new(&cache_path).exists() || !std::path::Path::new(&config_path).exists() {
        eprintln!("Skipping smoke test — missing {} or {}", cache_path, config_path);
        return;
    }

    let html = bp_web::financials::generate_report(&data_dir).await;

    // Basic assertions
    assert!(
        html.contains("<!DOCTYPE html>") || html.contains("<!doctype html>"),
        "Should produce valid HTML"
    );
    assert!(
        !html.contains("__TIMELINE_JSON__"),
        "Timeline JSON placeholder should be replaced"
    );
    assert!(
        !html.contains("__TAX_TIMELINE_JSON__"),
        "Tax timeline JSON placeholder should be replaced"
    );
    assert!(
        !html.contains("__TAX_YEAR__"),
        "Tax year placeholder should be replaced"
    );
    assert!(
        html.contains("cumulative_profit_usd"),
        "Should contain timeline data with cumulative fields"
    );
    assert!(
        html.contains("rel=\"icon\"") && html.contains("/logo/owl-64.png"),
        "Should include financials favicon"
    );

    // Sanity: the HTML should be reasonably large (template is 3000+ lines)
    assert!(
        html.len() > 10_000,
        "HTML should be substantial, got {} bytes",
        html.len()
    );

    println!("✓ Generated {} bytes of HTML", html.len());

    // Extract the timeline JSON and verify it has events.
    // Template injects: `const TIMELINE = __TIMELINE_JSON__;`
    let marker = "const TIMELINE = ";
    if let Some(start) = html.find(marker) {
        let rest = &html[start + marker.len()..];
        if let Some(end) = rest.find(";\n") {
            let json = &rest[..end];
            let events: Vec<serde_json::Value> = serde_json::from_str(json).expect("Timeline JSON should be valid");
            println!("✓ Operating timeline: {} events", events.len());
            assert!(!events.is_empty(), "Should have timeline events");

            // Check cumulative totals on the last event
            let last = events.last().unwrap();
            let profit = last["cumulative_profit_usd"].as_f64().unwrap();
            let revenue = last["cumulative_revenue_usd"].as_f64().unwrap();
            let expenses = last["cumulative_expenses_usd"].as_f64().unwrap();
            println!(
                "  Final totals: profit=${:.2}, revenue=${:.2}, expenses=${:.2}",
                profit, revenue, expenses
            );
            assert!(revenue > 0.0, "Should have positive revenue");

            // Business started in Nov 2025: there should be no earlier entries.
            let min_date = events
                .iter()
                .filter_map(|ev| ev["date"].as_str())
                .filter(|d| d.chars().nth(4) == Some('-') && d.chars().nth(7) == Some('-'))
                .min()
                .unwrap_or("9999-12-31");
            println!("  Earliest timeline date: {}", min_date);
            assert!(min_date >= "2025-11-01", "Found pre-business event date: {}", min_date);
        }
    }
}
