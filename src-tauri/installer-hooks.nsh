!define MUI_LANGDLL_ALWAYSSHOW

!macro NSIS_HOOK_POSTINSTALL
  ${If} $LANGUAGE == ${LANG_SIMPCHINESE}
    WriteRegStr HKCU "Software\io.github.cgfhsc.tracer" "DisplayLanguage" "zh-CN"
    Delete "$SMPROGRAMS\踪.lnk"
    Rename "$SMPROGRAMS\Tracer.lnk" "$SMPROGRAMS\踪.lnk"
  ${Else}
    WriteRegStr HKCU "Software\io.github.cgfhsc.tracer" "DisplayLanguage" "en-US"
    Delete "$SMPROGRAMS\Tracer.lnk"
    Rename "$SMPROGRAMS\踪.lnk" "$SMPROGRAMS\Tracer.lnk"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\io.github.cgfhsc.tracer" "DisplayLanguage"
  DeleteRegKey /ifempty HKCU "Software\io.github.cgfhsc.tracer"
  Delete "$SMPROGRAMS\Tracer.lnk"
  Rename "$SMPROGRAMS\踪.lnk" "$SMPROGRAMS\Tracer.lnk"
!macroend
