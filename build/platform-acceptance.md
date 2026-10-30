# macOS / Linux 全模块实机验收

代码入口和自动测试已接入。当前开发环境是 Windows，仓库没有已通过的 macOS / Linux 全模块实机记录。下面的流程用于在真实桌面与实际安装包上验收，CI 的端口探针和单元测试不能替代桌面权限、跨设备及安装卸载结果。

## 环境矩阵

每个环境分别执行并保存独立报告：

| 环境 | 必填信息 | 桌面权限与工具 |
| --- | --- | --- |
| macOS Apple Silicon | 系统版本、芯片、显示器与缩放 | 屏幕录制、辅助功能；System Events 取词时允许自动化 |
| macOS Intel | 系统版本、芯片、显示器与缩放 | 同上 |
| Linux X11 | 发行版、桌面版本、X11、多屏与缩放 | `lsof`、`xclip`，GTK/WebKitGTK 运行库 |
| Linux Wayland | 发行版、合成器和门户后端版本、Wayland、多屏与缩放 | PipeWire、匹配的 `xdg-desktop-portal-*`、`wl-clipboard`、`lsof` |

Wayland 后端至少提供 ScreenCast。全局快捷键需要 GlobalShortcuts，自动滚动和模拟复制分别需要 RemoteDesktop 的鼠标和键盘授权。后端缺少接口时，记录不支持的具体场景及错误；该项保留失败或待验收，不能写成全部通过。

首次验收使用可恢复的测试数据目录、临时仓库与测试账号；不要用日常工作目录验证覆盖恢复、结束进程或卸载清理。Windows 数据根目录仍按 D 盘优先、其他可写盘及用户目录兜底的既有规则工作。

## 生成自动与原生记录

在同一版本源码与同一台机器上执行。`--output` 必须指向空目录，已有记录不会被覆盖。示例使用当前用户目录：

```sh
npm ci
npm run test:acceptance -- --output "$HOME/ai-manager-acceptance/auto" --build
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --features desktop-acceptance --bin desktop-acceptance -- \
  --output "$HOME/ai-manager-acceptance/native" --ports --clipboard --capture
```

自动回归按项目现有脚本逐项运行，先生成 Tauri 资源所需的翻译 Agent，再运行全部 Rust 库测试、原生探针编译与前端构建；单项失败会保留日志并继续收集后续结果，最终返回非零状态。失败项还会把日志最后 120 行输出到 CI，便于直接定位构建错误。

原生探针验证本进程的 loopback 端口识别与自身结束保护、剪贴板读取或图片往返、同一会话连续三帧采集。Wayland 剪贴板探针只读，先复制一段测试文字或图片；写入与复制恢复由后面的人工场景验证。macOS/X11 图片往返后尝试恢复原剪贴板。采集会保存桌面 PNG，请在测试桌面运行并按团队要求保管证据。Windows 运行原生探针会明确返回不支持。

报告结构：

```text
ai-manager-acceptance/
  auto/
    automated-report.json
    manual-report.json
    test-*.log
    rust-all-modules.log
    native-probe-build.log
    renderer-*.log
  native/
    native-report.json
    desktop-frame-1.png
    desktop-frame-2.png
    desktop-frame-3.png
```

`--renderer-only` 可用于排查前端；缺少 Rust 或构建项目时不能通过完整实机记录校验。CI 只运行不需要桌面授权的原生端口探针，截图与剪贴板项目不会被计为通过。

## 全模块操作清单

使用该机器实际安装的 DMG/App、AppImage 或 DEB，记录安装包文件及 SHA256。在 `auto/manual-report.json` 填入 `hardware`、`osRelease` 与 `desktopSession`，分别使用 `aqua`、`x11`、`wayland`。每项实际执行后填 `passed` 或 `failed`，保留操作步骤、预期和实际结果及证据文件。没有执行的保持 `pending`。

| 项目 ID | 必须验证的场景 |
| --- | --- |
| installation | 安装与首启、权限同意及取消后重试、托盘显示/退出、自启动开关、实际旧版更新至待测版；最后验证卸载取消保留数据、确认卸载清理范围 |
| providers-runtime | 新建/编辑 Provider 与账号、切换模型和配置、CLI PATH 发现、终端启动与含空格路径；Claude Desktop 配置、会话与 Skills 读写 |
| skills-rules | Skills 仓库导入和刷新、本地目录与链接、启停；Rules/Prompt 保存、预览、启用并检查目标应用文件 |
| sessions-usage | 各 CLI 会话读取、搜索、删除/回收恢复；调用后检查日志、Token、价格和账号归属 |
| git-tools | 临时仓库状态、差异、分支/提交、受控冲突处理；图片生成和输出保存；各工具导出 |
| data-settings | 修改数据根目录后重启，备份、恢复、云同步、日志开关与清理；确认业务数据写入配置目录 |
| network | 系统代理切换及恢复，网络检测；列出自建测试子进程端口并关闭它；自身及受保护进程不可关闭 |
| lan-share | 两台不同系统设备配对，双向文字、图片、文件、群聊和中断后续传；删除设备保留共享群聊历史及原始文件 |
| screenshot | 单屏/多屏、混合 DPI；X11/macOS 额外检查负坐标副屏；框选、矩形检测、编辑、内置与大模型 OCR、复制图片/文件、贴图分组/拖动/缩放/菜单、重启恢复 |
| long-screenshot | 手动滚动与自动滚动、完成和取消、改变分辨率、切换目标应用；Wayland 单次会话复用授权、隐藏画布后快捷键完成、撤回共享后失败并能重新开始 |
| translation | 不同应用取词与剪贴板翻译，验证取词后原剪贴板优先格式恢复；期间复制非文字内容不被覆盖；权限取消重试、窗口显示/拖动、快捷键修改与重新授权 |
| floating-window | 桌宠和悬浮工具拖动、收起展开、页面/工具跳转、失焦与多屏；Wayland 原生拖动只启动一次，隐藏后迟到响应不能重新展开 |

Wayland 按支持的交互验收：共享画面按授权顺序横向排列；输入坐标映射到对应流。窗口位置交给合成器，贴图方向键微移会提示拖动，悬浮窗不承诺贴边停靠。置顶和穿透受合成器影响，必须记录实际表现。无法满足必需场景时应保留失败，并附上后端版本及复现步骤。

手工项目示例（只在实际完成后填写，下面不是实机结果）：

```json
{
  "id": "long-screenshot",
  "status": "passed",
  "notes": "记录目标应用、快捷键、授权次数、滚动方式、停止共享后的错误与重试结果",
  "evidence": [
    { "file": "../evidence/long-capture.log", "sha256": "该文件的64位SHA256" }
  ]
}
```

安装包记录同样填写到 `package.file` 与 `package.sha256`。证据路径相对相应 JSON 文件，绝对路径也可使用。可通过下面的命令读取文件摘要：

```sh
node --input-type=module -e 'import {hashFile} from "./scripts/verify-platform-acceptance.mjs"; console.log(await hashFile(process.argv[1]))' "/path/to/evidence.log"
```

## 校验与关闭验收项

完成所有操作后运行：

```sh
npm run test:acceptance:verify -- \
  --automated "$HOME/ai-manager-acceptance/auto/automated-report.json" \
  --native "$HOME/ai-manager-acceptance/native/native-report.json" \
  --manual "$HOME/ai-manager-acceptance/auto/manual-report.json" \
  --output "$HOME/ai-manager-acceptance/summary.json"
```

校验器检查系统/架构/应用版本/桌面会话一致、全部自动项目及退出状态、原生三项探针、连续采集文件、实测安装包、12 项人工记录与文件 SHA256。缺项、待执行、失败或文件变化均返回非零；汇总文件已存在时拒绝覆盖。

汇总中的 `fullAcceptance=true` 只表示该机器的报告完整且结果通过，人工操作真实性由验收人负责，不代表其他机器或平台通过。将系统、架构、桌面会话、源码提交/工作区改动与安装包摘要一同登记，分别保存四类环境的证据后，才关闭 README 中的 macOS/Linux 实机全模块验收项。CI 产物名称为 `acceptance-<target>`，可用于定位自动失败，不作为完整实机报告。
