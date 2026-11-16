; Install the 64-bit application under Program Files (x86) as requested.
; Tauri's perMachine mode defaults to $PROGRAMFILES64 for x64 builds.
!macro NSIS_HOOK_PREINSTALL
  StrCpy $INSTDIR "$PROGRAMFILES32\${PRODUCTNAME}"
  CreateDirectory "$INSTDIR"
  SetOutPath "$INSTDIR"
!macroend
