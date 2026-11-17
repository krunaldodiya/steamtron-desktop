; SteamTron Windows bootstrapper.
; Build with:
;   makensis -DINNER_INSTALLER=... -DOUTFILE=... -DVC_INSTALLER=... \
;            -DDOTNET_INSTALLER=... -DWINFSP_INSTALLER=... bootstrapper.nsi

Unicode true
!include "MUI2.nsh"
!include "nsDialogs.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "StrFunc.nsh"
${Using:StrFunc} StrStr

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
!define MUI_FINISHPAGE_TEXT "SteamTron and its prerequisites are installed. If Windows asks for a restart, restart before launching SteamTron."
!define MUI_FINISHPAGE_RUN "$PROGRAMFILES32\SteamTron\steamtron-desktop.exe"
!define MUI_FINISHPAGE_RUN_FUNCTION RunAfterInstall
!define MUI_FINISHPAGE_RUN_TEXT "Launch SteamTron"
!define MUI_FINISHPAGE_RUN_NOTCHECKED
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
  ; This is a 32-bit NSIS process. Read the native 64-bit registry view and
  ; check System32 for the x64 runtime; $SYSDIR redirects to SysWOW64.
  SetRegView 64
  StrCpy $VCStatus "Missing"
  ReadRegStr $0 HKLM "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64" "Installed"
  ${If} $0 == "1"
    IfFileExists "$WINDIR\System32\vcruntime140.dll" 0 vc_missing
    IfFileExists "$WINDIR\System32\msvcp140.dll" 0 vc_missing
    StrCpy $VCStatus "Installed"
  ${EndIf}
  vc_missing:

  ; .NET Desktop 9 x64: directory presence is not enough. Verify the
  ; registered shared framework through the official dotnet host.
  StrCpy $DotNetStatus "Missing"
  nsExec::ExecToStack '"$SYSDIR\cmd.exe" /C dotnet --list-runtimes'
  Pop $0
  Pop $2
  ${If} $0 == 0
    ${StrStr} $3 $2 "Microsoft.WindowsDesktop.App 9."
    ${If} $3 != ""
      StrCpy $DotNetStatus "Installed"
    ${EndIf}
  ${EndIf}

  ; WinFsp 2.1 x64: the registry can survive a partial uninstall. Require
  ; the installed service, not just an InstallDir value.
  StrCpy $WinFspStatus "Missing"
  nsExec::ExecToStack '"$SYSDIR\sc.exe" query WinFsp.Launcher'
  Pop $0
  Pop $2
  ${If} $0 == 0
    StrCpy $WinFspStatus "Installed"
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

Function RunAfterInstall
  ${If} $RebootRequired == "1"
    MessageBox MB_ICONINFORMATION "Windows must be restarted before SteamTron can use the newly installed runtimes and WinFsp driver. Restart Windows, then launch SteamTron."
    Return
  ${EndIf}
  ExecShell "open" "$PROGRAMFILES32\SteamTron\steamtron-desktop.exe"
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
