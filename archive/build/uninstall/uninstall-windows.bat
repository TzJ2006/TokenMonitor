@echo off
REM TokenMonitor Complete Uninstaller — Windows
REM Removes: app data, prerequisites, autostart registry, PATH entries.
REM The NSIS uninstaller handles the app binary and shortcuts.
setlocal enabledelayedexpansion

echo =============================================
echo   TokenMonitor Uninstaller — Windows
echo =============================================
echo.

REM ── Step 1: Stop running processes ──────────────────────────────────────────
echo Stopping TokenMonitor...
taskkill /f /im token-monitor.exe >nul 2>&1 && (
    echo [OK]  Process stopped
) || (
    echo [OK]  No running process found
)
timeout /t 2 /nobreak >nul

REM ── Step 2: Run NSIS uninstaller if present ─────────────────────────────────
set "NSIS_UNINSTALL="
for /f "tokens=2*" %%a in ('reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\TokenMonitor" /v "UninstallString" 2^>nul') do (
    set "NSIS_UNINSTALL=%%b"
)

if defined NSIS_UNINSTALL (
    echo Found NSIS uninstaller: %NSIS_UNINSTALL%
    echo Running NSIS uninstaller...
    start /wait "" %NSIS_UNINSTALL% /S
    echo [OK]  NSIS uninstaller completed
) else (
    echo [!!]  NSIS uninstaller not found — skipping app removal
    echo        If the app is still installed, remove it from Settings ^> Apps
)

REM ── Step 3: Remove app data ─────────────────────────────────────────────────
if exist "%APPDATA%\com.tokenmonitor.app" (
    rmdir /s /q "%APPDATA%\com.tokenmonitor.app"
    echo [OK]  Removed app data: %%APPDATA%%\com.tokenmonitor.app
)
if exist "%LOCALAPPDATA%\com.tokenmonitor.app" (
    rmdir /s /q "%LOCALAPPDATA%\com.tokenmonitor.app"
    echo [OK]  Removed local app data
)

REM ── Step 4: Remove prerequisites (portable Node.js + ccusage only) ──────────
REM IMPORTANT: Only removes the portable copy inside TokenMonitor directory.
REM Never touches system-wide Node.js installations.
if exist "%LOCALAPPDATA%\TokenMonitor" (
    rmdir /s /q "%LOCALAPPDATA%\TokenMonitor"
    echo [OK]  Removed prerequisites: %%LOCALAPPDATA%%\TokenMonitor
) else (
    echo [OK]  No prerequisites directory found
)

REM ── Step 5: Remove autostart registry entry ─────────────────────────────────
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v "TokenMonitor" /f >nul 2>&1 && (
    echo [OK]  Removed autostart registry entry
) || (
    echo [OK]  No autostart entry found
)

REM ── Step 6: Clean user PATH ─────────────────────────────────────────────────
REM Remove TokenMonitor entries from user PATH in registry
for /f "tokens=2*" %%a in ('reg query "HKCU\Environment" /v "Path" 2^>nul') do (
    set "CURRENT_PATH=%%b"
)

if defined CURRENT_PATH (
    REM Remove any path segment containing \TokenMonitor\
    set "NEW_PATH="
    for %%p in ("!CURRENT_PATH:;=" "!") do (
        set "SEGMENT=%%~p"
        echo !SEGMENT! | findstr /i /c:"TokenMonitor" >nul 2>&1
        if errorlevel 1 (
            if defined NEW_PATH (
                set "NEW_PATH=!NEW_PATH!;!SEGMENT!"
            ) else (
                set "NEW_PATH=!SEGMENT!"
            )
        ) else (
            echo [OK]  Removed from PATH: !SEGMENT!
        )
    )
    if defined NEW_PATH (
        reg add "HKCU\Environment" /v "Path" /t REG_EXPAND_SZ /d "!NEW_PATH!" /f >nul 2>&1
    )
)

REM ── Step 7: Clean up registry remnants ──────────────────────────────────────
reg delete "HKCU\Software\tokenmonitor" /f >nul 2>&1
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\TokenMonitor" /f >nul 2>&1

echo.
echo =============================================
echo   TokenMonitor has been completely removed.
echo =============================================
pause
