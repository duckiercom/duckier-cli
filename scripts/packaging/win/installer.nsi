; Duckier CLI — NSIS Installer Script
;
; Installs duckier-cli.exe + duckiervpn-daemon.exe to Program Files,
; adds to system PATH, registers daemon as a Windows service,
; and creates an uninstaller with Add/Remove Programs entry.
;
; Built by: scripts/build-windows.sh (cross-compile) or scripts/build-windows-native.ps1 (native)

!include "MUI2.nsh"
!include "x64.nsh"
!include "FileFunc.nsh"
!include "WordFunc.nsh"

; ── Branding ──
!define PRODUCT_NAME "Duckier CLI"
!define PRODUCT_PUBLISHER "Duckier"
!define PRODUCT_WEB_SITE "https://duckier.com"
!define INSTALL_DIR_NAME "Duckier"
!define CLI_EXE "duckier-cli.exe"
!define DAEMON_EXE "duckiervpn-daemon.exe"
!define DAEMON_SERVICE "DuckierVpnDaemon"
!define DESKTOP_EXE "Duckier.exe"
!define WIREGUARD_DLL "wireguard.dll"
!define UNINSTALLER "uninstall.exe"

; ── Passed from makensis -D ──
; VERSION is defined at build time via -DVERSION=x.y.z
!ifndef VERSION
    !define VERSION "0.0.0"
!endif

; ── General ──
Name "${PRODUCT_NAME} v${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${INSTALL_DIR_NAME}"
InstallDirRegKey HKLM "Software\${PRODUCT_NAME}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

; ── UI ──
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; ──────────────────────────────────────────────────────
; Install
; ──────────────────────────────────────────────────────
Section "Install"
    SetOutPath "$INSTDIR"

    ; Kill any running CLI instances before installing
    nsExec::ExecToStack "TaskKill /IM ${CLI_EXE} /F"

    ; Check if daemon service is already running (active VPN session)
    nsExec::ExecToStack '"$SYSDIR\sc.exe" query "${DAEMON_SERVICE}"'
    Pop $0 ; exit code
    Pop $1 ; stdout (contains STATE line)
    ; sc query returns 0 if the service exists
    ; stdout contains "STATE ... RUNNING" when active
    StrCpy $R9 "0" ; $R9 = daemon was running
    ${If} $0 == 0
        ; Service exists — check if it's running
        ${WordFind} $1 "RUNNING" "E+1{" $2
        ${IfNot} ${Errors}
            StrCpy $R9 "1" ; daemon is running
        ${EndIf}
    ${EndIf}

    ${If} $R9 == "0"
        ; Daemon not running — safe to kill and replace
        nsExec::ExecToStack "TaskKill /IM ${DAEMON_EXE} /F"
        nsExec::ExecToStack '"$SYSDIR\sc.exe" stop "${DAEMON_SERVICE}"'
        Sleep 2000
    ${EndIf}

    ; Install files — always update CLI; only update daemon + DLL if it wasn't running
    File "${STAGING_DIR}\${CLI_EXE}"
    File "${STAGING_DIR}\LICENSE"
    File "${STAGING_DIR}\THIRD_PARTY_NOTICES.md"

    ${If} $R9 == "0"
        File "${STAGING_DIR}\${DAEMON_EXE}"
        File "${STAGING_DIR}\${WIREGUARD_DLL}"
    ${EndIf}

    ; Write install directory to registry
    WriteRegStr HKLM "Software\${PRODUCT_NAME}" "InstallDir" "$INSTDIR"
    WriteRegStr HKLM "Software\${PRODUCT_NAME}" "Version" "${VERSION}"

    ; ── Add to system PATH ──
    ; Uses tokenized matching: split PATH on semicolons, check for exact entry.
    ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path"
    Push $0
    Push "$INSTDIR"
    Call PathEntryExists
    Pop $2
    StrCmp $2 "0" 0 +2
        WriteRegExpandStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" "$0;$INSTDIR"

    ; Broadcast environment change so open shells pick up new PATH
    SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=5000

    ; ── Register and start daemon service (only if it wasn't already running) ──
    ${If} $R9 == "0"
        ; Check if the user opted out of running the daemon as a Windows service.
        ; When DaemonNoService=1, skip service creation entirely — the CLI will
        ; launch the daemon elevated (UAC) on demand instead.
        ReadRegDWORD $R0 HKLM "SOFTWARE\${PRODUCT_PUBLISHER}" "DaemonNoService"
        ${If} $R0 == 1
            ; User prefers UAC-elevated daemon — stop & remove service if it exists
            nsExec::ExecToLog '"$SYSDIR\sc.exe" stop "${DAEMON_SERVICE}"'
            nsExec::ExecToLog '"$SYSDIR\sc.exe" delete "${DAEMON_SERVICE}"'
        ${Else}
            ; Remove stale service entry if present
            nsExec::ExecToStack '"$SYSDIR\sc.exe" delete "${DAEMON_SERVICE}"'
            Pop $0
            Pop $1

            ; Write a temp batch file to avoid NSIS quote-escaping issues with sc binPath
            FileOpen $R1 "$TEMP\duckier-svc.bat" w
            FileWrite $R1 '@echo off$\r$\n'
            FileWrite $R1 'sc create ${DAEMON_SERVICE} binPath= "$INSTDIR\${DAEMON_EXE} --service" start= auto$\r$\n'
            FileWrite $R1 'sc failure ${DAEMON_SERVICE} reset= 86400 actions= restart/5000/restart/10000/restart/30000$\r$\n'
            FileClose $R1
            nsExec::ExecToStack '"$SYSDIR\cmd.exe" /c "$TEMP\duckier-svc.bat"'
            Pop $0
            Pop $1
            Delete "$TEMP\duckier-svc.bat"

            ${If} $0 != 0
                MessageBox MB_OK|MB_ICONEXCLAMATION "Warning: Failed to register daemon service (exit code $0). The CLI may not function correctly until the daemon is installed manually."
            ${EndIf}

            nsExec::ExecToStack '"$SYSDIR\sc.exe" start "${DAEMON_SERVICE}"'
            Pop $0
            Pop $1
            ${If} $0 != 0
                MessageBox MB_OK|MB_ICONEXCLAMATION "Warning: Failed to start daemon service (exit code $0). Try rebooting or running: $\"$INSTDIR\${DAEMON_EXE}$\" start"
            ${EndIf}
        ${EndIf}
    ${EndIf}

    ; ── Create uninstaller ──
    WriteUninstaller "$INSTDIR\${UNINSTALLER}"

    ; ── Add/Remove Programs entry ──
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
        "DisplayName" "${PRODUCT_NAME}"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
        "UninstallString" '"$INSTDIR\${UNINSTALLER}"'
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
        "InstallLocation" "$INSTDIR"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
        "Publisher" "${PRODUCT_PUBLISHER}"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
        "URLInfoAbout" "${PRODUCT_WEB_SITE}"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
        "DisplayVersion" "${VERSION}"
    WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
        "NoModify" 1
    WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
        "NoRepair" 1

    ; Get install size for Add/Remove Programs
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}" \
        "EstimatedSize" $0
SectionEnd

; ──────────────────────────────────────────────────────
; Uninstall
; ──────────────────────────────────────────────────────
Section "Uninstall"
    ; ── Daemon coexistence check ──
    ; If the desktop app is installed, leave the daemon service running.
    ; Tauri registers the desktop app under "Uninstall\Duckier" (the product name).
    ; Check: HKLM uninstall key (per-machine), HKCU uninstall key (per-user),
    ; Tauri's secondary product key, and common install directories.
    StrCpy $1 ""
    ; Per-machine: Tauri writes HKLM\...\Uninstall\Duckier when installMode=perMachine
    ReadRegStr $1 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_PUBLISHER}" "InstallLocation"
    ${If} $1 == ""
        ; Per-user: Tauri writes HKCU\...\Uninstall\Duckier when installMode=perUser
        ReadRegStr $1 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_PUBLISHER}" "InstallLocation"
    ${EndIf}
    ${If} $1 == ""
        ; Tauri secondary product registry key
        ReadRegStr $1 HKLM "Software\duckier\${PRODUCT_PUBLISHER}" ""
    ${EndIf}
    ${If} $1 == ""
        ReadRegStr $1 HKCU "Software\duckier\${PRODUCT_PUBLISHER}" ""
    ${EndIf}
    ${If} $1 == ""
        ; Fallback: check common install locations
        IfFileExists "$PROGRAMFILES64\${PRODUCT_PUBLISHER}\${DESKTOP_EXE}" 0 +2
            StrCpy $1 "found"
    ${EndIf}
    ${If} $1 == ""
        IfFileExists "$LOCALAPPDATA\${PRODUCT_PUBLISHER}\${DESKTOP_EXE}" 0 +2
            StrCpy $1 "found"
    ${EndIf}
    ${If} $1 == ""
        ; Desktop app NOT found — safe to stop and remove daemon
        nsExec::ExecToStack '"$SYSDIR\sc.exe" stop "${DAEMON_SERVICE}"'
        Sleep 2000
        nsExec::ExecToStack "TaskKill /IM ${DAEMON_EXE} /F"
        nsExec::ExecToStack '"$SYSDIR\sc.exe" delete "${DAEMON_SERVICE}"'
        nsExec::ExecToStack "TaskKill /IM ${DAEMON_EXE} /F"
        Delete "$INSTDIR\${DAEMON_EXE}"
        Delete "$INSTDIR\${WIREGUARD_DLL}"
    ${Else}
        ; Desktop app is installed — leave daemon service running
    ${EndIf}

    ; ── Remove CLI binary ──
    Delete "$INSTDIR\${CLI_EXE}"
    Delete "$INSTDIR\LICENSE"
    Delete "$INSTDIR\THIRD_PARTY_NOTICES.md"
    Delete "$INSTDIR\${UNINSTALLER}"
    RMDir "$INSTDIR"

    ; ── Remove from system PATH ──
    ; Uses tokenized matching: split PATH on semicolons, rebuild without our entry.
    ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path"
    Push $0
    Push "$INSTDIR"
    Call un.RemovePathEntry
    Pop $0
    WriteRegExpandStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" "$0"
    SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=5000

    ; ── Remove registry entries ──
    DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
    DeleteRegKey HKLM "Software\${PRODUCT_NAME}"

    ; Clean up daemon mode registry key
    DeleteRegValue HKLM "SOFTWARE\${PRODUCT_PUBLISHER}" "DaemonNoService"
    DeleteRegKey /ifempty HKLM "SOFTWARE\${PRODUCT_PUBLISHER}"

    ; User config at %APPDATA%\duckier\ is intentionally preserved
SectionEnd

; ──────────────────────────────────────────────────────
; Helper: Check if a semicolon-delimited PATH contains an exact entry.
; Splits on ";", compares each token exactly.
; Stack: push PATH, push entry → call → pop result ("1" if found, "0" if not)
; ──────────────────────────────────────────────────────
Function PathEntryExists
    Exch $R1 ; entry to find
    Exch
    Exch $R0 ; original PATH
    Push $R2 ; current char
    Push $R3 ; current token
    Push $R4 ; char index

    StrCpy $R3 "" ; token = ""
    StrCpy $R4 0  ; index = 0

    pee_loop:
        StrCpy $R2 $R0 1 $R4
        IntOp $R4 $R4 + 1
        StrCmp $R2 ";" pee_check
        StrCmp $R2 "" pee_check_last
        StrCpy $R3 "$R3$R2"
        Goto pee_loop

    pee_check:
        StrCmp $R3 $R1 pee_found
        StrCpy $R3 ""
        Goto pee_loop

    pee_check_last:
        StrCmp $R3 $R1 pee_found
        ; Not found
        StrCpy $R0 "0"
        Goto pee_done

    pee_found:
        StrCpy $R0 "1"

    pee_done:
        Pop $R4
        Pop $R3
        Pop $R2
        Exch $R0
        Exch
        Pop $R1
FunctionEnd

; ──────────────────────────────────────────────────────
; Helper: Remove a single entry from a semicolon-delimited PATH
; Splits on ";", compares each token exactly, rebuilds without matches.
; Stack: push PATH, push entry-to-remove → call → pop result
; ──────────────────────────────────────────────────────
Function un.RemovePathEntry
    Exch $R1 ; entry to remove
    Exch
    Exch $R0 ; original PATH
    Push $R2 ; current char
    Push $R3 ; current token
    Push $R4 ; result
    Push $R5 ; char index
    Push $R6 ; PATH length

    StrCpy $R4 "" ; result = ""
    StrCpy $R3 "" ; token = ""
    StrCpy $R5 0  ; index = 0
    StrLen $R6 $R0

    token_loop:
        ; Read one character at index $R5
        StrCpy $R2 $R0 1 $R5
        IntOp $R5 $R5 + 1

        ; If char is ";" or we've reached the end, process the token
        StrCmp $R2 ";" process_token
        StrCmp $R2 "" process_last
        ; Otherwise append char to current token
        StrCpy $R3 "$R3$R2"
        Goto token_loop

    process_token:
        ; Compare token to the entry we want to remove (case-insensitive not needed
        ; for exact install path match, but PATH entries are case-preserving on Windows)
        StrCmp $R3 $R1 skip_token
        StrCmp $R3 "" skip_token ; skip empty tokens from double semicolons
        ; Keep this token
        StrCmp $R4 "" first_kept
            StrCpy $R4 "$R4;$R3"
            Goto skip_token
        first_kept:
            StrCpy $R4 "$R3"
    skip_token:
        StrCpy $R3 "" ; reset token
        Goto token_loop

    process_last:
        ; Process the final token (no trailing semicolon)
        StrCmp $R3 $R1 done
        StrCmp $R3 "" done
        StrCmp $R4 "" first_kept_last
            StrCpy $R4 "$R4;$R3"
            Goto done
        first_kept_last:
            StrCpy $R4 "$R3"

    done:
        StrCpy $R0 $R4
        Pop $R6
        Pop $R5
        Pop $R4
        Pop $R3
        Pop $R2
        Exch $R0
        Exch
        Pop $R1
FunctionEnd
