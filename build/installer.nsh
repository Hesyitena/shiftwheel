; NSIS additions for Shiftwheel (included by electron-builder)

!define SW_MENU_KEY "Software\Classes\*\shell\Shiftwheel"
!define SW_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define SW_APPROVED_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
!define SW_RUN_NAME "com.local.shiftwheel"

; Close the running process (a tray app with no window) without a confirmation dialog
!macro swKillRunning
  nsExec::Exec `taskkill /F /T /IM "${APP_EXECUTABLE_FILENAME}"`
  Pop $0
  Sleep 500
!macroend

!macro swAddVerb VERB LABEL
  WriteRegStr HKCU "${SW_MENU_KEY}\shell\${VERB}" "MUIVerb" "${LABEL}"
  WriteRegStr HKCU "${SW_MENU_KEY}\shell\${VERB}\command" "" '"$INSTDIR\${APP_EXECUTABLE_FILENAME}" --convert ${VERB} "%1"'
!macroend

!macro customInit
  !insertmacro swKillRunning
!macroend

!macro customUnInit
  !insertmacro swKillRunning
!macroend

; Explorer context menu: "Convert with Shiftwheel" > PDF / DOCX / TXT / JPG
!macro customInstall
  WriteRegStr HKCU "Software\Classes\.md" "Content Type" "text/markdown"
  WriteRegStr HKCU "Software\Classes\.markdown" "Content Type" "text/markdown"
  WriteRegStr HKCU "${SW_MENU_KEY}" "MUIVerb" "Convert with Shiftwheel"
  WriteRegStr HKCU "${SW_MENU_KEY}" "Icon" '"$INSTDIR\${APP_EXECUTABLE_FILENAME}",0'
  WriteRegStr HKCU "${SW_MENU_KEY}" "SubCommands" ""
  !insertmacro swAddVerb "pdf" "PDF"
  !insertmacro swAddVerb "docx" "DOCX"
  !insertmacro swAddVerb "txt" "TXT"
  !insertmacro swAddVerb "jpg" "JPG"
!macroend

!macro customUnInstall
  ; The old uninstaller also runs during an update: don't remove autostart/menu entries then
  ${ifNot} ${isUpdated}
    DeleteRegKey HKCU "${SW_MENU_KEY}"
    DeleteRegValue HKCU "${SW_RUN_KEY}" "${SW_RUN_NAME}"
    DeleteRegValue HKCU "${SW_APPROVED_KEY}" "${SW_RUN_NAME}"
  ${endIf}
!macroend
