; Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
; See LICENSE for the full text.
; SPDX-License-Identifier: GPL-3.0-or-later

; Hooks into the NSIS installer Tauri generates (bundle.windows.nsis.installerHooks in
; tauri.windows.conf.json).
;
; Uninstalling removes what the app wrote to the registry itself (src/platform/other.rs): the Run
; value that starts it at sign-in, and the native messaging host key each Chromium browser reads to
; find the Outlook and Teams capture host (src/nativehost.rs). The names match RUN_KEY, RUN_VALUE,
; BROWSERS and HOST there. An update runs the old uninstaller with /UPDATE ($UpdateMode = 1), and
; then everything is kept, so open at login and the extensions keep working across it.

; Installing writes Brainstead.exe, which Windows can't do while any process runs it: the app, an
; assistant's MCP server (`Brainstead.exe --mcp`, one per session) or Chrome's capture host. The
; template's own check (CheckIfAppIsRunning, Restart Manager) ends those, but a client can start a
; new one from the same file before it is written (Claude Code's SDK sessions start their MCP server
; again at the next tool call, Chrome its host at the next message). Then NSIS's File can't write
; it: under /S it skips the file and the install still ends with exit code 0, keeping the old
; program; under /P it asks Abort, Retry or Ignore. So the hook runs that same check first and
; then renames the old program to Brainstead.exe.old, which Windows allows while it runs: the path
; File writes is then free whatever starts meanwhile. Something started between the check and the
; rename runs the moved copy, out of reach of the template's check after the hook (which then finds
; nothing), so the hook runs the check again on the moved copy. A copy still held from a previous
; update gets a unique name. $MovedAside is the name the program was given, so only that copy is
; ever put back. POSTINSTALL puts it back when File didn't write the program (skipped under /S, or
; Ignore under /P), and otherwise deletes the old copies, which fails harmlessly for one still
; running; the app deletes those at its next start (src/platform/mod.rs, remove_update_installers).
;
; This file is included before the template defines MAINBINARYNAME, so .onInstFailed, a function
; compiled where it stands, names the program as HOOK_PROGRAM, and PREINSTALL stops the build if the two
; differ (productName or mainBinaryName changed in tauri.conf.json).
!define HOOK_PROGRAM "Brainstead"
Var MovedAside

!macro NSIS_HOOK_PREINSTALL
  !if "${MAINBINARYNAME}" != "${HOOK_PROGRAM}"
    !error "hooks.nsh names the program ${HOOK_PROGRAM}.exe but the template installs ${MAINBINARYNAME}.exe: change HOOK_PROGRAM"
  !endif
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  StrCpy $MovedAside ""
  Delete "$INSTDIR\${MAINBINARYNAME}.exe.old*"
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe.old"
      System::Call "kernel32::GetTickCount() i .r0"
      StrCpy $MovedAside "$INSTDIR\${MAINBINARYNAME}.exe.old-$0"
    ${Else}
      StrCpy $MovedAside "$INSTDIR\${MAINBINARYNAME}.exe.old"
    ${EndIf}
    ClearErrors
    Rename "$INSTDIR\${MAINBINARYNAME}.exe" "$MovedAside"
    ${If} ${Errors}
      StrCpy $MovedAside ""
    ${Else}
      !insertmacro CheckIfAppIsRunning "$MovedAside" "${PRODUCTNAME}"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $MovedAside != ""
  ${AndIfNot} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    Rename "$MovedAside" "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $MovedAside ""
  ${EndIf}
  Delete "$INSTDIR\${MAINBINARYNAME}.exe.old*"
!macroend

; An install that stops (the check couldn't end something, a file couldn't be written) puts the old
; program back if the hook had moved it and nothing new took its place. For an update from the app
; (the updater passes /UPDATE) it then opens the old version again, which says in its own update
; dialog that the update didn't install (src/updates.rs, `Started`). NSIS calls this once the user
; closes the failed installer's window, or at once under /S. The template declares $UpdateMode and
; adds the nsis_tauri_utils plugin after this file, so this reads the command line itself and starts
; the app with Exec (the install is per user, so not elevated).
Function .onInstFailed
  ${If} $MovedAside != ""
  ${AndIfNot} ${FileExists} "$INSTDIR\${HOOK_PROGRAM}.exe"
    Rename "$MovedAside" "$INSTDIR\${HOOK_PROGRAM}.exe"
  ${EndIf}
  ClearErrors
  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${IfNot} ${Errors}
  ${AndIf} ${FileExists} "$INSTDIR\${HOOK_PROGRAM}.exe"
    Exec '"$INSTDIR\${HOOK_PROGRAM}.exe"'
  ${EndIf}
FunctionEnd

; A copy the installer moved aside may still run (the app, an MCP server or the capture host), and
; the template's check before it deletes the program registers only the program itself: each copy
; is checked the same way here and then deleted, so the folder doesn't stay behind with it. $4 and
; $5 are kept, since the check uses $0 to $3 and $R0 to $R3.
!macro NSIS_HOOK_PREUNINSTALL
  Push $4
  Push $5
  FindFirst $4 $5 "$INSTDIR\${MAINBINARYNAME}.exe.old*"
  ${DoWhile} $5 != ""
    !insertmacro CheckIfAppIsRunning "$INSTDIR\$5" "${PRODUCTNAME}"
    Delete "$INSTDIR\$5"
    FindNext $4 $5
  ${Loop}
  FindClose $4
  Pop $5
  Pop $4
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Brainstead"
    DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.wayned.brainstead"
    DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.wayned.brainstead"
    DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.wayned.brainstead"
    DeleteRegKey HKCU "Software\Chromium\NativeMessagingHosts\com.wayned.brainstead"
  ${EndIf}
!macroend

; The uninstaller's "Delete the application data" box clears %APPDATA%\<identifier>, but Brainstead
; keeps its own folder in %APPDATA%\Brainstead (core/src/paths.rs, data_dir): settings, the index,
; Changes, unsaved chats. With the box ticked, and not for an update, that goes too, with the update
; installers the updater left in Temp (src/platform/mod.rs, remove_update_installers). The vault is
; never touched: it is the user's notes, wherever they keep them.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$APPDATA\${PRODUCTNAME}"
    FindFirst $0 $1 "$TEMP\${PRODUCTNAME}-*-updater-*"
    ${DoWhile} $1 != ""
      RmDir /r "$TEMP\$1"
      FindNext $0 $1
    ${Loop}
    FindClose $0
  ${EndIf}
!macroend
