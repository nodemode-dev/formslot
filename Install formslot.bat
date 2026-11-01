@echo off
setlocal
cd /d "%~dp0"
set "FORMSLOT_NO_SETUP_OFFER=1"

echo Installing formslot
echo.

if exist cli\ if exist core\ goto source
if not exist formslot.exe (
  echo There is no formslot.exe beside this file.
  goto finish
)
formslot.exe setup
goto finish

:source
if not exist target\release\formslot-cli.exe (
  echo First, building formslot. This takes a few minutes and only happens once.
  echo.
  call formslot_oc_install.bat
  if errorlevel 1 (
    echo.
    echo The build did not finish. The messages above say what is missing; install it, then double-click this file again.
    goto finish
  )
)
call formslot.bat setup

:finish
echo.
set "ANSWER="
set /p "ANSWER=Press Enter to close. "
exit /b 0
