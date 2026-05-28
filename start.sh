#!/usr/bin/env bash
# Ellen AI Rust Backend — Linux/macOS Startup Script
#
# 1. Checks LLM_API_KEY environment variable
# 2. Verifies GPT-SoVITS TTS service is reachable
# 3. Compiles and runs the backend

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "╔══════════════════════════════════════════════════════════╗"
echo "║      Ellen AI Rust Backend 启动器                       ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

# ── 1. Environment Check ─────────────────────────────────────────────────
echo "[1/3] 检查环境变量…"

if [[ -z "${LLM_API_KEY:-}" ]]; then
    echo "❌ 错误: 未设置 LLM_API_KEY 环境变量"
    echo ""
    echo "   请设置您的 DeepSeek API Key:"
    echo "   export LLM_API_KEY=sk-your-key-here"
    echo ""
    exit 1
fi

echo "✅ LLM_API_KEY 已设置"

# ── 2. TTS Health Check ──────────────────────────────────────────────────
echo ""
echo "[2/3] 检查 TTS 服务 (127.0.0.1:9880)…"

TTS_URL="${TTS_API_URL:-http://127.0.0.1:9880}"
TTS_HOST="$(echo "$TTS_URL" | sed -E 's|https?://||' | cut -d: -f1)"
TTS_PORT="$(echo "$TTS_URL" | sed -E 's|https?://||' | cut -d: -f2 | cut -d/ -f1)"
TTS_HOST="${TTS_HOST:-127.0.0.1}"
TTS_PORT="${TTS_PORT:-9880}"

if command -v nc &>/dev/null; then
    if nc -z -w 3 "$TTS_HOST" "$TTS_PORT" 2>/dev/null; then
        echo "✅ GPT-SoVITS TTS 服务已就绪 ($TTS_HOST:$TTS_PORT)"
    else
        echo "⚠️ 警告: GPT-SoVITS TTS 服务未响应 ($TTS_HOST:$TTS_PORT)"
        echo "    语音合成功能将不可用。继续启动…"
    fi
elif command -v curl &>/dev/null; then
    if curl -s --max-time 3 "$TTS_URL" &>/dev/null || curl -s --max-time 3 -o /dev/null -w "%{http_code}" "$TTS_URL" | grep -qE "200|404"; then
        echo "✅ GPT-SoVITS TTS 服务已就绪 ($TTS_URL)"
    else
        echo "⚠️ 警告: GPT-SoVITS TTS 服务未响应 ($TTS_URL)"
        echo "    语音合成功能将不可用。继续启动…"
    fi
else
    echo "⚠️ 警告: 未找到 nc 或 curl，跳过 TTS 健康检查"
fi

# ── 3. Build & Run ───────────────────────────────────────────────────────
echo ""
echo "[3/3] 编译并启动 Ellen Rust Backend…"
echo ""

if ! command -v cargo &>/dev/null; then
    echo "❌ 错误: 未找到 cargo"
    echo "   请安装 Rust: https://rustup.rs/"
    exit 1
fi

echo "→ cargo run --release"
echo ""
cargo run --release "$@"
