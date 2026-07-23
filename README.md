<div align="center">
  <img src="src/assets/ai-manager-logo.svg" width="104" height="104" alt="Monkey Thief 图标" />
  <h1>Monkey Thief</h1>
  <p>面向 Claude Code 与 Codex 的 Windows 桌面管理工具，集中管理 Provider、官方账号、Skills、Prompt、会话、模型用量和本地工作流。</p>
  <p>
    <a href="https://github.com/hxysj/AI-Manager/releases/latest"><strong>下载最新安装包</strong></a>
    ·
    <a href="#本地开发">本地开发</a>
    ·
    <a href="#数据与备份">数据与备份</a>
  </p>
</div>

> [!NOTE]
> 当前桌面端、安装器和自动发布流程以 Windows 为目标平台，发布产物为 NSIS 安装包。

## 功能概览

| 模块 | 能力 |
| --- | --- |
| Provider 管理 | 统一维护 Claude Code、Codex 的兼容 Provider、模型映射和 Runtime 配置，检测本地配置漂移并支持快速切换。 |
| Codex 官方账号 | 支持官方登录、`auth.json` 导入、多账号维护、额度刷新、代理配置和账号接管。 |
| Proxy 接管 | 为 Claude Code 与 Codex 管理本机代理服务和接管池，可在多个 Provider 或 Codex 账号之间切换。 |
| Usage 统计 | 从会话与请求记录同步用量，按时间、应用、Provider、来源和模型筛选，展示 Token、缓存命中、费用趋势及明细，并可导出 PNG 长图。 |
| Skills 管理 | 集中维护 Skill 源目录、仓库、分组和回收站，支持新建、ZIP/CLI 导入、批量安装、卸载、启用与禁用。 |
| Sessions 管理 | 聚合 Claude Code 与 Codex 本地会话，支持全文筛选、按项目分页、按需查看消息和会话回收站。 |
| Prompt Rules | 管理公共或 CLI 专属 Prompt，支持导入现有全局 Prompt、按目标启用、Runtime 状态检查和内容差异对比。 |
| Git 管理 | 管理项目的本地分支归档、提交检查和 stash 归档。 |
| 设备快传 | 在局域网启动共享服务，通过二维码连接设备，支持单聊、群聊、文件共享、下载和文本预览。 |
| 实用工具 | 按需启动本机浏览器工具面板，内置字符串/JSON 差异对比和图片链接提取。 |
| 桌面集成 | 提供系统托盘、Provider 快速切换悬浮窗、开机启动、关闭行为设置和应用内更新。 |
| 数据管理 | 支持加密导出、恢复预览、定时本地备份，以及通过坚果云或 Koofr WebDAV 上传、查看和恢复备份。 |

当前内置检测和管理的 CLI：

- Claude Code
- Codex

## 安装与使用

1. 从 [GitHub Releases](https://github.com/hxysj/AI-Manager/releases/latest) 下载 `Monkey-Thief-windows-setup.exe`。
2. 运行安装程序并启动 Monkey Thief。
3. 在 `Settings > 目录` 中确认数据目录和 Claude Code、Codex 配置目录；应用会自动检测已安装的 CLI。
4. 在 `Providers` 中添加兼容 Provider，或为 Codex 添加官方账号。
5. 根据需要进入 `Skills`、`Sessions`、`Rules` 和 `Usage` 完成管理与统计。

> [!TIP]
> 数据目录修改后需要重启应用才会生效。默认目录为 `D:\ai-manager-data`。

## 本地开发

### 环境要求

- Windows 10/11
- Node.js 20 或更高版本
- Rust stable 工具链
- Microsoft C++ Build Tools、Windows SDK 与 WebView2 Runtime
- 已安装 Claude Code 或 Codex 时，才能使用对应 CLI 的扫描和 Runtime 管理能力

### 启动项目

```powershell
git clone https://github.com/hxysj/AI-Manager.git
cd AI-Manager
npm ci
npm run dev
```

Tauri 开发窗口使用 `http://127.0.0.1:5173` 作为前端开发服务。仅调试界面时可以运行 `npm run dev:renderer`，但依赖 Tauri IPC 的功能需要通过 `npm run dev` 使用。

### 构建安装包

正式构建会生成应用内更新配置，需要先提供可访问更新仓库的 GitHub Token：

```powershell
$env:AI_MANAGER_GITHUB_TOKEN = "你的 GitHub Token"
npm run dist:win
```

也可以在项目根目录创建不会被 Git 跟踪的 `.env`：

```dotenv
AI_MANAGER_GITHUB_TOKEN=你的 GitHub Token
```

NSIS 安装包输出到：

```text
src-tauri/target/release/bundle/nsis/
```

### 常用命令

| 命令 | 作用 |
| --- | --- |
| `npm run dev` | 启动 Vite 与 Tauri 桌面开发环境。 |
| `npm run dev:renderer` | 仅启动前端开发服务器。 |
| `npm run build:renderer` | 构建 Vue 前端，用于快速验证前端编译。 |
| `npm run build` | 生成更新配置并执行 Tauri 构建。 |
| `npm run dist:win` | 构建 Windows NSIS 安装包和更新产物。 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 运行 Rust 后端测试。 |

## 数据与备份

默认业务数据位于 `D:\ai-manager-data`，可以在设置中调整。主要目录如下：

```text
D:\ai-manager-data\
├─ app-settings.json          # 应用设置和数据目录入口
├─ models\                    # 本地模型缓存
└─ workspace\
   ├─ storage\                # 配置、索引和主数据库
   │  └─ ai-manager.db        # Provider、账号、Skills、Prompt、Usage 等 SQLite 表
   ├─ skills\                 # 本地 Skill 实际内容
   ├─ prompts\                # Prompt 内容
   ├─ profiles\               # Prompt 配置档案
   ├─ logs\                   # 应用与请求日志
   ├─ temp\                   # 可重新生成的扫描和索引缓存
   ├─ repos\                  # 仓库工作目录
   ├─ sessions\               # 会话与会话回收站
   ├─ git-tool\               # Git 工具归档和本地数据
   └─ lan-share\              # 局域网快传状态、消息和下载记录
```

应用启动时会将旧版 Provider、Codex 账号、Skill、Prompt Rule、Usage JSON 或旧 Usage 数据库迁移到主数据库；只有迁移事务成功后才会删除对应旧数据。

备份文件使用 `.aimbackup` 加密格式。手动导出、本地自动备份和 WebDAV 云端备份使用同一份白名单，只包含：

- Provider 配置、模型和加密保存的 API Key。
- Codex 官方账号登录信息。
- Skills 索引、分组、仓库配置及 `workspace/skills` 中的实际内容。
- Rules 索引及 `workspace/prompts` 中的实际内容。
- 设置模块中的坚果云与 Koofr WebDAV 地址、账号、应用密码和备份文件名。

Usage、Tools、Sessions、代理配置与请求日志、Git 工具归档、仓库工作目录、缓存、回收站和设备 Runtime 状态均不进入备份。恢复时，当前已存在的 Provider 和 Skill 保留本机启用及安装状态；备份中新增的 Provider 和 Skill 默认禁用；当前启用项若不在恢复结果中，会清空对应活动状态，由用户手动重新启用。

## 技术栈

- Vue 3 + Vite
- Element Plus + ECharts + Monaco Editor + Lucide Icons
- Tauri 2 + Rust + Tokio
- SQLite（`rusqlite`，WAL 模式）
- 本地 HTTP、WebSocket 与 WebDAV
- Xenova Transformers（本地英译中模型）

## 项目结构

```text
AI-Manager/
├─ src/
│  ├─ api/                    # 前端 IPC 请求封装
│  ├─ components/             # 跨模块桌面组件
│  ├─ features/               # Providers、Usage、Skills、Rules、Tools 等功能模块
│  └─ styles/                 # 全局 Less 样式
├─ src-tauri/
│  ├─ node/                   # 随应用分发的 Node 辅助服务
│  └─ src/
│     ├─ api/                 # Tauri 命令与业务接口
│     └─ core/                # 路径、数据库、设置和状态管理
├─ build/                     # 图标、NSIS Hooks 与发布说明
├─ scripts/                   # 构建辅助脚本
└─ .github/workflows/         # Windows Release 自动发布流程
```

前端通过统一的 `dispatch_api` Tauri 命令按 channel 调用 Rust 后端；后端在 `core` 层集中处理路径、持久化和运行状态，在 `api` 层实现具体业务能力。
