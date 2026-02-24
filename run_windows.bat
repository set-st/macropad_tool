@echo off
:: Windows Launch Script for Macropad Editor Pro

echo Checking for Rust...
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo Rust is not installed. Please install it from https://rustup.rs/
    pause
    exit /b
)

echo Starting Macropad Editor Pro...
cargo run --release -- show-gui
pause
