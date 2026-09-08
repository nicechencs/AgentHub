@echo off
setlocal EnableExtensions
cd /d "%~dp0"

REM Stop leftover AgentHub desktop/dev processes, then start run.ps1.
REM Double-click from Explorer often has a stale PATH (no cargo / node / pnpm).
for /f "usebackq delims=" %%I in (`powershell -NoProfile -Command "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')"`) do set "PATH=%%I"

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run.ps1" --restart %*
set "EC=%ERRORLEVEL%"
endlocal & exit /b %EC%
