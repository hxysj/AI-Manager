# 项目协作规则

## Git 规则

- 无论如何都不能自动提交 git，只有用户明确要求提交、打 tag 或推送时才能执行。
- 无论如何都不能自动取消暂存区内容，禁止自动执行 `git restore --staged`、`git reset` 或其他会改变 staged 状态的命令；只有用户明确要求时才能改动暂存区。
- commit message 必须使用 `feat:`、`fix:`、`chore:` 等规范前缀开头，并使用中文描述。
- 用户要求提交当前内容时，提交信息要记录本次修改的所有主要内容，更新或修复的内容，不要过于冗余。
- 提交信息的 `-m` 内容不要写版本号信息，版本号只体现在 `package.json`、`package-lock.json`、`build/release-notes.md` 和 tag 中。
- 多内容提交必须使用多段 `-m`，例如：

```powershell
git commit -m "feat: 中文摘要" -m "- 修改内容一。" -m "- 修改内容二。"
```

- 用户要求“改一个版本”时，需要同步更新 `package.json`、`package-lock.json` 和 `build/release-notes.md`。
- `build/release-notes.md` 需要写清楚具体变更和带来的作用，不要只简单罗列功能名称，需要具体到上一次打tag的所有提交内容。
- `build/release-notes.md` 只保留最新版本更新说明，格式固定为 `## 变更说明` 开头，后续内容使用 `- 标题。` 加下一行两个空格缩进详细内容的列表格式。
- 用户要求打 tag 时，tag 使用版本号格式，例如 `v0.1.2`。
- 打 tag 后必须用 `git tag --points-at HEAD` 或 `git show-ref --tags <tag>` 确认 tag 已创建并指向当前提交。
- 用户要求推送时，需要推送当前分支和对应 tag。

## Tauri 桌面端规则

- 当前项目以 Tauri + Vue 3 + Less 桌面端应用为主，不再按网页端交互处理。
- 前端代码在 `src/` 目录，后端 Rust 代码在 `src-tauri/src/` 目录。
- Tauri 后端按模块组织：`core/` 放核心功能（路径、设置、状态），`api/` 放前端调用的命令接口。

## 工作区数据目录规则

- 用户数据和应用运行数据必须统一写入配置的数据目录（默认 `D:\ai-manager-data`），不得散落到 Tauri 的 AppData 应用私有目录。
- 所有工作区路径统一以 `src-tauri/src/core/paths.rs` 中的 `AppPaths` 结构为准，新增数据路径必须先在这里明确目录归属。
- `workspace/` 是应用业务数据根目录，只负责承载下级业务目录，不直接散放业务文件。
- `workspace/storage/` 只放可以云备份、跨设备恢复后仍然有意义的持久化配置和索引数据，例如 Provider 配置、模型费用配置、Skill 仓库地址、Prompt 索引等。
- `workspace/storage/` 不允许新增日志、运行时扫描结果、临时缓存、会话快照、机器相关状态、可重新拉取的数据或只对当前设备有效的数据。
- `workspace/logs/` 只放日志类数据，例如请求日志、用量请求记录、应用调用日志等；该目录不进入云备份。
- `workspace/temp/` 只放运行时缓存和可重新生成的数据，例如 GitHub Skill 扫描结果、会话索引缓存等；该目录不进入云备份，代码必须允许它被删除后重新生成。
- `workspace/skills/` 放已安装到本地的 Skill 实际内容，属于用户可保留数据，可以进入云备份。
- `workspace/prompts/` 放 Prompt 内容和按 CLI 分类的 Prompt 数据，属于用户可保留数据，可以进入云备份。
- `workspace/profiles/` 放 Prompt 配置档案，属于用户可保留数据，可以进入云备份。
- `workspace/repos/` 放仓库管理使用的本地仓库目录或仓库工作数据，不进入云备份；需要同步的仓库配置应写入 `storage` 中的配置文件。
- `workspace/sessions/` 放 CLI 原始会话、会话回收站和相关本地会话文件，不进入云备份。
- `workspace/git-tool/` 放 Git 工具归档、检查缓存等本地数据，不进入坚果云等云备份；只有本地备份明确包含 Git Tool 数据时才允许打包。
- 新增持久化文件前必须先判断它是“可云备份配置”、“日志”还是“可重新生成缓存”，不要为了读取方便把所有 JSON 都放进 `storage`。
- 新增云备份内容必须在 `collectBackupEntries()` 中显式加入；新增非云备份内容不得加入 `collectBackupEntries()`，也不得依赖后续迁移来修正目录放错的问题。
- 如果确实要调整已有数据文件目录，必须同时补充启动兼容迁移和备份恢复后的兼容迁移；但新增数据文件必须一开始就放在正确目录，避免后续再做迁移补救。

## 前端规则

- Vue 代码使用现有项目风格，避免全局格式化无关文件。
- 优先复用现有组件、工具函数、接口封装和页面结构。
- UI 修改必须符合当前桌面端布局体系。

## 后端接口规则

- GET 请求使用 `params/query` 传参。
- POST/PUT/PATCH/DELETE 请求使用 `data/body` 传参。
- 不要把业务参数直接拼接到接口路径中。
- 接口只返回前端当前需要的数据，不要把读取到的完整数据一股脑返回给前端。
- 所有后端响应保持 `{ status, data, message }` 结构。
