#!/bin/bash
set -e

# Define paths
CADDY_DIR="./caddy"
PWA_DIR="./PWA/dist"
CADDYFILE="$CADDY_DIR/Caddyfile"

# Create directories
mkdir -p "$CADDY_DIR"
mkdir -p "$PWA_DIR"

# Install Caddy (Linux)
if ! command -v caddy &> /dev/null; then
  echo "Installing Caddy..."
  sudo apt update && sudo apt install -y debian-keyring debian-archive-keyring curl
  curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
  curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | \
    sudo tee /etc/apt/sources.list.d/caddy-stable.list
  sudo apt update
  sudo apt install caddy
fi

# Create Caddyfile
cat > "$CADDYFILE" <<EOF
localhost:8843 {
  handle_path /net/* {
    root * $PWD/$PWA_DIR
    file_server
  }
  tls internal
}
EOF

echo "✅ Caddyfile created at $CADDYFILE"
echo "📂 Serving /net/ from $PWD/$PWA_DIR"
echo "🚀 Run with: caddy run --config $CADDYFILE"

