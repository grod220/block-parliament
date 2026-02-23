# Deploying Block Parliament to Fly.io

## Architecture

Single Fly Machine running:
- **bp-web** (Leptos SSR web server on port 8080)
- **Background scheduler** (fetches metrics from Stakewiz/Jito/SFDP/Solana RPC every 6 hours)
- **SQLite database** on a Fly Volume at `/data/bp.sqlite`

The web server reads metrics from SQLite instead of making live API calls.
The financial report (`/financials`) is served from `/data/report.html` on the volume.

## Secrets Management

All sensitive values live as **Fly secrets** (environment variables injected at runtime).
Non-secret config (validator addresses, commission, etc.) lives in `/data/config.toml` on the volume.

No API keys touch the filesystem — `config.toml` on the volume contains only public validator info.

## Prerequisites

- [flyctl](https://fly.io/docs/flyctl/install/) installed and authenticated (`fly auth login`)
- Docker installed (for building)

## Initial Setup

### 1. Create the Fly app

```bash
fly apps create bp-web --org personal
```

### 2. Create the volume

```bash
fly volumes create bp_data --region iad --size 1
```

### 3. Set secrets

```bash
fly secrets set \
  FINANCIALS_PASSWORD="your-password" \
  HELIUS_API_KEY="your-helius-key" \
  COINGECKO_API_KEY="your-coingecko-key" \
  DUNE_API_KEY="your-dune-key" \
  NOTION_API_TOKEN="your-notion-token" \
  NOTION_DB_ID="your-notion-db-id"
```

### 4. Upload validator config (no secrets)

Upload the secrets-free config file to the volume. This file contains only public
validator addresses and operational settings — all API keys come from Fly secrets.

```bash
# First deploy to create the machine
fly deploy

# Upload the secrets-free config
echo 'put crates/validator-accounting/config.fly.toml /data/config.toml' | fly ssh sftp shell
```

### 5. Migrate existing data

Upload your local `cache.sqlite` (validator-accounting data) to the volume:

```bash
fly ssh sftp shell
# In the SFTP shell:
put data/cache.sqlite /data/cache.sqlite
```

### 6. Generate the financial report

With the config on the volume and API keys in Fly secrets, just run:

```bash
fly ssh console -C "cd /app && ./validator-accounting --config /data/config.toml"
```

Verify:

```bash
fly ssh console -C "ls -la /data/report.html"
```

## Deployment

```bash
fly deploy
```

The build takes ~10-15 minutes (Rust compilation). Subsequent deploys reuse Docker layer cache.

## Operations

### Check logs

```bash
fly logs
```

### SSH into the machine

```bash
fly ssh console
```

### Verify the database

```bash
fly ssh console -C "sqlite3 /data/bp.sqlite '.tables'"
fly ssh console -C "sqlite3 /data/bp.sqlite 'SELECT fetched_at FROM metrics_snapshots ORDER BY fetched_at DESC LIMIT 5;'"
```

### Run a manual metrics ingestion

```bash
fly ssh console -C "/app/bp-web --data-dir /data --update-now"
```

### Regenerate financial report

```bash
fly ssh console -C "cd /app && ./validator-accounting --config /data/config.toml"
```

No cleanup needed — `/data/config.toml` contains no secrets.

### Check disk usage

```bash
fly ssh console -C "df -h /data && du -sh /data/*"
```

## Configuration

### Fly Secrets (Environment Variables)

| Variable | Description |
|---|---|
| `FINANCIALS_PASSWORD` | Basic auth password for /financials |
| `HELIUS_API_KEY` | Helius RPC API key |
| `COINGECKO_API_KEY` | CoinGecko API key |
| `DUNE_API_KEY` | Dune Analytics API key |
| `NOTION_API_TOKEN` | Notion integration token |
| `NOTION_DB_ID` | Notion hours database ID |

### App Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATA_DIR` | `/data` | Directory for SQLite DB and reports |
| `LEPTOS_SITE_ADDR` | `0.0.0.0:8080` | Web server bind address |
| `LEPTOS_SITE_ROOT` | `target/site` | Static assets directory |
| `INGESTION_INTERVAL_HOURS` | `6` | Hours between automatic metrics fetches |

### Scaling

The default VM is `shared-cpu-1x` with 512MB RAM. If you hit memory issues:

```bash
fly scale memory 1024
```

## Troubleshooting

### "Report not yet generated" on /financials

Run the financial report generation:

```bash
fly ssh console -C "cd /app && ./validator-accounting --config /data/config.toml"
```

The report is stored on the volume and survives deploys.

### Metrics showing "No data available"

Check if the ingestion has run:

```bash
fly ssh console -C "sqlite3 /data/bp.sqlite 'SELECT COUNT(*), MAX(fetched_at) FROM metrics_snapshots;'"
```

If empty, run a manual ingestion:

```bash
fly ssh console -C "/app/bp-web --data-dir /data --update-now"
```

### Database lost after deploy

Make sure the volume is properly mounted. Check with:

```bash
fly volumes list
fly ssh console -C "mount | grep /data"
```

The volume should show as mounted at `/data`. If not, check `fly.toml` mounts configuration.
