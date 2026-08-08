@echo off
setlocal
cd /d "%~dp0.."

REM AgentHub 更新发版入口。参数原样传给 PowerShell 脚本。
REM 示例:
REM   scripts\release-update.bat -Version 0.2.0 -Bump -Notes "bugfix"
REM   scripts\release-update.bat -Version 0.2.0 -Bump -Publish
REM   scripts\release-update.bat -SkipBuild -Version 0.2.0

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0release-update.ps1" %*
set EXITCODE=%ERRORLEVEL%
if %EXITCODE% neq 0 (
  echo.
  echo [ERROR] release-update failed with code %EXITCODE%
  exit /b %EXITCODE%
)
exit /b 0
