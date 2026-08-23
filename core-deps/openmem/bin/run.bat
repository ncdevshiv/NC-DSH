@echo off
REM OpenMem Launcher (Windows)
REM Double-click or run: bin\run.bat [args]

cd /d "%~dp0.."
call bin\portable_env.bat >nul 2>&1

if "%~1"=="" (
    python bin\launcher.py --status
) else (
    python bin\launcher.py %*
)
