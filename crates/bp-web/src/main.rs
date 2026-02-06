#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use axum::Router;
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use tower_http::compression::CompressionLayer;
    use tower_http::services::ServeDir;

    let conf = get_configuration(None).map_err(|e| {
        eprintln!("Failed to load Leptos configuration: {}", e);
        e
    })?;
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(bp_web::app::App);

    let site_root = leptos_options.site_root.clone();
    let app = Router::new()
        .leptos_routes(&leptos_options, routes, {
            move || {
                use bp_web::app::App;
                view! {
                    <!DOCTYPE html>
                    <html lang="en">
                        <head>
                            <meta charset="utf-8" />
                            <meta name="viewport" content="width=device-width, initial-scale=1" />
                            <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🦉</text></svg>" />
                            <meta name="theme-color" content="#f8f6f1" media="(prefers-color-scheme: light)" />
                            <meta name="theme-color" content="#1a1a1a" media="(prefers-color-scheme: dark)" />
                            <meta name="description" content="Block Parliament - Solana validator operated by an Anza core developer. 5% commission, Jito MEV enabled." />
                            <title>"Block Parliament - Anza Core Dev Validator"</title>
                            <link rel="stylesheet" href="/pkg/bp-web.css" />
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
