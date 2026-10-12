@echo off
setlocal
chcp 65001 >nul
cd /d "%~dp0"
title Build PDF-Tutor

echo.
echo === PDF-Tutor: build installer ===
echo.

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust is missing. Please install it once:
  echo   1. Open https://rustup.rs and download rustup-init.exe
  echo   2. Run it and choose the suggested default installation
  echo      ^(this also installs the Visual Studio Build Tools^)
  echo   3. Close this window and start the script again
  goto end
)
where npm >nul 2>nul
if errorlevel 1 (
  echo Node.js is missing. Please install the LTS version from https://nodejs.org
  echo and start this script again afterwards.
  goto end
)

if not exist "node_modules\@tauri-apps\cli" (
  echo [1/2] Setting up the tools once ...
  call npm ci
  if errorlevel 1 goto failed
) else (
  echo [1/2] Tools are present.
)

echo [2/2] Building PDF-Tutor. The first time this takes 5-15 minutes ...
call npx tauri build
if errorlevel 1 goto failed

echo.
echo Done. The installer is in the folder that has just opened.
start "" "%~dp0src-tauri\target\release\bundle\nsis"
goto end

:failed
echo.
echo *** The build failed. See the messages above. ***

:end
echo.
pause
