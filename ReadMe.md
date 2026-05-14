# AI Manager 全模块详细规则文档（V1 架构规范）

技术栈：

```text id="ud5ol3"
Electron
Vue3
LESS
Node.js
JSON Storage
```

架构目标：

```text id="rm1f6q"
统一管理 AI CLI 的：
- Skills
- Sessions
- Configs
- Workspaces
```

当前阶段：

```text id="z7r1jx"
CLI Detection
+
Skill System
```

为核心。

---

# 一、系统模块总览

```text id="4z0r1q"
AI Manager
│
├── CLI Detection
├── Skill System
├── Repo System
├── Skill Scanner
├── Metadata Parser
├── Skill Registry
├── Link Manager
├── File Watcher
├── Session System
├── JSON Storage
├── IPC Layer
└── UI Layer
```

---

# 二、CLI Detection 模块规则

模块：

```text id="fhv2zt"
CliDetectionService
```

职责：

```text id="2i2z8q"
自动检测当前机器已安装的 AI CLI
```

---

# 三、CLI Detection 数据结构

```ts id="6w5j31"
interface CliTarget {
  id: string

  type: "claude" | "codex" | "gemini" | "opencode"

  name: string

  installed: boolean

  executablePath?: string

  configPath?: string

  skillsPath?: string

  sessionsPath?: string

  version?: string

  detectedAt: number
}
```

---

# 四、CLI Detection 规则

## 1. Claude 检测规则

满足任意：

```text id="42s1x6"
存在：
%USERPROFILE%\.claude
```

或者：

```bash id="umv3qy"
where claude
```

成功。

---

## 2. Claude 路径规则

# configPath

```text id="iwjlwm"
%USERPROFILE%\.claude
```

---

# skillsPath

```text id="yjlwmz"
%USERPROFILE%\.claude\skills
```

---

# sessionsPath

```text id="jlwmxx"
%USERPROFILE%\.claude\projects
```

---

## 3. Codex 检测规则

检测：

```text id="2jlwmw"
%USERPROFILE%\.codex
```

或：

```bash id="3klwmv"
where codex
```

---

## 4. Gemini 检测规则

检测：

```text id="4llwmu"
%USERPROFILE%\.gemini
```

或：

```bash id="5mlwmt"
where gemini
```

---

## 5. CLI 判定规则

满足：

```text id="6nlwms"
config exists
OR
binary exists
```

即可视为：

```text id="7olwmr"
installed
```

---

# 五、CLI Adapter 模块规则

模块：

```text id="8plwmq"
Adapters
```

职责：

```text id="9qlwmp"
统一不同 CLI 的行为差异
```

---

# 六、Adapter 接口规范

```ts id="arlwmo"
interface CliAdapter {
  detect(): Promise<boolean>

  getConfigPath(): string

  getSkillsPath(): string

  getSessionsPath(): string

  installSkill(skill): Promise<void>

  uninstallSkill(skill): Promise<void>
}
```

---

# 七、Skill System 模块规则

模块：

```text id="brlwmn"
SkillSystem
```

职责：

```text id="crlwmm"
管理所有 Skill 生命周期
```

---

# 八、Skill 数据结构规则

```ts id="drlwml"
interface Skill {
  id: string

  name: string

  description?: string

  version?: string

  author?: string

  tags?: string[]

  icon?: string

  entry?: string

  repoId?: string

  sourcePath: string

  installedTargets: string[]

  createdAt: number

  updatedAt: number
}
```

---

# 九、Skill Source 规则（重要）

规则：

```text id="erlwmk"
AI Manager 永远持有真实 Skill
```

CLI：

```text id="frlwmj"
只允许引用
```

禁止：

```text id="grlwmi"
复制 Skill 到 CLI
```

---

# 十、Skill 文件结构规则

合法 Skill：

```text id="hrlwmh"
必须包含：
SKILL.md
```

推荐结构：

```text id="irlwmg"
skill/
├── SKILL.md
├── prompt.md
├── icon.png
├── examples/
├── rules/
└── context/
```

---

# 十一、Skill Scanner 模块规则

模块：

```text id="jrlwmf"
SkillScanner
```

职责：

```text id="krlwme"
扫描合法 Skill
```

---

# 十二、扫描规则（核心）

## 1. 命中规则

目录存在：

```text id="lrlwmd"
SKILL.md
```

即：

```text id="mrlwmc"
视为 Skill Root
```

---

## 2. 命中后规则

命中：

```text id="nrlwmb"
SKILL.md
```

后：

```text id="orlwma"
停止继续递归
```

避免：

```text id="prlwm9"
嵌套 Skill
```

---

## 3. 最大递归深度

```text id="qrlwm8"
maxDepth = 6
```

---

## 4. 忽略目录

```text id="rrlwm7"
.git
node_modules
dist
build
.cache
temp
```

---

## 5. Symlink 规则

扫描时：

```text id="srlwm6"
禁止 follow symlink
```

避免：

- 循环扫描
- 无限递归

---

# 十三、Skill Name 规则

## 1. name 必须存在

来自：

```yaml id="trlwm5"
name:
```

---

## 2. name 全局唯一

禁止：

```text id="urlwm4"
两个 skill 同名
```

因为：

```text id="vrlwm3"
CLI symlink 会冲突
```

---

# 十四、Metadata Parser 规则

模块：

```text id="wrlwm2"
MetadataParser
```

职责：

```text id="xrlwm1"
解析 SKILL.md Frontmatter
```

---

# 十五、SKILL.md 规则（核心）

格式：

```md id="yrlwm0"
---
name: Translate
description: Translate text
version: 1.0.0
author: xxx

tags:
  - translate

icon: icon.png

entry: prompt.md
---

# Skill
```

---

# 十六、Metadata 字段规则

## 必须字段

```yaml id="zrlwlz"
name
```

---

## 可选字段

```yaml id="0slwly"
description
version
author
tags
icon
entry
homepage
repository
```

---

## entry 规则

默认：

```text id="1tlwlx"
SKILL.md
```

如果：

```yaml id="2ulwlw"
entry: prompt.md
```

则：

```text id="3vlwlv"
使用 prompt.md
```

---

# 十七、Icon 规则

优先级：

# 1

Frontmatter：

```yaml id="4wlwlu"
icon: icon.png
```

---

# 2

默认：

```text id="5xlwlt"
icon.png
```

---

# 3

fallback：

```text id="6ylwls"
default icon
```

---

# 十八、Skill Registry 规则

模块：

```text id="7zlwlr"
SkillRegistry
```

职责：

```text id="80lwlq"
统一维护所有 Skill 索引
```

---

# 十九、Registry 行为规则

扫描后：

```text id="91lwlp"
自动注册 Skill
```

删除后：

```text id="a2lwlo"
自动移除 Registry
```

---

# 二十、Registry 唯一规则

唯一键：

```text id="b3lwln"
skill.name
```

禁止重复。

---

# 二十一、Repo System 规则

模块：

```text id="c4lwlm"
RepoSystem
```

职责：

```text id="d5lwll"
管理 Skill 仓库
```

---

# 二十二、Repo 类型规则

支持：

```text id="e6lwlk"
github
git
local
```

最终统一：

```text id="f7lwlj"
local filesystem
```

---

# 二十三、Repo Clone 规则

Git Repo：

```text id="g8lwli"
clone 到：
AIManager/repos/
```

---

# 二十四、Repo Pull 规则

更新：

```text id="h9lwlh"
git pull
↓
重新扫描
↓
刷新 registry
```

---

# 二十五、Repo 删除规则

必须：

# 第一步

删除：

```text id="ialwlg"
所有 symlink
```

---

# 第二步

删除 repo。

---

# 第三步

刷新 registry。

---

# 二十六、Link Manager 规则（核心）

模块：

```text id="jblwlf"
LinkManager
```

职责：

```text id="kclwle"
将 Skill 挂载到 CLI
```

---

# 二十七、Link 规则（最重要）

禁止：

```text id="ldlwld"
copy mode
```

必须：

```text id="melwlc"
junction mode
```

---

# 二十八、Windows Link 规则

使用：

```ts id="nflwlb"
fs.symlink(source, target, "junction")
```

禁止：

```text id="oglwla"
hardlink
```

---

# 二十九、Install 规则

安装：

```text id="phlwl9"
创建 symlink
```

卸载：

```text id="qilwl8"
删除 symlink
```

禁止删除：

```text id="rjlwl7"
真实 Skill Source
```

---

# 三十、Link Path 规则

Claude：

```text id="sklwl6"
~/.claude/skills/{skillName}
```

Codex：

```text id="tllwl5"
~/.codex/skills/{skillName}
```

---

# 三十一、File Watcher 规则

模块：

```text id="umlwl4"
FileWatcher
```

职责：

```text id="vnlwl3"
监听 Skill 文件变化
```

---

# 三十二、监听目录规则

监听：

```text id="wolwl2"
skills/
repos/
```

---

# 三十三、监听行为规则

## SKILL.md 修改

```text id="xplwl1"
重新解析 metadata
```

---

## prompt.md 修改

无需处理。

CLI 自动读取最新。

---

## 删除 Skill

```text id="yqlwl0"
自动清理 dead link
```

---

# 三十四、Session 模块规则（预留）

模块：

```text id="zrkwlz"
SessionSystem
```

职责：

```text id="0slwky"
聚合 CLI Session
```

---

# 三十五、Session 检测规则

Claude：

```text id="1tlwkx"
~/.claude/projects
```

Codex：

后续扩展。

---

# 三十六、JSON Storage 规则

模块：

```text id="2ulwkw"
JsonStorage
```

职责：

```text id="3vlwkv"
统一 JSON 数据读写
```

---

# 三十七、JSON 文件规则

# repos.json

```text id="4wlwku"
Repo 信息
```

---

# skills.json

```text id="5xlwkt"
Skill Registry
```

---

# installs.json

```text id="6ylwks"
安装关系
```

---

# cli-targets.json

```text id="7zlwkr"
CLI 检测结果
```

---

# 三十八、JSON 写入规则（重要）

禁止：

```text id="80lwkq"
频繁实时写入
```

必须：

```text id="91lwkp"
debounce write
```

例如：

```text id="a2lwko"
300ms
```

否则：

- IO 爆炸
- Electron 卡顿

---

# 三十九、IPC 模块规则

模块：

```text id="b3lwkn"
IPC Layer
```

职责：

```text id="c4lwkm"
Renderer 与 Main 通信
```

---

# 四十、IPC 规则

禁止：

```text id="d5lwkl"
Renderer 直接访问 fs
```

所有：

```text id="e6lwkk"
文件操作
git 操作
symlink 操作
```

必须：

```text id="f7lwkj"
Main Process
```

执行。

---

# 四十一、UI 模块规则

模块：

```text id="g8lwki"
Vue3 UI
```

---

# 四十二、UI Layout 规则

推荐：

```text id="h9lwkh"
左侧：
- CLI
- Skills
- Repos

右侧：
- Detail
- Install
- Status
```

---

# 四十三、Skill Card 显示规则

显示：

```text id="ialwkg"
icon
name
description
repo
tags
targets
```

---

# 四十四、状态规则

Skill 状态：

```text id="jblwkf"
installed
not installed
broken link
updating
disabled
```

---

# 四十五、Broken Link 规则

如果：

```text id="kclwke"
symlink target 不存在
```

则：

```text id="ldlwkd"
标记 broken
```

并允许：

```text id="melwkc"
repair
```

---

# 四十六、最终核心原则（最重要）

你的系统：

必须坚持：

```text id="nflwkb"
Centralized Skill Source
```

CLI：

永远：

```text id="oglwka"
只做引用
```

不要退回：

```text id="phlwk9"
copy sync
```

否则后面：

- 多副本漂移
- Skill 不一致
- 更新混乱
- Session 错乱

问题会越来越严重。
