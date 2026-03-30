@echo off
setlocal EnableDelayedExpansion
REM TokenMonitor Prerequisites Installer (Windows)
REM Installs Node.js portable + @ccusage/mcp if not already present.
REM Safe to re-run: detects existing installations and skips.

set "INSTALL_DIR=%LOCALAPPDATA%\TokenMonitor"
set "NODE_DEST=%INSTALL_DIR%\node"
set "BUNDLED_NODE=%~dp0prereqs\node-portable"

echo =============================================
echo   TokenMonitor Prerequisites Setup
echo =============================================
echo.

REM ── Step 1: Check Node.js ──────────────────────────────────────────────────
echo Checking for Node.js...
where node >nul 2>nul
if %ERRORLEVEL% equ 0 (
    for /f "tokens=*" %%v in ('node --version 2^>nul') do set "NODE_VER=%%v"
    echo [OK]  Node.js found: !NODE_VER!
    goto :check_ccusage
)

echo [!!]  Node.js not found. Installing from bundled package...

if not exist "%BUNDLED_NODE%\node.exe" (
    echo [ERR] Bundled Node.js not found at %BUNDLED_NODE%
    echo       Please install Node.js manually from https://nodejs.org/
    goto :check_ccusage
)

REM Copy portable Node.js to install location
if not exist "%NODE_DEST%" mkdir "%NODE_DEST%"
echo       Copying Node.js to %NODE_DEST% ...
xcopy /E /Y /Q "%BUNDLED_NODE%\*" "%NODE_DEST%\" >nul

REM Add to user PATH (persistent)
echo       Adding to PATH...
for /f "tokens=2*" %%a in ('reg query "HKCU\Environment" /v Path 2^>nul') do set "USER_PATH=%%b"
echo !USER_PATH! | findstr /I /C:"%NODE_DEST%" >nul
if %ERRORLEVEL% neq 0 (
    setx PATH "!USER_PATH!;%NODE_DEST%" >nul 2>nul
)

REM Update current session PATH
set "PATH=%PATH%;%NODE_DEST%"

echo [OK]  Node.js installed to %NODE_DEST%

REM ── Step 2: Check ccusage ──────────────────────────────────────────────────
:check_ccusage
echo.
echo Checking for ccusage...

REM Check if ccusage-mcp or ccusage is available
where ccusage >nul 2>nul
if %ERRORLEVEL% equ 0 (
    echo [OK]  ccusage found
    goto :done
)

REM If we just installed our portable Node.js, ccusage should be bundled
if exist "%NODE_DEST%\node_modules\@ccusage" (
    echo [OK]  ccusage found in portable Node.js
    goto :done
)

REM Try npm install (works if online, or if cached)
where npm >nul 2>nul
if %ERRORLEVEL% equ 0 (
    echo [!!]  ccusage not found. Installing via npm...
    npm install -g @ccusage/mcp >nul 2>nul
    if %ERRORLEVEL% equ 0 (
        echo [OK]  ccusage installed
    ) else (
        echo [!!]  npm install failed. ccusage can be installed later:
        echo         npm install -g @ccusage/mcp
    )
) else (
    echo [!!]  npm not available. Install ccusage later: npm install -g @ccusage/mcp
)

:done
echo.
echo =============================================
echo   Prerequisites ready!
echo   You can now install TokenMonitor.
echo =============================================
echo.
pause
