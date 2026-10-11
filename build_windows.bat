@echo off
setlocal
chcp 65001 >nul
cd /d "%~dp0"
title PDF-Tutor bauen

echo.
echo === PDF-Tutor: Installer bauen ===
echo.

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust fehlt. Bitte einmalig installieren:
  echo   1. https://rustup.rs oeffnen und rustup-init.exe herunterladen
  echo   2. Ausfuehren und die vorgeschlagene Standardinstallation waehlen
  echo      ^(dabei werden auch die Visual-Studio-Buildtools installiert^)
  echo   3. Dieses Fenster schliessen und das Skript erneut starten
  goto ende
)
where npm >nul 2>nul
if errorlevel 1 (
  echo Node.js fehlt. Bitte die LTS-Version von https://nodejs.org installieren
  echo und dieses Skript danach erneut starten.
  goto ende
)

if not exist "node_modules\@tauri-apps\cli" (
  echo [1/2] Werkzeuge werden einmalig eingerichtet ...
  call npm ci
  if errorlevel 1 goto fehler
) else (
  echo [1/2] Werkzeuge vorhanden.
)

echo [2/2] Baue den PDF-Tutor. Beim ersten Mal dauert das 5-15 Minuten ...
call npx tauri build
if errorlevel 1 goto fehler

echo.
echo Fertig. Der Installer liegt im geoeffneten Ordner.
start "" "%~dp0src-tauri\target\release\bundle\nsis"
goto ende

:fehler
echo.
echo *** Beim Bauen ist ein Fehler aufgetreten. Bitte die Meldungen oben an Claude schicken. ***

:ende
echo.
pause
