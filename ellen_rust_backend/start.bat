@echo off
chcp 65001 >nul 2>&1
REM Ellen AI Rust Backend — Windows Startup Script
REM
REM 1. Checks LLM_API_KEY environment variable
REM 2. Verifies GPT-SoVITS TTS service is reachable
REM 3. Compiles and runs the backend

echo ╔══════════════════════════════════════════════════════════╗
echo ║      Ellen AI Rust Backend Launcher                      ║
echo ╚══════════════════════════════════════════════════════════╝
echo.

REM ── 1. Environment Check ─────────────────────────────────────────────────
echo [1/3] Checking environment variables...

if "%LLM_API_KEY%"=="" (
    echo ❌ Error: LLM_API_KEY environment variable is not set
    echo.
    echo    Please set your DeepSeek API Key:
    echo    set LLM_API_KEY=sk-your-key-here
    echo.
    pause
    exit /b 1
)

echo ✅ LLM_API_KEY is set

REM ── 2. TTS Health Check ──────────────────────────────────────────────────
echo.
echo [2/3] Checking TTS service (127.0.0.1:9880)...

if not defined TTS_API_URL set "TTS_API_URL=http://127.0.0.1:9880"

REM Try to connect using PowerShell
powershell -Command "try { $r = Invoke-WebRequest -Uri '%TTS_API_URL%' -TimeoutSec 3 -UseBasicParsing; exit 0 } catch { if ($_.Exception.Response -or $_.Exception.Status -eq 'NameResolutionFailure') { exit 0 } else { exit 1 } }" >nul 2>&1

if %ERRORLEVEL% == 0 (
    echo ✅ GPT-SoVITS TTS service is ready
) else (
    echo ⚠️  Warning: GPT-SoVITS TTS service is not responding
    echo    Voice synthesis will be unavailable. Continuing...
)

REM ── 3. Build & Run ───────────────────────────────────────────────────────
echo.
echo [3/3] Building and starting Ellen Rust Backend...
echo.

where cargo >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo ❌ Error: cargo not found
    echo    Please install Rust: https://rustup.rs/
    pause
    exit /b 1
)

echo → cargo run --release
echo.
cargo run --release %*

pause
