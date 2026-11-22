@echo off
setlocal
set "HERE=%~dp0"

set "TARGET=%HERE%target"
if not defined CARGO_TARGET_DIR goto located
pushd "%HERE%"
for %%I in ("%CARGO_TARGET_DIR%") do set "TARGET=%%~fI"
popd
:located
set "BIN=%TARGET%\release\formslot-cli.exe"

where cargo >nul 2>nul || goto nocargo
if not exist "%BIN%" echo First run: building formslot. This takes a few minutes and only happens once. 1>&2
pushd "%HERE%"
cargo build -q --release --locked -p formslot-cli 1>&2 || (popd & exit /b 1)
popd
goto run

:nocargo
if exist "%BIN%" goto run
echo Rust is required: https://rustup.rs. Then run formslot_oc_install.bat 1>&2
exit /b 1

:run
"%BIN%" %*
exit /b %errorlevel%
