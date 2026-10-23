; NSIS hooks for the Peek installer (bundle.windows.nsis.installerHooks in tauri.conf.json).

; "Iniciar com o Windows" creates an elevated "Peek" logon task: remove it with the app,
; without a console window. Kept on updates (/UPDATE), as the template keeps its Run key:
; the updated app recreates it on start.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$SYSDIR\schtasks.exe" /Delete /TN Peek /F'
    Pop $0
  ${EndIf}
!macroend
