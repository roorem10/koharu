Unicode true
!include "MUI2.nsh"
!include "x64.nsh"

!ifndef RUNTIME_DIR
  !error "Pass RUNTIME_DIR containing the staged Windows application."
!endif
!ifndef OUTPUT_DIR
  !error "Pass OUTPUT_DIR for the installer."
!endif
!ifndef SOURCE_DIR
  !error "Pass SOURCE_DIR containing the source checkout."
!endif
!ifndef UNINSTALL_INCLUDE
  !error "Pass UNINSTALL_INCLUDE listing the exact staged files to remove."
!endif

Name "Koharu Typesetting Preview"
OutFile "${OUTPUT_DIR}/Koharu-Typesetting-Preview-2-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\KoharuTypesettingPreview"
InstallDirRegKey HKCU "Software\KoharuTypesettingPreview" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "${SOURCE_DIR}/crates/koharu/icons/icon.ico"
UninstallIcon "${SOURCE_DIR}/crates/koharu/icons/icon.ico"
VIProductVersion "0.83.5.2"
VIAddVersionKey "ProductName" "Koharu Typesetting Preview"
VIAddVersionKey "FileDescription" "Koharu font favorites and contour outline preview"
VIAddVersionKey "FileVersion" "0.83.5 Preview 2"
VIAddVersionKey "ProductVersion" "0.83.5 Preview 2"
VIAddVersionKey "LegalCopyright" "Koharu contributors; see bundled licenses"

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SOURCE_DIR}/LICENSE-MIT"
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE VerifyDestination
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\koharu.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Koharu Typesetting Preview"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK "This preview requires 64-bit Windows."
    Abort
  ${EndIf}
  SetRegView 64
FunctionEnd

Function VerifyDestination
  IfFileExists "$INSTDIR\koharu.exe" 0 allowed
  ReadRegStr $0 HKCU "Software\KoharuTypesettingPreview" "InstallDir"
  StrCmp $0 "$INSTDIR" allowed
  MessageBox MB_ICONSTOP|MB_OK "This folder already contains Koharu. Choose a new folder for the preview so the existing program stays untouched." /SD IDOK
  Abort
allowed:
FunctionEnd

Section "Application"
  Call VerifyDestination
  SetOutPath "$INSTDIR"
  File /r /x ".*" "${RUNTIME_DIR}/*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Koharu Typesetting Preview"
  CreateShortcut "$SMPROGRAMS\Koharu Typesetting Preview\Koharu Typesetting Preview.lnk" "$INSTDIR\koharu.exe"
  CreateShortcut "$SMPROGRAMS\Koharu Typesetting Preview\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\KoharuTypesettingPreview" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KoharuTypesettingPreview" "DisplayName" "Koharu Typesetting Preview 2"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KoharuTypesettingPreview" "DisplayVersion" "0.83.5 Preview 2"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KoharuTypesettingPreview" "DisplayIcon" "$INSTDIR\koharu.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KoharuTypesettingPreview" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KoharuTypesettingPreview" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KoharuTypesettingPreview" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KoharuTypesettingPreview" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetRegView 64
  RMDir /r "$SMPROGRAMS\Koharu Typesetting Preview"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KoharuTypesettingPreview"
  DeleteRegKey HKCU "Software\KoharuTypesettingPreview"
  !include "${UNINSTALL_INCLUDE}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
