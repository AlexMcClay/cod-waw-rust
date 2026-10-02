@echo off
rem Builds a standalone copy of Undead Rounds into dist\UndeadRounds.
rem The folder contains only the game's own files: art, sounds and the Nacht
rem map are read at runtime from your World at War install.
setlocal
cd /d "%~dp0"
where cargo >nul 2>nul
if errorlevel 1 set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust is not installed. Get it from https://rustup.rs and run this again.
  pause
  exit /b 1
)
cargo build --release -p zm_game
if errorlevel 1 (
  echo Build failed.
  pause
  exit /b 1
)
set "OUT=dist\UndeadRounds"
if not exist "%OUT%" mkdir "%OUT%"
copy /y "target\release\undead_rounds.exe" "%OUT%\UndeadRounds.exe" >nul
copy /y "sounds.cfg" "%OUT%\sounds.cfg" >nul
copy /y "textures.cfg" "%OUT%\textures.cfg" >nul
if not exist "%OUT%\undead.cfg" copy /y "undead.cfg" "%OUT%\undead.cfg" >nul
copy /y "dist_readme.txt" "%OUT%\README.txt" >nul
copy /y "LICENSE" "%OUT%\LICENSE.txt" >nul
echo.
echo Built %OUT%\UndeadRounds.exe
echo Double-click it to play.
endlocal
