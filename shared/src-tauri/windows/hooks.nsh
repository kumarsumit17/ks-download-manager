; NSIS installer hooks (tauri.conf.json: bundle.windows.nsis.installerHooks).
; The Chrome extension's native messaging host (docs/release-plan.md, section 4.2).

; After the files are in place: the app writes its host manifest next to itself and names
; it in HKCU\Software\Google\Chrome\NativeMessagingHosts\com.ks.downloadmanager, then exits
; (src-tauri/src/handoff.rs). The app does the same at every start, so nothing is lost if
; this step is skipped.
!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --register-chrome-host'
!macroend

; Before the files go: remove the registration and the manifest, so Chrome no longer finds
; a host that is not there. Downloads and settings are left to the uninstaller's own choice.
!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.ks.downloadmanager"
  Delete "$INSTDIR\com.ks.downloadmanager.json"
!macroend
