!macro customInit
  ReadRegStr $0 HKCU "${INSTALL_REGISTRY_KEY}" InstallLocation
  ReadRegStr $1 HKLM "${INSTALL_REGISTRY_KEY}" InstallLocation

  StrCmp $0 "" 0 aiManagerFoundInstalled
  StrCmp $1 "" aiManagerNoInstalled aiManagerFoundInstalled

  aiManagerFoundInstalled:
    MessageBox MB_YESNO|MB_ICONQUESTION "检测到当前电脑已经安装 ${PRODUCT_NAME}。$\r$\n$\r$\n选择“是”将先卸载旧版本并继续安装新版本，用户数据会保留。$\r$\n选择“否”将取消本次安装。" IDYES aiManagerUpgradeInstall IDNO aiManagerCancelInstall

  aiManagerCancelInstall:
    Quit

  aiManagerUpgradeInstall:
    Push "HKEY_CURRENT_USER"
    Call uninstallOldVersion
    IfErrors aiManagerUninstallFailed

    Push "HKEY_LOCAL_MACHINE"
    Call uninstallOldVersion
    IfErrors aiManagerUninstallFailed

    Goto aiManagerNoInstalled

  aiManagerUninstallFailed:
    MessageBox MB_OK|MB_ICONEXCLAMATION "旧版本卸载失败，安装已取消。"
    Quit

  aiManagerNoInstalled:
!macroend

!macro customUnInstallSection
  Section /o "删除用户数据"
    nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$settingsPath = ''D:\ai-manager-data\app-settings.json''; if (Test-Path -LiteralPath $$settingsPath) { $$dataPath = (Get-Content -Raw -LiteralPath $$settingsPath | ConvertFrom-Json).dataPath; if ($$dataPath -and (Test-Path -LiteralPath $$dataPath)) { Remove-Item -LiteralPath $$dataPath -Recurse -Force } }; if (Test-Path -LiteralPath ''D:\ai-manager-data'') { Remove-Item -LiteralPath ''D:\ai-manager-data'' -Recurse -Force }"'
  SectionEnd
!macroend
