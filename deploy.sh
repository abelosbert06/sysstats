#!/bin/bash
set -e

echo "========================================"
echo "sys_stats Cloudflare Pages Deployer"
echo "========================================"

echo "Building WebAssembly frontend..."
cd frontend-app
~/.cargo/bin/dx build --platform web --release

echo "Deploying to Cloudflare Pages Production..."
npx wrangler pages deploy ../target/dx/frontend-app/release/web/public --project-name sysstats-ui --branch=main

echo "========================================"
echo "Deployment Complete!"
echo "Your dashboard is now live on the Edge."
echo "========================================"
