!include nsDialogs.nsh

!define AI_MANAGER_UNINSTALL_ROOT "Software\Microsoft\Windows\CurrentVersion\Uninstall"
!define AI_MANAGER_ELECTRON_UNINSTALL_KEY "a178c25c-9e1d-5bca-9cea-7f005c2da482"

!macro NSIS_HOOK_PREUNINSTALL
  IfSilent aiManagerSkipUserDataCleanup
  MessageBox MB_YESNO|MB_ICONQUESTION "是否同时清空用户数据？$\r$\n$\r$\n选择“是”会删除当前 Data 存放位置下的 workspace 内容。$\r$\n选择“否”仅卸载软件并保留用户数据。" IDYES aiManagerRemoveUserData IDNO aiManagerSkipUserDataCleanup

  aiManagerRemoveUserData:
    ExecWait `powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$$settingsPath = 'D:\ai-manager-data\app-settings.json'; if (Test-Path -LiteralPath $$settingsPath) { $$settings = Get-Content -LiteralPath $$settingsPath -Raw | ConvertFrom-Json; $$dataPath = [Environment]::ExpandEnvironmentVariables([string]$$settings.dataPath); if ($$dataPath) { $$workspacePath = Join-Path $$dataPath 'workspace'; if (Test-Path -LiteralPath $$workspacePath) { Remove-Item -LiteralPath $$workspacePath -Recurse -Force } } }"`

  aiManagerSkipUserDataCleanup:
!macroend

!macro NSIS_HOOK_PREINSTALL
  ReadRegStr $0 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" QuietUninstallString
  ReadRegStr $1 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" QuietUninstallString

  StrCmp $0 "" 0 aiManagerFoundInstalled
  StrCmp $1 "" aiManagerNoInstalled aiManagerFoundInstalled

  aiManagerFoundInstalled:
    IfSilent aiManagerUpgradeInstall
    MessageBox MB_YESNO|MB_ICONQUESTION "检测到当前电脑已经安装旧版本 ${PRODUCTNAME}。$\r$\n$\r$\n选择“是”将先卸载旧版本并继续安装新版本，用户数据会保留。$\r$\n选择“否”将取消本次安装。" IDYES aiManagerUpgradeInstall IDNO aiManagerCancelInstall

  aiManagerCancelInstall:
    Quit

  aiManagerUpgradeInstall:
    StrCmp $0 "" 0 aiManagerUninstallCurrentUser
    StrCpy $0 $1

  aiManagerUninstallCurrentUser:
    ExecWait '$0' $1
    IfErrors aiManagerUninstallFailed
    IntCmp $1 0 aiManagerNoInstalled aiManagerUninstallFailed aiManagerUninstallFailed

  aiManagerUninstallFailed:
    MessageBox MB_OK|MB_ICONEXCLAMATION "旧版本卸载失败，安装已取消。"
    Quit

  aiManagerNoInstalled:
!macroend
