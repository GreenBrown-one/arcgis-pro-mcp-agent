!macro NSIS_HOOK_POSTINSTALL
  CreateShortCut "$DESKTOP\ArcGIS Pro 智能助手.lnk" "$INSTDIR\arcgis-pro-agent-desktop.exe"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\arcgis-pro-agent-desktop.exe" --uninstall-cleanup' $0
  StrCmp $0 0 cleanup_succeeded
  Abort
cleanup_succeeded:
  Delete "$DESKTOP\ArcGIS Pro 智能助手.lnk"
!macroend
