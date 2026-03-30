; TokenMonitor Prerequisites Installer
; Installs Node.js (via winget) and @ccusage/mcp (via npm)

!include "MUI2.nsh"
!include "LogicLib.nsh"

Name "TokenMonitor Prerequisites Setup"
OutFile "TokenMonitor-PrereqSetup.exe"
InstallDir "$TEMP\TokenMonitor-Setup"
RequestExecutionLevel user

; -- UI --
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "TokenMonitor Prerequisites Setup"
!define MUI_WELCOMEPAGE_TEXT "This tool will install the required dependencies for TokenMonitor:$\r$\n$\r$\n  1. Node.js (if not installed)$\r$\n  2. @ccusage/mcp (npm package)$\r$\n$\r$\nClick Next to continue."
!define MUI_FINISHPAGE_TITLE "Setup Complete"
!define MUI_FINISHPAGE_TEXT "All prerequisites have been installed.$\r$\n$\r$\nYou can now launch TokenMonitor."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_LANGUAGE "English"

; -- Main install section --
Section "Install Prerequisites" SecMain
  SetOutPath $INSTDIR

  ; -- Step 1: Check Node.js --
  DetailPrint "Checking for Node.js..."
  nsExec::ExecToStack 'cmd.exe /c "node --version"'
  Pop $0
  Pop $1
  ${If} $0 == 0
    DetailPrint "Node.js found: $1"
  ${Else}
    DetailPrint "Node.js not found. Installing via winget..."
    nsExec::ExecToStack 'cmd.exe /c "winget --version"'
    Pop $0
    Pop $1
    ${If} $0 == 0
      DetailPrint "Using winget to install Node.js LTS..."
      nsExec::ExecToLog 'cmd.exe /c "winget install OpenJS.NodeJS.LTS --accept-source-agreements --accept-package-agreements"'
      Pop $0
      ${If} $0 != 0
        MessageBox MB_OK|MB_ICONEXCLAMATION "winget install failed. Please install Node.js manually from https://nodejs.org/ then re-run this setup."
        Abort
      ${EndIf}
      DetailPrint "Node.js installed successfully."
    ${Else}
      MessageBox MB_OK|MB_ICONEXCLAMATION "winget not available. Please install Node.js manually from https://nodejs.org/ then re-run this setup."
      Abort
    ${EndIf}
  ${EndIf}

  ; -- Step 2: Refresh PATH --
  DetailPrint "Refreshing PATH..."
  ReadRegStr $2 HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path"
  ReadRegStr $3 HKCU "Environment" "Path"
  System::Call 'Kernel32::SetEnvironmentVariable(t "PATH", t "$2;$3;$PROGRAMFILES\nodejs;$APPDATA\npm")i'

  ; -- Step 3: Check npm --
  DetailPrint "Checking for npm..."
  nsExec::ExecToStack 'cmd.exe /c "npm --version"'
  Pop $0
  Pop $1
  ${If} $0 == 0
    DetailPrint "npm found: $1"
  ${Else}
    MessageBox MB_OK|MB_ICONEXCLAMATION "npm not found. Please ensure Node.js is properly installed, then re-run this setup."
    Abort
  ${EndIf}

  ; -- Step 4: Install @ccusage/mcp --
  DetailPrint "Installing @ccusage/mcp globally..."
  nsExec::ExecToLog 'cmd.exe /c "npm install -g @ccusage/mcp"'
  Pop $0
  ${If} $0 == 0
    DetailPrint "@ccusage/mcp installed successfully."
  ${Else}
    DetailPrint "Global install had issues, but npx will auto-download on first use."
  ${EndIf}

  DetailPrint ""
  DetailPrint "All prerequisites are ready!"
SectionEnd
