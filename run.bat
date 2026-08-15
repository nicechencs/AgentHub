@echo off
setlocal EnableExtensions
cd /d "%~dp0"

REM Double-click from Explorer often has a stale PATH (no cargo / node / pnpm).
for /f "usebackq delims=" %%I in (`powershell -NoProfile -Command "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')"`) do set "PATH=%%I"

REM One launcher: port check, leftover GUI warning, and tauri:dev live in run.ps1.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run.ps1" %*
set "EC=%ERRORLEVEL%"
endlocal & exit /b %EC%
