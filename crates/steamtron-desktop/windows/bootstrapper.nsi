; SteamTron Windows bootstrapper.
; Build with:
;   makensis -DINNER_INSTALLER=... -DOUTFILE=... -DVC_INSTALLER=... \
;            -DDOTNET_INSTALLER=... -DWINFSP_INSTALLER=... bootstrapper.nsi

Unicode true
!include "MUI2.nsh"
!include "nsDialogs.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef OUT
  !define OUT "SteamTron-Setup.exe"
!endif

Name "SteamTron ${VERSION}"
OutFile "${OUT}"
InstallDir "$PROGRAMFILES32\SteamTron"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
BrandingText "SteamTron"

!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "Install SteamTron"
!define MUI_WELCOMEPAGE_TEXT "This setup checks the Windows runtime prerequisites, installs only the missing components, then installs SteamTron."
!define MUI_FINISHPAGE_TITLE "SteamTron is ready"
!define MUI_FINISHPAGE_TEXT "SteamTron and its prerequisites are installed."
!define MUI_FINISHPAGE_RUN "$PROGRAMFILES32\SteamTron\steamtron-desktop.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch SteamTron"
!insertmacro MUI_PAGE_WELCOME
Page custom PrereqPage PrereqPageLeave
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_LANGUAGE "English"

Var Dialog
Var VCStatus
Var DotNetStatus
Var WinFspStatus
Var InstallButton
Var StatusText
Var RebootRequired

Function .onInit
  SetShellVarContext all
  StrCpy $RebootRequired "0"
FunctionEnd

Function CheckPrerequisites
  ; Visual C++ 2015-2022 x64
  ReadRegStr $0 HKLM "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64" "Installed"
  ${If} $0 == "1"
    StrCpy $VCStatus "Installed"
  ${Else}
    StrCpy $VCStatus "Missing"
  ${EndIf}

  ; .NET Desktop 9 x64
  FindFirst $1 $2 "$PROGRAMFILES64\dotnet\shared\Microsoft.WindowsDesktop.App\9.*"
  ${If} $1 != 0
    FindClose $1
    StrCpy $DotNetStatus "Installed"
  ${Else}
    StrCpy $DotNetStatus "Missing"
  ${EndIf}

  ; WinFsp 2.1 x64
  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\WinFsp" "InstallDir"
  ${If} $0 == ""
    ReadRegStr $0 HKLM "SOFTWARE\WinFsp" "InstallDir"
  ${EndIf}
  ${If} $0 != ""
    StrCpy $WinFspStatus "Installed"
  ${Else}
    StrCpy $WinFspStatus "Missing"
  ${EndIf}
FunctionEnd

Function PrereqPage
  Call CheckPrerequisites
  !insertmacro MUI_HEADER_TEXT "Prerequisites" "SteamTron checks these before installing."
  nsDialogs::Create 1018
  Pop $Dialog
  ${If} $Dialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 24u "The following components are required. Installed components are left untouched."
  Pop $0
  ${NSD_CreateLabel} 0 30u 70% 18u "Visual C++ 2015-2022 x64"
  Pop $0
  ${NSD_CreateLabel} 72% 30u 28% 18u "$VCStatus"
  Pop $0
  ${NSD_CreateLabel} 0 50u 70% 18u ".NET Desktop Runtime 9.0.x x64"
  Pop $0
  ${NSD_CreateLabel} 72% 50u 28% 18u "$DotNetStatus"
  Pop $0
  ${NSD_CreateLabel} 0 70u 70% 18u "WinFsp 2.1 x64"
  Pop $0
  ${NSD_CreateLabel} 72% 70u 28% 18u "$WinFspStatus"
  Pop $0
  ${NSD_CreateLabel} 0 100u 100% 36u "Click Install missing prerequisites to continue. The installers run silently; the progress page shows each step."
  Pop $StatusText

  ${NSD_CreateButton} 0 145u 100% 24u "Install missing prerequisites and continue"
  Pop $InstallButton
  ${NSD_OnClick} $InstallButton StartInstall
  nsDialogs::Show
FunctionEnd

Function StartInstall
  ; Let the installer advance to the built-in progress page. The Section
  ; performs checks again and skips already-installed components.
  GetDlgItem $0 $HWNDPARENT 1
  SendMessage $0 0x00F5 0 0
FunctionEnd

Function PrereqPageLeave
  ; Nothing to validate here: the progress section installs only missing items.
FunctionEnd

Section "Install"
  SetDetailsPrint both
  SetOutPath "$PLUGINSDIR"
  File /oname=vc_redist.x64.exe "${VC_INSTALLER}"
  File /oname=windowsdesktop-runtime-9.0-win-x64.exe "${DOTNET_INSTALLER}"
  File /oname=winfsp-2.1.25156.msi "${WINFSP_INSTALLER}"
  File /oname=SteamTron-app-setup.exe "${INNER_INSTALLER}"

  Call CheckPrerequisites

  ${If} $VCStatus == "Missing"
    DetailPrint "Installing Visual C++ 2015-2022 x64..."
    ExecWait '"$PLUGINSDIR\vc_redist.x64.exe" /install /quiet /norestart' $0
    ${If} $0 == 3010
    ${OrIf} $0 == 1641
      StrCpy $RebootRequired "1"
    ${ElseIf} $0 != 0
      MessageBox MB_ICONSTOP "Visual C++ installation failed with exit code $0."
      Abort
    ${EndIf}
  ${Else}
    DetailPrint "Visual C++ already installed; skipping."
  ${EndIf}

  ${If} $DotNetStatus == "Missing"
    DetailPrint "Installing .NET Desktop Runtime 9.0.x x64..."
    ExecWait '"$PLUGINSDIR\windowsdesktop-runtime-9.0-win-x64.exe" /install /quiet /norestart' $0
    ${If} $0 == 3010
    ${OrIf} $0 == 1641
      StrCpy $RebootRequired "1"
    ${ElseIf} $0 != 0
      MessageBox MB_ICONSTOP ".NET Desktop Runtime installation failed with exit code $0."
      Abort
    ${EndIf}
  ${Else}
    DetailPrint ".NET Desktop Runtime already installed; skipping."
  ${EndIf}

  ${If} $WinFspStatus == "Missing"
    DetailPrint "Installing WinFsp 2.1 x64..."
    ExecWait '"$SYSDIR\msiexec.exe" /i "$PLUGINSDIR\winfsp-2.1.25156.msi" /qn /norestart' $0
    ${If} $0 == 3010
    ${OrIf} $0 == 1641
      StrCpy $RebootRequired "1"
    ${ElseIf} $0 != 0
      MessageBox MB_ICONSTOP "WinFsp installation failed with exit code $0."
      Abort
    ${EndIf}
  ${Else}
    DetailPrint "WinFsp already installed; skipping."
  ${EndIf}

  DetailPrint "Installing SteamTron..."
  ExecWait '"$PLUGINSDIR\SteamTron-app-setup.exe" /S' $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "SteamTron installation failed with exit code $0."
    Abort
  ${EndIf}
  DetailPrint "Installation complete."
  ${If} $RebootRequired == "1"
    MessageBox MB_ICONINFORMATION "A restart is required to finish installing the prerequisites."
  ${EndIf}
SectionEnd
