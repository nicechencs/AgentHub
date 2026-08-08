@echo off
setlocal EnableExtensions
cd /d "%~dp0"

echo ========================================
echo   AgentHub Desktop Client Launcher
echo ========================================
echo.
echo Working dir: %CD%
echo.

where node >nul 2>nul
if errorlevel 1 (
  echo [ERROR] Node.js not found. Install: https://nodejs.org/
  goto fail
)

where cargo >nul 2>nul
if errorlevel 1 (
  echo [ERROR] Rust/Cargo not found. Install: https://rustup.rs/
  echo [HINT] If already installed, ensure %%USERPROFILE%%\.cargo\bin is in PATH,
  echo        then reopen Explorer and double-click again.
  goto fail
)

where pnpm >nul 2>nul
if errorlevel 1 (
  echo [INFO] pnpm not found, installing via npm ...
  call npm install -g pnpm
  if errorlevel 1 (
    echo [ERROR] Failed to install pnpm
    goto fail
  )
)

if not exist "node_modules\" (
  echo [INFO] Installing deps: pnpm install ...
  call pnpm install
  if errorlevel 1 (
    echo [ERROR] pnpm install failed
    goto fail
  )
  echo.
)

echo [START] pnpm tauri:dev
echo [INFO] Starts Vite + Tauri desktop window (real backend, NOT browser mock)
echo [INFO] First build may take a while. Press Ctrl+C to stop.
echo.

call pnpm tauri:dev
set "EC=%ERRORLEVEL%"
if not "%EC%"=="0" (
  echo.
  echo [ERROR] tauri:dev exited with code %EC%
  goto fail
)

endlocal
exit /b 0

:fail
echo.
echo Press any key to close this window...
pause >nul
endlocal
exit /b 1