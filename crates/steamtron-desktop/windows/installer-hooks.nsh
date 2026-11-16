; Install the 64-bit application under Program Files (x86) as requested.
; Tauri's perMachine mode defaults to $PROGRAMFILES64 for x64 builds.
!macro NSIS_HOOK_PREINSTALL
  StrCpy $INSTDIR "$PROGRAMFILES32\${PRODUCTNAME}"
  CreateDirectory "$INSTDIR"
  SetOutPath "$INSTDIR"
!macroend

; WinFsp installs its DLL outside the app directory. Copy it beside the
; executable so the Windows loader can satisfy the static WinFsp import.
!macro NSIS_HOOK_POSTINSTALL
  IfFileExists "$PROGRAMFILES32\WinFsp\bin\winfsp-x64.dll" 0 winfsp_x86_done
  CopyFiles /SILENT "$PROGRAMFILES32\WinFsp\bin\winfsp-x64.dll" "$INSTDIR\winfsp-x64.dll"
  Goto winfsp_copy_done
  winfsp_x86_done:
  IfFileExists "$PROGRAMFILES64\WinFsp\bin\winfsp-x64.dll" 0 winfsp_copy_done
  CopyFiles /SILENT "$PROGRAMFILES64\WinFsp\bin\winfsp-x64.dll" "$INSTDIR\winfsp-x64.dll"
  winfsp_copy_done:
!macroend
