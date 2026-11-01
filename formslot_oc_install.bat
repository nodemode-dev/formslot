@echo off
setlocal
cd /d "%~dp0"

where git >nul 2>nul || (echo git is required: https://git-scm.com & exit /b 1)
where cargo >nul 2>nul || (echo Rust 1.90 or newer is required: https://rustup.rs & exit /b 1)

cargo build --release --locked -p formslot-cli || exit /b 1

echo.
echo formslot is ready to run from source. Start the service with: formslot.bat serve
echo Check it with: formslot.bat status
echo Check this computer with: formslot.bat doctor
