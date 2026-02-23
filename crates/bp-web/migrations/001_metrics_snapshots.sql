-- Stores periodic snapshots of all metrics data fetched from external APIs.
-- bp-web reads the latest row to serve the dashboard; the ingestion job writes new rows daily.
CREATE TABLE IF NOT EXISTS metrics_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    fetched_at TEXT NOT NULL DEFAULT (datetime('now')),
    -- Full MetricsData serialized as JSON (StakewizValidator + Jito + SFDP + NetworkComparison)
    data_json TEXT NOT NULL
);

-- Index for fast "get latest" query
CREATE INDEX IF NOT EXISTS idx_metrics_fetched_at ON metrics_snapshots(fetched_at DESC);

-- Housekeeping metadata
CREATE TABLE IF NOT EXISTS ingestion_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
