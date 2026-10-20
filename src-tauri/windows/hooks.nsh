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

!macro NSIS_HOOK_PREUNINSTALL
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
