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
!ifndef PACKAGE_NAME
  !error "Pass PACKAGE_NAME for the installer file name, for example Booharu-0.0.1."
!endif
!ifndef PRODUCT_VERSION
  !error "Pass PRODUCT_VERSION, the fork version shown to users."
!endif
!ifndef FILE_VERSION
  !error "Pass FILE_VERSION as four numbers, for example 0.0.1.0."
!endif

Name "Booharu"
OutFile "${OUTPUT_DIR}/${PACKAGE_NAME}-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Booharu"
InstallDirRegKey HKCU "Software\Booharu" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "${SOURCE_DIR}/crates/koharu/icons/icon.ico"
UninstallIcon "${SOURCE_DIR}/crates/koharu/icons/icon.ico"
VIProductVersion "${FILE_VERSION}"
VIAddVersionKey "ProductName" "Booharu"
VIAddVersionKey "FileDescription" "Booharu, a personal fork of Koharu"
VIAddVersionKey "FileVersion" "${PRODUCT_VERSION}"
VIAddVersionKey "ProductVersion" "${PRODUCT_VERSION}"
VIAddVersionKey "LegalCopyright" "Koharu contributors; see bundled licenses"

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SOURCE_DIR}/LICENSE-MIT"
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE VerifyDestination
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\koharu.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Booharu"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK "Booharu requires 64-bit Windows."
    Abort
  ${EndIf}
  SetRegView 64
FunctionEnd

Function VerifyDestination
  IfFileExists "$INSTDIR\koharu.exe" 0 allowed
  ReadRegStr $0 HKCU "Software\Booharu" "InstallDir"
  StrCmp $0 "$INSTDIR" allowed
  MessageBox MB_ICONSTOP|MB_OK "This folder already contains Koharu. Choose a new folder for Booharu so the existing program stays untouched." /SD IDOK
  Abort
allowed:
FunctionEnd

Section "Application"
  Call VerifyDestination
  SetOutPath "$INSTDIR"
  ; The build script already rejects dot-files in staging. With /x ".*" and a "/*" pattern, makensis on Windows found no files.
  File /r "${RUNTIME_DIR}\*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Booharu"
  CreateShortcut "$SMPROGRAMS\Booharu\Booharu.lnk" "$INSTDIR\koharu.exe"
  CreateShortcut "$SMPROGRAMS\Booharu\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Booharu" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu" "DisplayName" "Booharu"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu" "DisplayIcon" "$INSTDIR\koharu.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetRegView 64
  RMDir /r "$SMPROGRAMS\Booharu"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu"
  DeleteRegKey HKCU "Software\Booharu"
  !include "${UNINSTALL_INCLUDE}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
