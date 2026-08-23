@echo off
REM OpenMem Portable Environment Setup (Windows)
REM Usage: call bin\portable_env.bat

REM Set OpenMem root
set "OPENMEM_ROOT=%~dp0.."
set "OPENMEM_ROOT=%OPENMEM_ROOT:~0,-1%"

REM Add OpenMem to Python path
set "PYTHONPATH=%OPENMEM_ROOT%;%PYTHONPATH%"

REM Add bin directory to PATH
set "PATH=%OPENMEM_ROOT%\bin;%PATH%"

REM Data directory
set "OPENMEM_DATA=%OPENMEM_ROOT%\data"

REM Auto-detect agent (override with OPENMEM_AGENT)
if not defined OPENMEM_AGENT (
    REM Check for agent indicators
    if exist "%CD%\.qwen" set "OPENMEM_AGENT=qwen_code"
    if exist "%CD%\.claude" set "OPENMEM_AGENT=claude_code"
    if exist "%CD%\.cursor" set "OPENMEM_AGENT=cursor"
    if exist "%CD%\.vscode" set "OPENMEM_AGENT=vscode"
    if exist "%CD%\CLAUDE.md" set "OPENMEM_AGENT=claude_code"
)

REM Default to generic if not detected
if not defined OPENMEM_AGENT set "OPENMEM_AGENT=generic"

echo [OpenMem] Environment configured:
echo   ROOT:    %OPENMEM_ROOT%
echo   DATA:    %OPENMEM_DATA%
echo   AGENT:   %OPENMEM_AGENT%
echo   PYTHON:  %PYTHON%

echo.
echo Quick commands:
echo   python bin\launcher.py          - Status
echo   python bin\launcher.py --install - Install
echo   python bin\launcher.py --agents  - List agents
echo   python bin\launcher.py --skill all - Install all skills
