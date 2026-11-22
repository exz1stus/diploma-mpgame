@echo off
rem Double-click / cmd.exe wrapper for dev.ps1 that bypasses the execution policy.
rem Usage: scripts\dev.cmd [-n client_count] [-s empty|circle]
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0dev.ps1" %*
exit /b %ERRORLEVEL%
