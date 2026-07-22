!include nsDialogs.nsh
!include FileFunc.nsh

!define AI_MANAGER_UNINSTALL_ROOT "Software\Microsoft\Windows\CurrentVersion\Uninstall"
!define AI_MANAGER_ELECTRON_UNINSTALL_KEY "a178c25c-9e1d-5bca-9cea-7f005c2da482"
!define AI_MANAGER_TAURI_UNINSTALL_KEY "Monkey Thief"
!define AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY "com.monkeythief.desktop"

Function aiManagerCheckRunningProcesses
  InitPluginsDir
  FileOpen $0 "$PLUGINSDIR\ai-manager-running.ps1" w
  FileWrite $0 "$$processes = @(Get-Process -Name 'Monkey Thief', 'monkey-thief' -ErrorAction SilentlyContinue)$\r$\n"
  FileWrite $0 "if ($$processes.Count -gt 0) { exit 0 }$\r$\n"
  FileWrite $0 "exit 1$\r$\n"
  FileClose $0
  ExecWait '"powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\ai-manager-running.ps1"' $0
  StrCmp $0 0 aiManagerConfirmCloseProcesses aiManagerRunningProcessesDone

  aiManagerConfirmCloseProcesses:
    IfSilent aiManagerStopRunningProcesses
    MessageBox MB_YESNO|MB_ICONQUESTION "检测到 ${PRODUCTNAME} 正在运行。$\r$\n$\r$\n是否关闭正在运行的程序并继续安装？" IDYES aiManagerStopRunningProcesses IDNO aiManagerCancelInstaller

  aiManagerStopRunningProcesses:
    FileOpen $1 "$PLUGINSDIR\ai-manager-stop.ps1" w
    FileWrite $1 "Get-Process -Name 'Monkey Thief', 'monkey-thief' -ErrorAction SilentlyContinue | Stop-Process -Force$\r$\n"
    FileWrite $1 "Start-Sleep -Milliseconds 500$\r$\n"
    FileWrite $1 "if (@(Get-Process -Name 'Monkey Thief', 'monkey-thief' -ErrorAction SilentlyContinue).Count -gt 0) { exit 1 }$\r$\n"
    FileWrite $1 "exit 0$\r$\n"
    FileClose $1
    ExecWait '"powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\ai-manager-stop.ps1"' $0
    StrCmp $0 0 aiManagerRunningProcessesDone aiManagerStopProcessesFailed

  aiManagerStopProcessesFailed:
    MessageBox MB_OK|MB_ICONEXCLAMATION "无法关闭正在运行的 ${PRODUCTNAME}，安装已取消。"
    Quit

  aiManagerCancelInstaller:
    Quit

  aiManagerRunningProcessesDone:
FunctionEnd

Function aiManagerShowUpgradeOptions
  InitPluginsDir
  FileOpen $0 "$PLUGINSDIR\ai-manager-upgrade-options.ps1" w
  FileWrite $0 "Add-Type -AssemblyName System.Windows.Forms$\r$\n"
  FileWrite $0 "Add-Type -AssemblyName System.Drawing$\r$\n"
  FileWrite $0 "$$form = New-Object System.Windows.Forms.Form$\r$\n"
  FileWrite $0 "$$form.Text = '${PRODUCTNAME} 更新'$\r$\n"
  FileWrite $0 "$$form.StartPosition = [System.Windows.Forms.FormStartPosition]::CenterScreen$\r$\n"
  FileWrite $0 "$$form.ClientSize = [System.Drawing.Size]::new(430, 170)$\r$\n"
  FileWrite $0 "$$label = New-Object System.Windows.Forms.Label$\r$\n"
  FileWrite $0 "$$label.Text = '检测到已安装的旧版本。确认后将先卸载旧版本，再继续安装新版本。'$\r$\n"
  FileWrite $0 "$$label.Location = [System.Drawing.Point]::new(16, 16)$\r$\n"
  FileWrite $0 "$$label.Size = [System.Drawing.Size]::new(398, 42)$\r$\n"
  FileWrite $0 "$$checkbox = New-Object System.Windows.Forms.CheckBox$\r$\n"
  FileWrite $0 "$$checkbox.Text = '同时删除用户数据（workspace）'$\r$\n"
  FileWrite $0 "$$checkbox.Location = [System.Drawing.Point]::new(16, 70)$\r$\n"
  FileWrite $0 "$$checkbox.Size = [System.Drawing.Size]::new(398, 24)$\r$\n"
  FileWrite $0 "$$confirm = New-Object System.Windows.Forms.Button$\r$\n"
  FileWrite $0 "$$confirm.Text = '确认'$\r$\n"
  FileWrite $0 "$$confirm.DialogResult = [System.Windows.Forms.DialogResult]::OK$\r$\n"
  FileWrite $0 "$$confirm.Location = [System.Drawing.Point]::new(234, 118)$\r$\n"
  FileWrite $0 "$$confirm.Size = [System.Drawing.Size]::new(82, 30)$\r$\n"
  FileWrite $0 "$$cancel = New-Object System.Windows.Forms.Button$\r$\n"
  FileWrite $0 "$$cancel.Text = '取消'$\r$\n"
  FileWrite $0 "$$cancel.DialogResult = [System.Windows.Forms.DialogResult]::Cancel$\r$\n"
  FileWrite $0 "$$cancel.Location = [System.Drawing.Point]::new(332, 118)$\r$\n"
  FileWrite $0 "$$cancel.Size = [System.Drawing.Size]::new(82, 30)$\r$\n"
  FileWrite $0 "$$form.Controls.Add($$label)$\r$\n"
  FileWrite $0 "$$form.Controls.Add($$checkbox)$\r$\n"
  FileWrite $0 "$$form.Controls.Add($$confirm)$\r$\n"
  FileWrite $0 "$$form.Controls.Add($$cancel)$\r$\n"
  FileWrite $0 "$$form.AcceptButton = $$confirm$\r$\n"
  FileWrite $0 "$$form.CancelButton = $$cancel$\r$\n"
  FileWrite $0 "$$result = $$form.ShowDialog()$\r$\n"
  FileWrite $0 "if ($$result -ne [System.Windows.Forms.DialogResult]::OK) { exit 1 }$\r$\n"
  FileWrite $0 "if ($$checkbox.Checked) { exit 2 }$\r$\n"
  FileWrite $0 "exit 0$\r$\n"
  FileClose $0
  ExecWait '"powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\ai-manager-upgrade-options.ps1"' $0
FunctionEnd

!macro NSIS_HOOK_PREUNINSTALL
  ${GetParameters} $0
  ${GetOptions} $0 "/UPGRADE=" $1
  StrCmp $1 "1" aiManagerUpgradeUninstall

  ; 普通卸载复用 Tauri 卸载页的勾选状态，不再重复询问。
  StrCmp $DeleteAppDataCheckboxState "1" aiManagerRemoveUserData aiManagerSkipUserDataCleanup

  ; 安装器升级时复用升级对话框中的选择，避免再次弹出确认框。
  aiManagerUpgradeUninstall:
  ${GetOptions} $0 "/DELETEUSERDATA=" $1
  StrCmp $1 "1" aiManagerRemoveUserData
  Goto aiManagerSkipUserDataCleanup

  aiManagerRemoveUserData:
    ExecWait `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -Command "$$settingsPath = 'D:\ai-manager-data\app-settings.json'; if (Test-Path -LiteralPath $$settingsPath) { $$settings = Get-Content -LiteralPath $$settingsPath -Raw | ConvertFrom-Json; $$dataPath = [Environment]::ExpandEnvironmentVariables([string]$$settings.dataPath); if ($$dataPath) { $$workspacePath = Join-Path $$dataPath 'workspace'; if (Test-Path -LiteralPath $$workspacePath) { Remove-Item -LiteralPath $$workspacePath -Recurse -Force } } }"`

  aiManagerSkipUserDataCleanup:
!macroend

!macro NSIS_HOOK_PREINSTALL
  Call aiManagerCheckRunningProcesses
  StrCpy $0 ""
  StrCpy $4 ""

  ReadRegStr $0 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" QuietUninstallString
  ReadRegStr $4 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" InstallLocation
  StrCmp $0 "" 0 aiManagerFoundInstalled
  ReadRegStr $0 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" UninstallString
  StrCmp $0 "" 0 aiManagerFoundInstalled

  ReadRegStr $0 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" QuietUninstallString
  ReadRegStr $4 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" InstallLocation
  StrCmp $0 "" 0 aiManagerFoundInstalled
  ReadRegStr $0 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" UninstallString
  StrCmp $0 "" 0 aiManagerFoundInstalled

  ReadRegStr $0 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_UNINSTALL_KEY}" QuietUninstallString
  ReadRegStr $4 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_UNINSTALL_KEY}" InstallLocation
  StrCmp $0 "" 0 aiManagerFoundInstalled
  ReadRegStr $0 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_UNINSTALL_KEY}" UninstallString
  StrCmp $0 "" 0 aiManagerFoundInstalled

  ReadRegStr $0 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_UNINSTALL_KEY}" QuietUninstallString
  ReadRegStr $4 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_UNINSTALL_KEY}" InstallLocation
  StrCmp $0 "" 0 aiManagerFoundInstalled
  ReadRegStr $0 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_UNINSTALL_KEY}" UninstallString
  StrCmp $0 "" 0 aiManagerFoundInstalled

  ReadRegStr $0 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY}" QuietUninstallString
  ReadRegStr $4 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY}" InstallLocation
  StrCmp $0 "" 0 aiManagerFoundInstalled
  ReadRegStr $0 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY}" UninstallString
  StrCmp $0 "" 0 aiManagerFoundInstalled

  ReadRegStr $0 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY}" QuietUninstallString
  ReadRegStr $4 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY}" InstallLocation
  StrCmp $0 "" 0 aiManagerFoundInstalled
  ReadRegStr $0 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY}" UninstallString
  StrCmp $0 "" aiManagerNoInstalled aiManagerFoundInstalled

  aiManagerFoundInstalled:
    StrCpy $R8 $0
    StrCpy $R9 $4
    IfSilent aiManagerSilentUpgradeInstall
    Call aiManagerShowUpgradeOptions
    StrCmp $0 2 aiManagerUpgradeWithDataCleanup
    StrCmp $0 0 aiManagerUpgradeInstall
    Goto aiManagerCancelInstall

  aiManagerUpgradeWithDataCleanup:
    StrCpy $R7 "1"
    Goto aiManagerRestoreUpgradePaths

  aiManagerUpgradeInstall:
    StrCpy $R7 "0"
    Goto aiManagerRestoreUpgradePaths

  aiManagerSilentUpgradeInstall:
    StrCpy $R7 "0"

  aiManagerRestoreUpgradePaths:
    StrCpy $0 $R8
    StrCpy $4 $R9
    Goto aiManagerUninstallCurrentUser

  aiManagerCancelInstall:
    Quit

  aiManagerUninstallCurrentUser:
    DetailPrint "正在卸载旧版本 ${PRODUCTNAME}..."
    StrCmp $R7 "1" aiManagerUninstallWithDataCleanup aiManagerUninstallKeepData

  aiManagerUninstallWithDataCleanup:
    StrCmp $4 "" aiManagerUninstallWithoutInstallDirWithDataCleanup
    ExecWait '$0 /S /UPGRADE=1 /DELETEUSERDATA=1 _?=$4' $1
    Goto aiManagerCheckUninstallResult

  aiManagerUninstallWithoutInstallDirWithDataCleanup:
    ExecWait '$0 /S /UPGRADE=1 /DELETEUSERDATA=1' $1
    Goto aiManagerCheckUninstallResult

  aiManagerUninstallKeepData:
    StrCmp $4 "" aiManagerUninstallWithoutInstallDir
    ExecWait '$0 /S /UPGRADE=1 _?=$4' $1
    Goto aiManagerCheckUninstallResult

  aiManagerUninstallWithoutInstallDir:
    ExecWait '$0 /S /UPGRADE=1' $1

  aiManagerCheckUninstallResult:
    IfErrors aiManagerUninstallFailed
    IntCmp $1 0 aiManagerWaitOldUninstall aiManagerUninstallFailed aiManagerUninstallFailed

  aiManagerWaitOldUninstall:
    StrCpy $6 0

  aiManagerWaitOldUninstallLoop:
    Sleep 1000
    IntOp $6 $6 + 1
    ReadRegStr $7 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" UninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" QuietUninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" UninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_ELECTRON_UNINSTALL_KEY}" QuietUninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_UNINSTALL_KEY}" UninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_UNINSTALL_KEY}" QuietUninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_UNINSTALL_KEY}" UninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_UNINSTALL_KEY}" QuietUninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY}" UninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKCU "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY}" QuietUninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY}" UninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    ReadRegStr $7 HKLM "${AI_MANAGER_UNINSTALL_ROOT}\${AI_MANAGER_TAURI_IDENTIFIER_UNINSTALL_KEY}" QuietUninstallString
    StrCmp $7 "" 0 aiManagerStillUninstalling
    StrCmp $4 "" aiManagerNoInstalled
    IfFileExists "$4\${PRODUCTNAME}.exe" aiManagerStillUninstalling aiManagerNoInstalled

  aiManagerStillUninstalling:
    IntCmp $6 120 aiManagerUninstallFailed aiManagerWaitOldUninstallLoop aiManagerUninstallFailed

  aiManagerUninstallFailed:
    MessageBox MB_OK|MB_ICONEXCLAMATION "旧版本卸载失败，安装已取消。"
    Quit

  aiManagerNoInstalled:
!macroend
