@echo off
setlocal
cd /d "%~dp0"

set "USERINSTALL=0"
if /i "%~1"=="--user" set "USERINSTALL=1"
if /i "%~1"=="-h" goto usage
if /i "%~1"=="--help" goto usage
if not "%~1"=="" if not "%USERINSTALL%"=="1" goto usage_error

if exist cli\ if exist core\ goto source
set "BUILT=%CD%\formslot.exe"
if not exist "%BUILT%" (echo no formslot.exe beside this script & exit /b 1)
goto place

:source
where git >nul 2>nul || (echo git is required: https://git-scm.com & exit /b 1)
where cargo >nul 2>nul || (echo Rust 1.90 or newer is required: https://rustup.rs & exit /b 1)

cargo build --release --locked -p formslot-cli || exit /b 1
set "TARGET=target"
if defined CARGO_TARGET_DIR set "TARGET=%CARGO_TARGET_DIR%"
set "BUILT=%TARGET%\release\formslot-cli.exe"

echo.
echo formslot is ready to run from source. Start the service with: formslot.bat serve
echo Check it with: formslot.bat status
echo Check this computer with: formslot.bat doctor

:place
if not "%USERINSTALL%"=="1" goto offer

set "DEST=%LOCALAPPDATA%\Programs\formslot"
if not exist "%DEST%" mkdir "%DEST%" || exit /b 1
copy /y "%BUILT%" "%DEST%\formslot.exe" >nul || exit /b 1
echo.
echo Installed %DEST%\formslot.exe
echo %PATH% | find /i "%DEST%" >nul && (echo %DEST% is already on your PATH, so Claude Code hooks find formslot.) || (
  echo %DEST% is not on your PATH yet. Your PATH was not changed. To add it for your account, run this once in PowerShell, then open a new terminal:
  echo   [Environment]::SetEnvironmentVariable^('Path', [Environment]::GetEnvironmentVariable^('Path','User'^) + ';%DEST%', 'User'^)
)
echo Connect a repository with: formslot install ^<repo^>
set "SETUPEXE=%DEST%\formslot.exe"

:offer
if defined FORMSLOT_NO_SETUP_OFFER exit /b 0
if not defined SETUPEXE set "SETUPEXE=%BUILT%"
echo.
echo Or let formslot walk you through everything: "%SETUPEXE%" setup
set "ANSWER="
set /p "ANSWER=Run the guided setup now? [Y/n] "
if not defined ANSWER goto runsetup
if /i "%ANSWER:~0,1%"=="y" goto runsetup
exit /b 0

:runsetup
"%SETUPEXE%" setup
exit /b %errorlevel%

:usage
echo formslot_oc_install.bat           check git and cargo and build the release binary here
echo formslot_oc_install.bat --user    also copy the binary to %%LOCALAPPDATA%%\Programs\formslot\formslot.exe
echo In an unpacked release folder, --user copies formslot.exe from beside this script and builds nothing.
exit /b 0

:usage_error
echo unknown option %~1 1>&2
goto usage
