/// Dynamic financial report handler.
///
/// Authenticates via Basic Auth (FINANCIALS_PASSWORD env var), then queries
/// cache.sqlite at request time to build an always-fresh HTML report.
#[cfg(feature = "ssr")]
async fn financials_handler(headers: axum::http::HeaderMap) -> axum::response::Response {
    use axum::http::{HeaderName, StatusCode, header};
    use axum::response::IntoResponse;
    use base64::Engine;

    let password = std::env::var("FINANCIALS_PASSWORD").unwrap_or_default();

    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|encoded| base64::engine::general_purpose::STANDARD.decode(encoded).ok())
        .and_then(|decoded| String::from_utf8(decoded).ok())
        .map(|credentials| {
            let pass = credentials.split_once(':').map(|x| x.1).unwrap_or("");
            !password.is_empty() && pass == password
        })
        .unwrap_or(false);

    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            [
                (header::WWW_AUTHENTICATE, "Basic realm=\"Block Parliament Financials\""),
                (header::CACHE_CONTROL, "no-store"),
            ],
            "",
        )
            .into_response();
    }

    // Build report dynamically from cache.sqlite
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    let html = bp_web::financials::generate_report(&data_dir).await;

    let mut response = (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        html,
    )
        .into_response();
    response.headers_mut().insert(
        HeaderName::from_static("x-robots-tag"),
        "noindex, nofollow".parse().unwrap(),
    );
    response
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use axum::Router;
    use clap::Parser;
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use tower_http::compression::CompressionLayer;
    use tower_http::services::ServeDir;

    #[derive(Parser)]
    #[command(name = "bp-web", about = "Block Parliament web server")]
    struct Cli {
        /// Run a single metrics ingestion cycle and exit (no web server)
        #[arg(long)]
        update_now: bool,

        /// Data directory for SQLite database and reports
        #[arg(long, env = "DATA_DIR", default_value = "./data")]
        data_dir: String,
    }

    let cli = Cli::parse();

    // Initialize database
    bp_web::db::init_db(&cli.data_dir).await.map_err(|e| {
        eprintln!("Failed to initialize database: {}", e);
        e
    })?;

    // --update-now: run ingestion once and exit (no web server)
    if cli.update_now {
        println!("Running one-time metrics ingestion...");
        match bp_web::ingestion::run_ingestion().await {
            Ok(true) => println!("Ingestion completed successfully."),
            Ok(false) => eprintln!("Ingestion returned no data."),
            Err(e) => {
                eprintln!("Ingestion failed: {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // Start background scheduler for periodic ingestion
    bp_web::scheduler::spawn_scheduler();

    let conf = get_configuration(None).map_err(|e| {
        eprintln!("Failed to load Leptos configuration: {}", e);
        e
    })?;
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(bp_web::app::App);

    let site_root = leptos_options.site_root.clone();
    let app = Router::new()
        .route("/financials", axum::routing::get(financials_handler))
        .leptos_routes(&leptos_options, routes, {
            move || {
                use bp_web::app::App;
                use leptos_meta::MetaTags;
                view! {
                    <!DOCTYPE html>
                    <html lang="en">
                        <head>
                            <meta charset="utf-8" />
                            <meta name="viewport" content="width=device-width, initial-scale=1" />
                            <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🦉</text></svg>" />
                            <meta name="theme-color" content="#f8f6f1" media="(prefers-color-scheme: light)" />
                            <meta name="theme-color" content="#1a1a1a" media="(prefers-color-scheme: dark)" />
                            <link rel="stylesheet" href="/pkg/bp-web.css" />
                            <MetaTags />
                        </head>
                        <body>
                            <App />
                        </body>
                    </html>
                }
            }
        })
        .fallback_service(ServeDir::new(&*site_root))
        .layer(CompressionLayer::new())
        .with_state(leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
        eprintln!("Failed to bind to {}: {}", addr, e);
        e
    })?;

    println!("Listening on http://{}", addr);

    axum::serve(listener, app).await.map_err(|e| {
        eprintln!("Server error: {}", e);
        e
    })?;

    Ok(())
}

#[cfg(not(feature = "ssr"))]
fn main() {
    // SSR-only: no client-side entry point needed
}
