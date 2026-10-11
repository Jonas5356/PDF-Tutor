; „Öffnen mit → PDF-Tutor“ für PDF-Dateien, ohne den Standard-PDF-Viewer zu ändern
!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Classes\PDFTutor.pdf" "" "PDF-Dokument"
  WriteRegStr HKCU "Software\Classes\PDFTutor.pdf\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\PDFTutor.pdf\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\.pdf\OpenWithProgids" "PDFTutor.pdf" ""
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe" "FriendlyAppName" "PDF-Tutor"
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".pdf" ""
  WriteRegStr HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\PDFTutor.pdf"
  DeleteRegValue HKCU "Software\Classes\.pdf\OpenWithProgids" "PDFTutor.pdf"
  DeleteRegKey HKCU "Software\Classes\Applications\${MAINBINARYNAME}.exe"
!macroend
