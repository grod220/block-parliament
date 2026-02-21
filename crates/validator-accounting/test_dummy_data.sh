#!/usr/bin/env bash
set -euo pipefail

cd /Users/gabe/Desktop/block-parliament
cargo run -p validator-accounting
open output/report.html
