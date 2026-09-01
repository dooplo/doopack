#!/bin/bash
set -e

echo "=========================================="
echo "  Deploying Doopack Orchestrator on VPS"
echo "=========================================="

# 1. Check if docker is installed
if ! command -v docker &> /dev/null; then
    echo "❌ Docker is not installed. Please install Docker first: https://get.docker.com"
    exit 1
fi

# 2. Check if docker compose is available
if ! docker compose version &> /dev/null; then
    echo "❌ Docker Compose v2 is not installed."
    exit 1
fi

# 3. Create .env if it does not exist
if [ ! -f .env ]; then
    echo "📄 Creating .env from .env.example..."
    cp .env.example .env
    # Generate random redis password
    RANDOM_PASS=$(head -c 16 /dev/urandom | xxd -p || openssl rand -hex 16 || echo "doopack_$(date +%s)")
    sed -i.bak "s/doopack_secure_redis_password_change_me/$RANDOM_PASS/g" .env
    rm -f .env.bak
    echo "✅ Created .env with secure random Redis password."
fi

# 4. Build and run containers
echo "🚀 Building and starting Doopack containers..."
docker compose build
docker compose up -d

echo ""
echo "=========================================="
echo "✅ Doopack is running successfully!"
echo "   Access Dashboard: http://localhost:$(grep APP_PORT .env | cut -d '=' -f2 || echo '80')"
echo "=========================================="
