@echo off
setlocal enabledelayedexpansion
cd /d "%~dp0.."
set "ROOT=%CD%"

where cargo >nul 2>nul || (echo [formslot] Rust 1.90 or newer is required: https://rustup.rs & exit /b 1)

set "VERSION="
for /f "tokens=3 delims= " %%v in ('findstr /b /c:"version = " Cargo.toml') do if not defined VERSION set "VERSION=%%v"
if not defined VERSION (echo [formslot] no workspace version found in Cargo.toml & exit /b 1)
set "VERSION=%VERSION:"=%"

set "ARCH=x86_64"
if /i "%PROCESSOR_ARCHITECTURE%"=="ARM64" set "ARCH=aarch64"
set "NAME=formslot-v%VERSION%-windows-%ARCH%"

echo [formslot] building formslot-cli v%VERSION% for windows-%ARCH%
cargo build --release --locked -p formslot-cli || exit /b 1

set "TARGET=target"
if defined CARGO_TARGET_DIR set "TARGET=%CARGO_TARGET_DIR%"
if not exist "%TARGET%\release\formslot-cli.exe" (echo [formslot] the release binary is missing at %TARGET%\release\formslot-cli.exe & exit /b 1)

set "DIST=%ROOT%\dist"
if defined FORMSLOT_DIST_DIR set "DIST=%FORMSLOT_DIST_DIR%"
set "STAGE=%DIST%\%NAME%"
if exist "%STAGE%" rmdir /s /q "%STAGE%"
if exist "%DIST%\%NAME%.zip" del /q "%DIST%\%NAME%.zip"
mkdir "%STAGE%\plugin" "%STAGE%\data" || exit /b 1

copy /y "%TARGET%\release\formslot-cli.exe" "%STAGE%\formslot.exe" >nul || exit /b 1
copy /y LICENSE "%STAGE%\" >nul || exit /b 1
copy /y NOTICE.md "%STAGE%\" >nul || exit /b 1
copy /y README.md "%STAGE%\" >nul || exit /b 1
copy /y formslot_oc_install.bat "%STAGE%\" >nul || exit /b 1
xcopy /e /i /q /y plugin\formslot "%STAGE%\plugin\formslot" >nul || exit /b 1
xcopy /e /i /q /y data "%STAGE%\data" >nul || exit /b 1

powershell -NoProfile -Command "$s='%STAGE%'; Get-ChildItem -Recurse -File $s | Where-Object { $_.Name -ne 'SHA256SUMS' } | Sort-Object FullName | ForEach-Object { (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLower() + '  ' + $_.FullName.Substring($s.Length + 1).Replace('\','/') } | Set-Content -Encoding ascii (Join-Path $s 'SHA256SUMS')" || exit /b 1
powershell -NoProfile -Command "Compress-Archive -Path '%STAGE%' -DestinationPath '%DIST%\%NAME%.zip' -Force" || exit /b 1
powershell -NoProfile -Command "$h=(Get-FileHash -Algorithm SHA256 '%DIST%\%NAME%.zip').Hash.ToLower(); Set-Content -Encoding ascii '%DIST%\%NAME%.zip.sha256' ($h + '  %NAME%.zip')" || exit /b 1

echo [formslot] staged %STAGE%
echo [formslot] archive %DIST%\%NAME%.zip
exit /b 0
