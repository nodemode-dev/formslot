@echo off
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul || (echo Rust is required: https://rustup.rs. Then run formslot_oc_install.bat & exit /b 1)
cargo build -q --release --locked -p formslot-cli 1>&2 || exit /b 1
target\release\formslot-cli.exe %*
exit /b %errorlevel%
