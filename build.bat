@echo off
rem Builds and runs Undead Rounds (development). Requires Rust: https://rustup.rs
cd /d "%~dp0"
where cargo >nul 2>nul
if errorlevel 1 set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust is not installed. Get it from https://rustup.rs and run this again.
  pause
  exit /b 1
)
cargo run --release -- %*
if errorlevel 1 pause
