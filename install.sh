#!/bin/bash
set -e

# sys_stats Agent Installation Script
echo "========================================"
echo "sys_stats Agent Installer"
echo "========================================"

if [ -z "$1" ] || [ "$1" != "--token" ] || [ -z "$2" ]; then
    echo "Usage: curl -sL https://sysstats.com/install.sh | bash -s -- --token YOUR_DEVICE_TOKEN"
    exit 1
fi

TOKEN=$2
INSTALL_DIR="/opt/sys_stats"
BIN_PATH="$INSTALL_DIR/sys-stats-agent"

echo "[1/4] Creating installation directory at $INSTALL_DIR..."
sudo mkdir -p $INSTALL_DIR

echo "[2/4] Downloading agent binary..."
# Note: Replace this URL with the actual release URL (e.g., GitHub Releases or Cloudflare R2)
DOWNLOAD_URL="https://github.com/abelosbert/sys_stats/releases/latest/download/sys-stats-agent-linux-amd64"
# For now, this is a placeholder. If building from source:
# echo "Installing Rust and compiling from source..."
# curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# cargo install --git https://github.com/abelosbert/sys_stats sys-stats-agent
# cp ~/.cargo/bin/sys-stats-agent $BIN_PATH

# Placeholder download command:
# sudo curl -sL $DOWNLOAD_URL -o $BIN_PATH
# sudo chmod +x $BIN_PATH

echo "[3/4] Configuring environment variables..."
sudo bash -c "echo 'SYS_STATS_TOKEN=$TOKEN' > $INSTALL_DIR/.env"

echo "[4/4] Setting up systemd service..."
SERVICE_FILE="/etc/systemd/system/sys-stats-agent.service"
sudo bash -c "cat > $SERVICE_FILE << EOF
[Unit]
Description=Sys Stats Telemetry Agent
After=network.target

[Service]
Type=simple
ExecStart=$BIN_PATH --headless
EnvironmentFile=$INSTALL_DIR/.env
Restart=always
RestartSec=5
User=root

[Install]
WantedBy=multi-user.target
EOF"

echo "Reloading systemd daemon..."
sudo systemctl daemon-reload
echo "Enabling service to start on boot..."
sudo systemctl enable sys-stats-agent
echo "Starting service..."
# sudo systemctl start sys-stats-agent

echo "========================================"
echo "Installation Complete!"
echo "The agent is now running silently in the background."
echo "You can check its status with: sudo systemctl status sys-stats-agent"
echo "View your dashboard at https://sysstats.com"
echo "========================================"
