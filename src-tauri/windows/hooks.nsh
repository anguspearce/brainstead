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
