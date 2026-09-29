; Extra steps for Grandium's installer (see bundle > windows > nsis >
; installerHooks in tauri.conf.json).

; After installing or updating, Grandium opens on its setup screen the next
; time it starts, so the browser and search engine can be (re)picked. The
; app clears this once the setup screen is saved.
!macro NSIS_HOOK_POSTINSTALL
  WriteRegDWORD HKCU "Software\Grandium" "ShowSetup" 1
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Grandium adds itself here when "Start with Windows" is on.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Grandium"
  DeleteRegValue HKCU "Software\Grandium" "ShowSetup"
  DeleteRegKey /ifempty HKCU "Software\Grandium"
!macroend
