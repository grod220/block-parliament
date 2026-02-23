//! SQLite database access for bp-web.
//! Manages the metrics snapshot table and provides read/write helpers.

#[cfg(feature = "ssr")]
mod ssr {
    use sqlx::SqlitePool;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::OnceLock;

    static DB_POOL: OnceLock<SqlitePool> = OnceLock::new();

    /// Initialize the database pool and run migrations.
    /// Must be called once at startup before any queries.
    pub async fn init_db(data_dir: &str) -> Result<(), sqlx::Error> {
        std::fs::create_dir_all(data_dir).ok();
        let db_path = format!("{}/bp.sqlite", data_dir);
        let url = format!("sqlite:{}?mode=rwc", db_path);

        let pool = SqlitePoolOptions::new().max_connections(5).connect(&url).await?;

        // Run embedded migrations
        sqlx::migrate!().run(&pool).await?;

        DB_POOL
            .set(pool)
            .map_err(|_| sqlx::Error::Configuration("DB pool already initialized".into()))?;

        println!("Database initialized at {}", db_path);
        Ok(())
    }

    /// Get a reference to the database pool.
    /// Panics if called before init_db.
    pub fn pool() -> &'static SqlitePool {
        DB_POOL.get().expect("Database not initialized — call init_db first")
    }

    /// Save a metrics snapshot (serialized MetricsData JSON).
    pub async fn save_metrics_snapshot(data_json: &str) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO metrics_snapshots (data_json) VALUES (?)")
            .bind(data_json)
            .execute(pool())
            .await?;

        // Keep only the 30 most recent snapshots to avoid unbounded growth
        sqlx::query(
            "DELETE FROM metrics_snapshots WHERE id NOT IN (SELECT id FROM metrics_snapshots ORDER BY fetched_at DESC LIMIT 30)"
        )
        .execute(pool())
        .await?;

        Ok(())
    }

    /// Read the latest metrics snapshot JSON and its timestamp.
    pub async fn get_latest_metrics() -> Result<Option<(String, String)>, sqlx::Error> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT data_json, fetched_at FROM metrics_snapshots ORDER BY fetched_at DESC LIMIT 1")
                .fetch_optional(pool())
                .await?;

        Ok(row)
    }

    /// Set a metadata key-value pair.
    pub async fn set_metadata(key: &str, value: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO ingestion_metadata (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(pool())
        .await?;
        Ok(())
    }
}

#[cfg(feature = "ssr")]
pub use ssr::*;
