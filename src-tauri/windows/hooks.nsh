; "Open with -> PDF-Tutor" for PDF files, without changing the default PDF viewer
!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Classes\PDFTutor.pdf" "" "PDF document"
  WriteRegStr HKCU "Software\Classes\PDFTutor.pdf\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\PDFTutor.pdf\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\.pdf\OpenWithProgids" "PDFTutor.pdf" ""
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe" "FriendlyAppName" "PDF-Tutor"
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".pdf" ""
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
  ; Sprache des Installers an das Programm weitergeben (nicht bei automatischen Updates,
  ; damit eine im Programm gewählte Sprache erhalten bleibt). 1031 = Deutsch.
  ${If} $UpdateMode <> 1
    FileOpen $0 "$INSTDIR\installer-language.txt" w
    ${If} $LANGUAGE = 1031
      FileWrite $0 "de"
    ${Else}
      FileWrite $0 "en"
    ${EndIf}
    FileClose $0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\installer-language.txt"
  DeleteRegKey HKCU "Software\Classes\PDFTutor.pdf"
  DeleteRegValue HKCU "Software\Classes\.pdf\OpenWithProgids" "PDFTutor.pdf"
  DeleteRegKey HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe"
!macroend
