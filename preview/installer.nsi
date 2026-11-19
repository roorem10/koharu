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
!define MUI_FINISHPAGE_RUN "$INSTDIR\Booharu.exe"
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

Var OldBooharuAccepted

Function VerifyDestination
  IfFileExists "$INSTDIR\koharu.exe" 0 checkOld
  ReadRegStr $0 HKCU "Software\Booharu" "InstallDir"
  StrCmp $0 "$INSTDIR" checkOld
  MessageBox MB_ICONSTOP|MB_OK "This folder already contains Koharu. Choose a new folder for Booharu so the existing program stays untouched." /SD IDOK
  Abort
checkOld:
  ; Booharu 0.0.4 and earlier kept settings, projects and models in PreviewData beside the
  ; program. Their projects don't open in this version, so say so once per folder.
  StrCmp $OldBooharuAccepted "$INSTDIR" allowed
  IfFileExists "$INSTDIR\PreviewData\*.*" 0 allowed
  MessageBox MB_ICONEXCLAMATION|MB_OKCANCEL "This folder has an older Booharu (0.0.4 or earlier).$\r$\n$\r$\nThis version can't open projects made by the older one, and installing here replaces the old program. Your old projects stay in the PreviewData folder.$\r$\n$\r$\nTo keep the old version for unfinished chapters, click Cancel and choose another folder." /SD IDOK IDOK accepted
  Abort
accepted:
  StrCpy $OldBooharuAccepted "$INSTDIR"
allowed:
FunctionEnd

Section "Application"
  Call VerifyDestination
  SetOutPath "$INSTDIR"
  ; Booharu 0.0.3 and earlier installed the program as koharu.exe. VerifyDestination has
  ; already refused any folder holding an official Koharu, so this one is our own old copy.
  Delete "$INSTDIR\koharu.exe"
  ; Every earlier Booharu, up to the 0.0.5 builds on Koharu 0.85.2, installed CEF 154's dxil.dll,
  ; which CEF 156 no longer ships, and File /r replaces only the files this build ships. Delete an
  ; older Booharu's copy so it doesn't sit beside CEF 156's dxcompiler.dll, but never another
  ; program's: only beside Booharu.exe or in the folder the last install recorded, which is where
  ; VerifyDestination lets koharu.exe through.
  ReadRegStr $0 HKCU "Software\Booharu" "InstallDir"
  ${If} ${FileExists} "$INSTDIR\Booharu.exe"
  ${OrIf} $0 == "$INSTDIR"
    Delete "$INSTDIR\dxil.dll"
  ${EndIf}
  ; The build script already rejects dot-files in staging. With /x ".*" and a "/*" pattern, makensis on Windows found no files.
  File /r "${RUNTIME_DIR}\*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Booharu"
  CreateShortcut "$SMPROGRAMS\Booharu\Booharu.lnk" "$INSTDIR\Booharu.exe"
  CreateShortcut "$SMPROGRAMS\Booharu\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Booharu" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu" "DisplayName" "Booharu"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Booharu" "DisplayIcon" "$INSTDIR\Booharu.exe"
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
  ; Booharu's builds on CEF 154 installed dxil.dll too, which this build's list lacks.
  Delete "$INSTDIR\dxil.dll"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
