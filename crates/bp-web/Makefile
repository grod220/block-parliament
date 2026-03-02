.PHONY: dev release css clean run run-release sync-remote-data

CSS_INPUT := style/tailwind.css
CSS_OUTPUT := target/site/pkg/bp-web.css
SYNC_SCRIPT := ../../scripts/sync_fly_data.sh
FLY_APP ?= bp-web
REMOTE_DATA_DIR ?= /data
LOCAL_REMOTE_DATA_DIR ?= ../../.fly-data

# Development build
dev: css
	cargo build -p bp-web --features ssr

# Release build
release: css
	cargo build -p bp-web --features ssr --release

# Build CSS with Tailwind v4
css:
	@mkdir -p ../../target/site/pkg
	npx @tailwindcss/cli --input $(CSS_INPUT) --output $(CSS_OUTPUT) --minify

# Run development server
run: dev
	$(SYNC_SCRIPT) --app $(FLY_APP) --remote-dir $(REMOTE_DATA_DIR) --local-dir $(LOCAL_REMOTE_DATA_DIR)
	DATA_DIR=$(LOCAL_REMOTE_DATA_DIR) ../../target/debug/bp-web

# Run release server
run-release: release
	$(SYNC_SCRIPT) --app $(FLY_APP) --remote-dir $(REMOTE_DATA_DIR) --local-dir $(LOCAL_REMOTE_DATA_DIR)
	DATA_DIR=$(LOCAL_REMOTE_DATA_DIR) ../../target/release/bp-web

# Pull remote Fly data files to a local directory
sync-remote-data:
	$(SYNC_SCRIPT) --app $(FLY_APP) --remote-dir $(REMOTE_DATA_DIR) --local-dir $(LOCAL_REMOTE_DATA_DIR)

# Clean build artifacts
clean:
	cargo clean
	rm -f $(CSS_OUTPUT)
