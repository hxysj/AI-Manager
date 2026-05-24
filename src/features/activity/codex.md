# Codex Runtime 实时监控最终方案

## 目标

实现生产级的 Codex Runtime Observability System，覆盖以下能力：

- 实时任务状态。
- 实时 token stream。
- Tool 生命周期。
- 子 Agent 状态。
- Timeline。
- 多 Session 并发。
- Session Resume。
- 崩溃恢复。
- 外部 Codex 检测。
- Electron 低资源占用。

核心原则：

- 不依赖前端轮询。
- 使用 Main 主动推送 IPC Delta Patch。
- 使用 PTY 实时流和 Session JSONL 持久化的混合架构。

## 总体架构

```text
┌─────────────────────────────────────────────┐
│                  Renderer                   │
│                                             │
│ Session List                                │
│ Timeline                                    │
│ Tool Runtime                                │
│ Agent Tree                                  │
│ Token Stream                                │
└─────────────────────────────────────────────┘
                       ▲
                       │ IPC Delta Patch
                       ▼
┌─────────────────────────────────────────────┐
│                Electron Main                │
│                                             │
│ Runtime Registry                            │
│ Runtime State Machine                       │
│ PTY Runtime Manager                         │
│ External Session Manager                    │
│ JSON Cache                                  │
└─────────────────────────────────────────────┘
                ▲                    ▲
                │                    │
        PTY Stream           JSONL Session Watch
          实时流                 持久化恢复
                │                    │
                ▼                    ▼
         Managed Codex        External Codex
```

## Runtime 模式

系统同时支持两种 Runtime。

### Managed Runtime

应用主动通过 PTY 启动 Codex。

```text
AI Manager
↓
node-pty
↓
spawn codex
↓
实时 stdout
↓
runtime parser
```

Managed Runtime 是推荐模式，支持：

| 能力 | 支持 |
| --- | --- |
| 实时 token | 是 |
| 实时 tool | 是 |
| 实时 agent | 是 |
| interrupt | 是 |
| kill | 是 |
| pause | 是 |
| timeline | 是 |
| approval | 是 |

### External Runtime

用户在外部终端自行启动 Codex。

```bash
codex
```

系统通过 Session JSONL watcher 识别和恢复状态。

| 能力 | 支持 |
| --- | --- |
| 基础活跃检测 | 是 |
| timeline | 是 |
| session 恢复 | 是 |
| token usage | 是 |
| 真正实时 token | 否 |

## 推荐目录结构

```text
electron/
├── runtime/
│
├── pty/
│   ├── pty-manager.cjs
│   ├── codex-process.cjs
│   ├── stream-parser.cjs
│   └── terminal-buffer.cjs
│
├── session/
│   ├── watcher.cjs
│   ├── tail-reader.cjs
│   ├── jsonl-parser.cjs
│   └── session-recovery.cjs
│
├── state/
│   ├── runtime-registry.cjs
│   ├── runtime-machine.cjs
│   ├── tool-machine.cjs
│   ├── agent-machine.cjs
│   └── timeline-store.cjs
│
├── cache/
│   ├── json-cache.cjs
│   ├── memory-cache.cjs
│   └── snapshot-manager.cjs
│
├── ipc/
│   ├── runtime-ipc.cjs
│   ├── delta-emitter.cjs
│   └── event-bus.cjs
│
└── services/
    └── runtime-service.cjs
```

## PTY Runtime

Codex 是 TTY Interactive Program，真正的 runtime 信号来自 stdout，而不是 Session 文件。因此 Managed Runtime 必须使用 PTY。

推荐技术：

- `node-pty`。

### PTY Manager

```js
const pty = require("node-pty")

class PtyManager {
  createRuntime(runtimeId, cwd) {
    const proc = pty.spawn("codex", [], {
      cwd,
      cols: 120,
      rows: 40,
      env: process.env
    })

    return proc
  }
}
```

## 实时 stdout 解析

`stream-parser.cjs` 负责把 PTY stdout 转成 runtime events。

```js
proc.onData(chunk => {
  parser.feed(chunk)
})
```

Parser 输入：

```text
stdout token stream
```

Parser 输出：

```text
runtime events
```

示例：

```js
class StreamParser {
  feed(chunk) {
    if (chunk.includes("Running tool")) {
      emit("TOOL_STARTED")
    }

    if (chunk.includes("Completed tool")) {
      emit("TOOL_COMPLETED")
    }

    if (chunk.includes("Waiting for approval")) {
      emit("WAITING_APPROVAL")
    }

    emit("STREAM_DELTA")
  }
}
```

## Session Watcher

Session Watcher 只用于 External Runtime、崩溃恢复和历史回放。

监听范围：

```text
~/.codex/sessions/**/*.jsonl
```

要求：

- 不读取整个文件。
- 只读取 append 增量。
- 处理 JSON 半行。
- 文件 offset 需要持久记录或可从快照恢复。

### Tail Reader

```js
class TailReader {
  offsets = new Map()

  async readIncrement(file) {
    const offset = this.offsets.get(file) || 0
    const stat = await fs.promises.stat(file)
    const stream = fs.createReadStream(file, { start: offset })

    this.offsets.set(file, stat.size)

    return readStream(stream)
  }
}
```

### JSONL Parser

JSONL append 可能写到半行，parser 必须保存未完成内容。

```js
class JsonlParser {
  pendingLine = ""

  feed(chunk) {
    const lines = `${this.pendingLine}${chunk}`.split("\n")

    this.pendingLine = lines.pop() || ""

    return lines
      .filter(Boolean)
      .map(line => JSON.parse(line))
  }
}
```

## Runtime State Machine

Runtime 状态：

```ts
type RuntimeState =
  | "idle"
  | "streaming"
  | "running_tools"
  | "waiting_approval"
  | "background_agents"
  | "waiting_user"
  | "completed"
  | "error"
```

Runtime Session：

```ts
interface RuntimeSession {
  id: string
  cwd: string
  title: string
  model: string
  state: RuntimeState
  startedAt: number
  lastActivityAt: number
  activeTools: ToolRuntime[]
  agents: AgentRuntime[]
  tokenUsage: {
    input: number
    output: number
  }
  timeline: RuntimeEvent[]
}
```

## Tool Runtime

```ts
interface ToolRuntime {
  id: string
  name: string
  state: "running" | "completed" | "failed"
  startedAt: number
  completedAt?: number
}
```

Tool 状态流：

```text
TOOL_STARTED
↓
running
↓
TOOL_COMPLETED | TOOL_FAILED
↓
completed | failed
```

## Agent Runtime

不要依赖 `parentSessionId`，因为 Codex schema 不稳定。

Agent 使用推断模型：

```ts
interface AgentRuntime {
  inferred: true
  confidence: number
  source: "tool_call" | "fork" | "stdout"
}
```

推断来源：

- stdout 中出现 `Spawning agent`。
- Session JSONL 中出现 fork event。
- 新 Session 在 3 秒内创建，并且 cwd、上下文或 tool 调用相近。

## Timeline System

Timeline Event：

```ts
interface RuntimeEvent {
  id: string
  sessionId: string
  type:
    | "STREAM_STARTED"
    | "STREAM_DELTA"
    | "STREAM_COMPLETED"
    | "TOOL_STARTED"
    | "TOOL_COMPLETED"
    | "TOOL_FAILED"
    | "AGENT_SPAWNED"
    | "TOKEN_USAGE"
    | "APPROVAL_REQUEST"
    | "SESSION_RESUMED"
    | "RUNTIME_ERROR"
  timestamp: number
  payload: any
}
```

内存 Timeline 限制：

```js
if (timeline.length > 2000) {
  timeline.shift()
}
```

完整 Timeline 持久化到现有 storage JSON，Renderer 只拿当前视图所需片段。

## Runtime Registry

`runtime-registry.cjs` 是系统核心，统一维护所有 runtime session。

核心数据结构：

```js
Map<sessionId, RuntimeSession>
```

核心 API：

```js
registry.applyEvent(event)
registry.getSession(sessionId)
registry.getActiveSessions()
registry.removeSession(sessionId)
registry.emitPatch(sessionId, patch)
```

职责：

- 把 PTY events 和 Session events 合并到同一套状态模型。
- 维护多 Session 并发状态。
- 生成 IPC Delta Patch。
- 为 Renderer reload 提供 Snapshot。
- 把关键状态写入现有 JsonStorage。

## IPC 实时推送

禁止前端轮询 Runtime 状态。

正确流程：

```text
Runtime Event
↓
State Machine
↓
Registry Patch
↓
Main 主动推送 runtime:delta
↓
Renderer applyPatch
```

Patch 示例：

```json
{
  "sequence": 1821,
  "sessionId": "abc",
  "patch": {
    "state": "running_tools"
  }
}
```

Main：

```js
mainWindow.webContents.send("runtime:delta", patch)
```

Preload：

```js
contextBridge.exposeInMainWorld("runtime", {
  onDelta(callback) {
    ipcRenderer.on("runtime:delta", (_, delta) => {
      callback(delta)
    })
  },
  requestSnapshot() {
    return ipcRenderer.invoke("runtime:snapshot")
  }
})
```

## Renderer Store

Renderer 使用增量 patch 更新，不深度响应整个 Session 树。

```ts
const sessions = shallowReactive(new Map())

function applyPatch(delta) {
  const session = sessions.get(delta.sessionId)

  Object.assign(session, delta.patch)
}
```

要求：

- 使用 `shallowReactive`。
- Timeline 使用虚拟滚动。
- token stream 使用批量 flush。
- missed delta 时通过 snapshot 修正。

## 前端 UI

第一屏是运行态监控，不做说明页。

```text
┌──────────────────────────────────────┐
│ Session Runtime List                 │
├──────────────────────────────────────┤
│ ● streaming                          │
│ ● running tools                      │
│ ● waiting approval                   │
└──────────────────────────────────────┘

┌──────────────────────────────────────┐
│ Timeline                             │
├──────────────────────────────────────┤
│ TOOL_STARTED Read                    │
│ TOOL_COMPLETED Read                  │
│ STREAM_DELTA                         │
│ AGENT_SPAWNED                        │
└──────────────────────────────────────┘
```

推荐视图：

- Session Runtime List。
- 当前 Session Timeline。
- Tool Runtime 面板。
- Agent Tree。
- Token Stream。
- Approval 状态。

## JSON 持久化

当前项目已经有统一的 `JsonStorage`，Runtime 数据继续写入 workspace storage 下的 JSON 文件，避免引入额外数据库依赖。

推荐文件：

- `runtime-snapshots.json`：保存 Runtime session 快照。
- `runtime-offsets.json`：保存 JSONL tail offset。

### runtime-snapshots.json

```json
{
  "sequence": 1821,
  "sessions": []
}
```

### runtime-offsets.json

```json
{
  "C:\\Users\\example\\.codex\\sessions\\session.jsonl": 1024
}
```

## Consistency Sync

主方案是 IPC Push，但仍需要 30 秒一次低频一致性校验。

用途：

- Renderer reload。
- missed delta。
- Electron crash。
- hot reload。

流程：

```text
Renderer
↓
requestSnapshot()
↓
Main
↓
return minimal runtime state
```

这不是前端轮询主数据流，只是低频校验和恢复机制。

## 性能优化

### Timeline 渲染限制

Timeline 可能非常长，当前项目先不引入额外 UI 依赖，采用长度限制和局部滚动容器。

后续如果 Timeline 规模继续扩大，再按现有布局体系引入虚拟列表。

### 高频 stream batching

不要每个 token 都触发 Vue 更新。

正确做法：

```js
const pending = []

function pushToken(delta) {
  pending.push(delta)
}

function flush() {
  const batch = pending.splice(0)

  emitPatch(batch)
}
```

flush 节奏：

- Renderer 更新按 16ms batch。
- IPC patch 按 requestAnimationFrame 或 16ms 合并。
- Session JSONL 按 append 增量解析。

### Electron 低资源占用

- Watcher 只监听 Codex session 根目录。
- Tail reader 只读新增字节。
- Main 内存只保留活跃 Session 和有限 Timeline。
- 历史 Timeline 进 Runtime 快照 JSON，并限制单 Session 内存长度。
- Renderer 不持有全量历史。

## 错误恢复

### JSON 半行

append 写入可能出现半行，必须等到 `\n` 再 parse。

```text
this.pendingLine += chunk
```

### 崩溃恢复

启动流程：

```text
load runtime-snapshots.json
↓
恢复 active sessions
↓
读取 session tail offset
↓
补齐 crash 后新增 JSONL
↓
emit snapshot
```

### Session Resume

Resume 需要基于：

- session id。
- cwd。
- JSONL 文件路径。
- lastActivityAt。
- JSON snapshot sequence。

恢复后继续接收 PTY 或 JSONL 增量。

## Managed Runtime 推荐入口

最佳使用方式是用户通过 AI Manager 启动 Codex，而不是直接在终端启动。

```bash
aim codex
```

最终状态流：

```text
spawn codex
↓
PTY stdout
↓
stream parser
↓
runtime event
↓
state machine
↓
runtime registry
↓
ipc patch
↓
renderer apply patch
↓
ui update
```

## External Runtime 状态流

```text
jsonl append
↓
tail reader
↓
incremental parser
↓
runtime event
↓
state machine
↓
registry
↓
ipc patch
↓
ui
```

## 最终技术栈

| 模块 | 技术 |
| --- | --- |
| Electron | Electron 35+ |
| PTY | node-pty |
| Cache | JsonStorage |
| Watcher | chokidar |
| Frontend | Vue 3 |
| Store | shallowReactive 或 Pinia |
| Timeline | 有限列表 + 局部滚动 |

## 最终能力

| 能力 | 支持 |
| --- | --- |
| 真正实时状态 | 是 |
| token streaming | 是 |
| tool lifecycle | 是 |
| agent runtime | 是 |
| approval detect | 是 |
| session resume | 是 |
| external codex detect | 是 |
| 崩溃恢复 | 是 |
| timeline | 是 |
| JSON snapshot | 是 |
| delta push | 是 |
| Electron 低占用 | 是 |
| 多 session | 是 |

## 结论

生产级 Codex Runtime Observability System 应采用：

```text
PTY Runtime + Session Persistence Hybrid
```

其中：

- PTY 负责真正实时 runtime。
- Session JSONL 负责持久化恢复、历史回放和外部 Codex fallback。
- JsonStorage 负责快照、tail offset 和启动恢复。
- IPC Delta Patch 负责无轮询的实时 UI 更新。
