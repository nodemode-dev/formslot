@echo off
setlocal
set "HERE=%~dp0"

where cargo >nul 2>nul || (echo Rust is required: https://rustup.rs. Then run formslot_oc_install.bat & exit /b 1)
if not exist "%HERE%target\release\formslot-cli.exe" echo First run: building formslot. This takes a few minutes and only happens once. 1>&2
pushd "%HERE%"
cargo build -q --release --locked -p formslot-cli 1>&2 || (popd & exit /b 1)
popd
"%HERE%target\release\formslot-cli.exe" %*
exit /b %errorlevel%
