; アンインストール前に、Clarivo が変えたアプリ別の出力先と既定デバイスを元に戻す
!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\clarivo.exe" --reset-audio'
!macroend
