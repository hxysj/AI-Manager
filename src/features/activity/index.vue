<template>
  <section class="activity-view">
    <header class="activity-view__topbar">
      <div class="activity-view__title">
        <span>Codex Runtime</span>
        <h1>运行监控</h1>
      </div>
      <div class="activity-view__actions">
        <input v-model.trim="cwdDraft" type="text" placeholder="工作目录" />
        <button type="button" @click="startCodex">
          <Play :size="15" />
          启动 Codex
        </button>
        <button type="button" @click="$emit('refresh')">
          <RefreshCw :size="15" />
          同步
        </button>
      </div>
    </header>

    <div class="activity-view__metrics">
      <article>
        <span>Runtime</span>
        <strong>{{ sessions.length }}</strong>
      </article>
      <article>
        <span>活跃</span>
        <strong>{{ activeSessions.length }}</strong>
      </article>
      <article>
        <span>运行中 Tool</span>
        <strong>{{ runningTools.length }}</strong>
      </article>
      <article>
        <span>事件序列</span>
        <strong>{{ runtime.sequence || 0 }}</strong>
      </article>
    </div>

    <div class="activity-view__workspace">
      <section class="activity-view__sessions">
        <header class="activity-view__panel-head">
          <div>
            <span>Sessions</span>
            <h2>Runtime List</h2>
          </div>
        </header>

        <div v-if="sessions.length" class="activity-view__session-list">
          <button
            v-for="session in sessions"
            :key="session.id"
            :class="[
              'activity-view__session',
              { 'activity-view__session--active': session.id === selectedSessionId }
            ]"
            type="button"
            @click="selectedSessionId = session.id"
          >
            <span
              :class="[
                'activity-view__status-dot',
                `activity-view__status-dot--${session.state}`
              ]"
            ></span>
            <span class="activity-view__session-copy">
              <strong>{{ shortTitle(session.title || session.id) }}</strong>
              <small>{{ session.cwd || '未识别工作目录' }}</small>
            </span>
            <span class="activity-view__state-pill">
              {{ stateLabel(session.state) }}
            </span>
          </button>
        </div>

        <div v-else class="activity-view__empty">暂无 Runtime Session</div>
      </section>

      <main class="activity-view__detail">
        <header class="activity-view__session-head">
          <div class="activity-view__session-title">
            <span>{{ modeLabel(selectedSession?.mode) }}</span>
            <h2>{{ selectedSession?.title || '等待 Runtime' }}</h2>
            <p>{{ selectedSession?.cwd || '等待识别工作目录' }}</p>
          </div>
          <button
            v-if="selectedSession?.mode === 'managed'"
            type="button"
            @click="$emit('stop-codex', selectedSession.id)"
          >
            <Square :size="15" />
            停止
          </button>
        </header>

        <div class="activity-view__session-metrics">
          <article>
            <span>状态</span>
            <strong>{{ stateLabel(selectedSession?.state) }}</strong>
          </article>
          <article>
            <span>Agent</span>
            <strong>{{ selectedSession?.agents?.length || 0 }}</strong>
          </article>
          <article>
            <span>Token</span>
            <strong>{{ tokenText }}</strong>
          </article>
          <article>
            <span>最后活动</span>
            <strong>{{ formatTime(selectedSession?.lastActivityAt) }}</strong>
          </article>
        </div>

        <div class="activity-view__runtime">
          <section class="activity-view__chat">
            <header class="activity-view__panel-head">
              <div>
                <span>Live Stream</span>
                <h2>实时会话流</h2>
              </div>
              <strong>{{ chatMessages.length }} 条</strong>
            </header>

            <div ref="chatStreamRef" class="activity-view__chat-list">
              <article
                v-for="message in visibleChatMessages"
                :key="message.id"
                :class="[
                  'activity-view__chat-message',
                  `activity-view__chat-message--${message.role}`
                ]"
              >
                <header>
                  <strong>{{ roleLabel(message.role) }}</strong>
                  <span>{{ message.source }} · {{ formatTime(message.updatedAt) }}</span>
                </header>
                <pre>{{ message.text }}</pre>
              </article>

              <div v-if="!visibleChatMessages.length" class="activity-view__empty">
                暂无实时会话内容
              </div>
            </div>

            <div
              v-if="selectedSession?.mode === 'managed'"
              class="activity-view__input-row"
            >
              <input
                v-model="inputDraft"
                type="text"
                placeholder="发送到 Codex PTY"
                @keydown.enter="sendInput"
              />
              <button type="button" @click="sendInput">
                <Send :size="15" />
              </button>
            </div>
          </section>

          <aside class="activity-view__side">
            <section class="activity-view__side-section">
              <header class="activity-view__panel-head">
                <div>
                  <span>Timeline</span>
                  <h2>事件流</h2>
                </div>
              </header>
              <div v-if="timeline.length" class="activity-view__timeline-list">
                <article v-for="item in timeline" :key="item.id">
                  <span>{{ eventLabel(item.type) }}</span>
                  <p>{{ eventText(item) }}</p>
                  <small>{{ formatTime(item.timestamp) }}</small>
                </article>
              </div>
              <div v-else class="activity-view__empty">暂无 Timeline 事件</div>
            </section>

            <section class="activity-view__side-section">
              <header class="activity-view__panel-head">
                <div>
                  <span>Tools</span>
                  <h2>Tool Runtime</h2>
                </div>
                <strong>{{ selectedTools.length }}</strong>
              </header>
              <div v-if="selectedTools.length" class="activity-view__tool-list">
                <article
                  v-for="tool in selectedTools"
                  :key="tool.id"
                  :class="[
                    'activity-view__tool',
                    `activity-view__tool--${tool.state}`
                  ]"
                >
                  <strong>{{ tool.name }}</strong>
                  <span>{{ tool.state }}</span>
                </article>
              </div>
              <div v-else class="activity-view__empty">暂无 Tool</div>
            </section>

            <section class="activity-view__side-section">
              <header class="activity-view__panel-head">
                <div>
                  <span>Agents</span>
                  <h2>Agent Tree</h2>
                </div>
              </header>
              <div
                v-if="selectedSession?.agents?.length"
                class="activity-view__tool-list"
              >
                <article
                  v-for="agent in selectedSession.agents"
                  :key="agent.id"
                  class="activity-view__tool"
                >
                  <strong>{{ agent.title }}</strong>
                  <span>{{ agent.source }}</span>
                </article>
              </div>
              <div v-else class="activity-view__empty">暂无 Agent</div>
            </section>
          </aside>
        </div>
      </main>
    </div>
  </section>
</template>

<script setup>
import { computed, nextTick, ref, watch } from "vue"
import { Play, RefreshCw, Send, Square } from "lucide-vue-next"

const props = defineProps({
  runtime: {
    type: Object,
    required: true
  },
  paths: {
    type: Object,
    required: true
  }
})

const emit = defineEmits(["refresh", "send-input", "start-codex", "stop-codex"])

const selectedSessionId = ref("")
const cwdDraft = ref("")
const inputDraft = ref("")
const chatStreamRef = ref(null)

const sessions = computed(() => props.runtime.sessions || [])
const activeSessions = computed(() => {
  const now = Date.now()

  return sessions.value.filter(session => {
    return now - Number(session.lastActivityAt || 0) < 5 * 60 * 1000
  })
})
const selectedSession = computed(() => {
  return (
    sessions.value.find(item => item.id === selectedSessionId.value) ||
    sessions.value[0] ||
    null
  )
})
const timeline = computed(() => {
  return [...(selectedSession.value?.timeline || [])].reverse().slice(0, 120)
})
const chatMessages = computed(() => selectedSession.value?.chatMessages || [])
const visibleChatMessages = computed(() => chatMessages.value.slice(-120))
const chatScrollKey = computed(() => {
  const lastMessage = chatMessages.value[chatMessages.value.length - 1]

  return [
    selectedSession.value?.id || "",
    chatMessages.value.length,
    lastMessage?.updatedAt || "",
    lastMessage?.text?.length || 0
  ].join(":")
})
const tokenText = computed(() => {
  const usage = selectedSession.value?.tokenUsage || {}

  return `${Number(usage.input || 0)} / ${Number(usage.output || 0)}`
})
const selectedTools = computed(() => selectedSession.value?.activeTools || [])
const runningTools = computed(() =>
  selectedTools.value.filter(tool => tool.state === "running")
)

watch(
  sessions,
  value => {
    if (!selectedSessionId.value && value.length) {
      selectedSessionId.value = value[0].id
    }
  },
  { immediate: true }
)

watch(
  () => props.paths.workspaceRoot,
  value => {
    if (!cwdDraft.value) {
      cwdDraft.value = value || ""
    }
  },
  { immediate: true }
)

watch(
  chatScrollKey,
  async () => {
    await nextTick()

    if (chatStreamRef.value) {
      chatStreamRef.value.scrollTop = chatStreamRef.value.scrollHeight
    }
  }
)

function startCodex() {
  emit("start-codex", {
    cwd: cwdDraft.value || props.paths.workspaceRoot,
    title: "Managed Codex"
  })
}

function sendInput() {
  if (!selectedSession.value || !inputDraft.value) {
    return
  }

  emit("send-input", {
    sessionId: selectedSession.value.id,
    data: `${inputDraft.value}\r`
  })
  inputDraft.value = ""
}

function shortTitle(value) {
  return String(value || "").replace(/^rollout-/, "")
}

function modeLabel(value) {
  return value === "managed" ? "Managed Runtime" : "External Session"
}

function stateLabel(value) {
  const labels = {
    idle: "空闲",
    streaming: "输出中",
    running_tools: "工具运行中",
    waiting_approval: "等待批准",
    background_agents: "Agent 运行中",
    waiting_user: "等待用户",
    completed: "已完成",
    error: "异常"
  }

  return labels[value] || "未知"
}

function roleLabel(value) {
  const labels = {
    assistant: "Assistant",
    user: "User",
    system: "System"
  }

  return labels[value] || "Runtime"
}

function eventLabel(value) {
  const labels = {
    STREAM_STARTED: "开始输出",
    STREAM_DELTA: "流式输出",
    STREAM_COMPLETED: "输出完成",
    TOOL_STARTED: "工具开始",
    TOOL_COMPLETED: "工具完成",
    TOOL_FAILED: "工具失败",
    AGENT_SPAWNED: "Agent 启动",
    TOKEN_USAGE: "Token",
    APPROVAL_REQUEST: "批准请求",
    SESSION_RESUMED: "Session 恢复",
    RUNTIME_ERROR: "异常"
  }

  return labels[value] || value
}

function eventText(item) {
  const payload = item.payload || {}

  return (
    payload.name ||
    payload.title ||
    payload.message ||
    payload.text ||
    payload.source ||
    ""
  )
}

function formatTime(value) {
  if (!value) {
    return "-"
  }

  return new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(new Date(value))
}
</script>

<style scoped lang="less">
.activity-view {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 12px;
  color: var(--color-text);

  &__topbar,
  &__sessions,
  &__detail,
  &__chat,
  &__side-section {
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
  }

  &__topbar {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 16px;
  }

  &__title span,
  &__panel-head span,
  &__metrics span,
  &__session-metrics span {
    color: var(--color-text-soft);
    font-size: 0.68rem;
    font-weight: 800;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  &__title h1,
  &__panel-head h2,
  &__session-title h2 {
    margin: 4px 0 0;
    color: var(--color-text);
    font-size: 1.05rem;
    line-height: 1.3;
  }

  &__title h1 {
    color: var(--color-primary);
    font-size: 1.35rem;
  }

  &__actions,
  &__input-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &__actions input,
  &__input-row input {
    width: 260px;
    height: 32px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-text);
    padding: 0 10px;
  }

  &__actions button,
  &__session-head button,
  &__input-row button {
    display: inline-flex;
    height: 32px;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #fbfcfd;
    color: var(--color-primary);
    cursor: pointer;
    font-weight: 800;
  }

  &__metrics,
  &__session-metrics {
    display: grid;
    flex: none;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
  }

  &__metrics article,
  &__session-metrics article {
    min-width: 0;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    padding: 11px 12px;
  }

  &__metrics strong,
  &__session-metrics strong {
    display: block;
    overflow: hidden;
    margin-top: 5px;
    color: var(--color-primary);
    font-size: 1.18rem;
    line-height: 1.25;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__workspace {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 12px;
  }

  &__sessions,
  &__detail,
  &__chat,
  &__side-section {
    display: flex;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
  }

  &__panel-head,
  &__session-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px;
    border-bottom: 1px solid var(--color-line);
  }

  &__sessions &__panel-head {
    padding: 9px 12px;
  }

  &__panel-head strong {
    color: var(--color-text-muted);
    font-size: 0.78rem;
  }

  &__session-list,
  &__chat-list,
  &__timeline-list,
  &__tool-list {
    min-height: 0;
    overflow: auto;
  }

  &__session-list {
    display: flex;
    flex: none;
    overflow-x: auto;
    overflow-y: hidden;
  }

  &__session {
    display: grid;
    width: 320px;
    min-height: 62px;
    flex: 0 0 auto;
    grid-template-columns: 8px minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    border: 0;
    border-right: 1px solid var(--color-line);
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    padding: 10px 12px;
    text-align: left;
  }

  &__session:hover,
  &__session--active {
    background: #f3f8ff;
  }

  &__status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #94a3b8;
  }

  &__status-dot--streaming,
  &__status-dot--running_tools,
  &__status-dot--background_agents {
    background: #16a34a;
  }

  &__status-dot--waiting_approval,
  &__status-dot--waiting_user {
    background: #d97706;
  }

  &__status-dot--error {
    background: #dc2626;
  }

  &__session-copy {
    min-width: 0;
  }

  &__session-copy strong,
  &__session-copy small {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__session-copy small {
    margin-top: 4px;
    color: var(--color-text-muted);
    font-size: 0.78rem;
  }

  &__state-pill {
    height: 24px;
    padding: 0 8px;
    border-radius: 999px;
    background: #edf4fb;
    color: var(--color-primary);
    font-size: 0.72rem;
    font-weight: 800;
    line-height: 24px;
    white-space: nowrap;
  }

  &__detail {
    flex: 1;
    gap: 12px;
    padding-bottom: 12px;
  }

  &__session-title {
    min-width: 0;
  }

  &__session-title h2,
  &__session-title p {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__session-title p {
    margin: 4px 0 0;
    color: var(--color-text-muted);
    font-size: 0.8rem;
  }

  &__session-metrics,
  &__runtime {
    margin: 0 12px;
  }

  &__runtime {
    display: grid;
    min-height: 0;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) 310px;
    gap: 12px;
  }

  &__chat-list {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    background: #f8fafc;
  }

  &__chat-message {
    max-width: 88%;
    border: 1px solid #dce6f0;
    border-radius: 8px;
    background: #ffffff;
    padding: 10px 12px;
    box-shadow: 0 8px 18px rgba(15, 23, 42, 0.04);
  }

  &__chat-message--assistant {
    border-color: #cbdff2;
  }

  &__chat-message--user {
    align-self: flex-end;
    border-color: #c9e6d2;
    background: #f3fbf5;
  }

  &__chat-message header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 7px;
  }

  &__chat-message header strong {
    color: var(--color-primary);
    font-size: 0.78rem;
  }

  &__chat-message header span {
    color: var(--color-text-soft);
    font-size: 0.72rem;
    white-space: nowrap;
  }

  &__chat-message pre {
    overflow: hidden;
    margin: 0;
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 0.78rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }

  &__input-row {
    flex: none;
    padding: 10px 12px;
    border-top: 1px solid var(--color-line);
    background: #ffffff;
  }

  &__input-row input {
    flex: 1;
    width: auto;
  }

  &__side {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 10px;
  }

  &__side-section {
    flex: 1;
  }

  &__timeline-list article {
    display: grid;
    grid-template-columns: 86px minmax(0, 1fr) 62px;
    gap: 8px;
    border-bottom: 1px solid var(--color-line);
    padding: 9px 10px;
  }

  &__timeline-list span {
    color: var(--color-primary);
    font-size: 0.76rem;
    font-weight: 800;
  }

  &__timeline-list p {
    overflow: hidden;
    margin: 0;
    color: var(--color-text);
    font-size: 0.76rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__timeline-list small {
    color: var(--color-text-muted);
    font-size: 0.72rem;
    text-align: right;
  }

  &__tool-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
  }

  &__tool {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #f8fafc;
    padding: 8px 10px;
  }

  &__tool strong,
  &__tool span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__tool strong {
    color: var(--color-text);
    font-size: 0.8rem;
  }

  &__tool span {
    color: var(--color-text-muted);
    font-size: 0.76rem;
    font-weight: 800;
  }

  &__tool--running {
    border-color: #b8dfc2;
    background: #f3fbf5;
  }

  &__tool--failed {
    border-color: #f1c1c1;
    background: #fff7f7;
  }

  &__empty {
    display: grid;
    min-height: 96px;
    flex: 1;
    place-items: center;
    color: var(--color-text-muted);
    font-size: 0.86rem;
    font-weight: 700;
  }
}
</style>
