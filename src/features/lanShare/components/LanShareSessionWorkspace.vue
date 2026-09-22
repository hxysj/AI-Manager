<template>
  <section class="chat-workspace">
    <header class="workspace-header">
      <div class="workspace-identity">
        <div
          class="workspace-avatar"
          :class="{ 'workspace-avatar-online': currentDevice?.online }"
        >
          <div v-if="currentDevice?.online" class="cyber-avatar-ring"></div>
          <Users
            v-if="chatMode === 'group'"
            :size="18"
            class="cyber-avatar-icon"
          />
          <MonitorSmartphone v-else :size="18" class="cyber-avatar-icon" />
        </div>
        <div class="workspace-heading">
          <div class="workspace-title-row">
            <span class="workspace-name">{{ title }}</span>
            <span
              v-if="currentDevice"
              class="cyber-node-badge"
              :class="{ 'cyber-node-online': currentDevice.online }"
            >
              {{
                currentDevice.online
                  ? "ONLINE // 节点在线"
                  : currentDevice.native
                    ? "PAIRED // 已配对"
                    : "OFFLINE // 离线"
              }}
            </span>
          </div>
          <span class="workspace-subtitle">{{ subtitle }}</span>
        </div>
      </div>

      <div class="workspace-actions">
        <button
          class="workspace-icon"
          type="button"
          title="搜索聊天记录"
          aria-label="搜索聊天记录"
          :disabled="!currentSessionId"
          @click="messagesRef?.openSearch()"
        >
          <Search :size="15" />
        </button>
        <button
          class="workspace-icon"
          type="button"
          title="查看会话文件"
          aria-label="查看会话文件"
          :disabled="!currentSessionId"
          @click="filesOpen = true"
        >
          <FolderOpen :size="15" />
        </button>
        <button
          v-if="chatMode === 'direct'"
          class="workspace-icon"
          type="button"
          title="历史会话"
          aria-label="历史会话"
          :disabled="!currentDevice"
          @click="historyOpen = true"
        >
          <History :size="15" />
        </button>
        <button
          v-if="chatMode === 'group'"
          class="workspace-icon"
          type="button"
          title="群聊设置"
          aria-label="群聊设置"
          :disabled="!currentGroup"
          @click="groupOpen = true"
        >
          <Settings2 :size="15" />
        </button>
        <el-dropdown trigger="click" @command="handleCommand">
          <button
            class="workspace-icon"
            type="button"
            aria-label="更多会话操作"
            :disabled="
              chatMode === 'direct' ? !currentDevice : !currentSessionId
            "
          >
            <MoreHorizontal :size="17" />
          </button>
          <template #dropdown>
            <el-dropdown-menu class="cyber-dropdown-menu">
              <el-dropdown-item v-if="chatMode === 'direct'" command="new">
                新建独立会话
              </el-dropdown-item>
              <el-dropdown-item command="clear">清空本机会话</el-dropdown-item>
              <el-dropdown-item v-if="chatMode === 'direct'" command="delete">
                清除设备记录
              </el-dropdown-item>
              <el-dropdown-item
                v-if="chatMode === 'direct'"
                command="delete-device"
                divided
              >
                解除配对并删除
              </el-dropdown-item>
            </el-dropdown-menu>
          </template>
        </el-dropdown>
      </div>
    </header>

    <LanShareMessagesPanel
      ref="messagesRef"
      :chat-mode="chatMode"
      :current-device="currentDevice"
      :current-session-id="currentSessionId"
      :current-session="currentSession"
      :service="service"
      :state-version="stateVersion"
      @refresh-state="$emit('refresh-state')"
      @preview-file="$emit('preview-file', $event)"
      @download-file="$emit('download-file', $event)"
    />

    <!-- 历史会话抽屉 -->
    <el-drawer
      v-model="historyOpen"
      title="历史会话列表"
      size="420px"
      append-to-body
      destroy-on-close
      class="cyber-drawer"
    >
      <div class="workspace-history">
        <div class="workspace-history-head">
          <span>历史会话存档，主界面仅显示当前活跃会话。</span>
          <el-button size="small" type="primary" plain @click="createSession">
            + 新建会话
          </el-button>
        </div>
        <div class="workspace-history-list">
          <div
            v-for="session in sortedSessions"
            :key="session.id"
            class="workspace-history-row"
            :class="{
              'workspace-history-selected': session.id === selectedSessionId
            }"
          >
            <button
              class="workspace-history-select"
              type="button"
              @click="selectSession(session.id)"
            >
              <MessagesSquare :size="16" class="history-icon" />
              <span class="workspace-history-copy">
                <span class="history-session-id"
                  >会话 {{ session.id.slice(-8) }}</span
                >
                <span class="workspace-history-time">
                  {{ formatDateTime(session.updatedAt) }}
                </span>
              </span>
            </button>
            <button
              class="workspace-history-delete"
              type="button"
              title="删除会话"
              aria-label="删除会话"
              @click="deleteSession(session.id)"
            >
              <Trash2 :size="14" />
            </button>
          </div>
        </div>
      </div>
    </el-drawer>

    <!-- 会话文件抽屉 -->
    <el-drawer
      v-model="filesOpen"
      title="会话传输文件"
      size="560px"
      append-to-body
      destroy-on-close
      class="cyber-drawer"
    >
      <LanShareFilesPanel
        class="workspace-files"
        :current-session-id="currentSessionId"
        :can-manage-files="Boolean(currentSessionId)"
        :service-running="service.running"
        :state-version="stateVersion"
        @refresh-state="$emit('refresh-state')"
        @preview-file="$emit('preview-file', $event)"
        @download-file="$emit('download-file', $event)"
      />
    </el-drawer>

    <!-- 群聊设置抽屉 -->
    <el-drawer
      v-model="groupOpen"
      title="群聊信道设置"
      size="430px"
      append-to-body
      destroy-on-close
      class="cyber-drawer"
    >
      <div v-if="currentGroup" class="workspace-group">
        <label class="workspace-group-field">
          <span>信道名称</span>
          <el-input v-model="groupDraft.name" />
        </label>
        <label class="workspace-group-field">
          <span>新成员消息可见范围</span>
          <el-select v-model="groupDraft.messageVisibility">
            <el-option label="全部历史" value="all" />
            <el-option label="加入后消息" value="afterJoin" />
            <el-option label="加入前最近 10 条" value="recent10" />
          </el-select>
        </label>
        <el-button
          type="primary"
          @click="
            $emit('update-group', { groupId: currentGroup.id, ...groupDraft })
          "
        >
          保存群设置
        </el-button>

        <div class="workspace-group-invite">
          <span>邀请访客加入群聊</span>
          <div
            v-if="currentGroup.qrSvg"
            class="workspace-group-qr"
            v-html="currentGroup.qrSvg"
          ></div>
          <small class="invite-code-text"
            >邀请码 {{ currentGroup.inviteCode }}</small
          >
          <el-button
            size="small"
            @click="
              $emit(
                'copy-text',
                currentGroup.inviteUrl || currentGroup.inviteCode
              )
            "
          >
            复制邀请链接
          </el-button>
        </div>

        <div class="workspace-group-members">
          <span>群组成员 · {{ currentGroup.members?.length || 0 }}</span>
          <div
            v-for="member in currentGroup.members || []"
            :key="member.deviceId"
            class="workspace-group-member"
          >
            <span class="workspace-member-copy">
              {{ member.deviceName || member.deviceId }}
              <span
                class="workspace-member-status"
                :class="{ 'status-online': member.online }"
              >
                {{ member.online ? "在线" : "离线" }}
              </span>
            </span>
            <el-button
              size="small"
              text
              type="danger"
              @click="
                $emit('remove-group-member', {
                  groupId: currentGroup.id,
                  deviceId: member.deviceId
                })
              "
            >
              移出
            </el-button>
          </div>
        </div>
        <div class="workspace-group-footer-actions">
          <el-button
            type="danger"
            plain
            @click="$emit('clear-group-messages', currentGroup.id)"
          >
            清空群消息
          </el-button>
          <el-button type="danger" text @click="deleteGroup"
            >解散群聊</el-button
          >
        </div>
      </div>
    </el-drawer>
  </section>
</template>

<script setup>
import { computed, reactive, ref, watch } from "vue"
import {
  ElButton,
  ElDrawer,
  ElDropdown,
  ElDropdownItem,
  ElDropdownMenu,
  ElInput,
  ElOption,
  ElSelect
} from "element-plus"
import "element-plus/es/components/button/style/css"
import "element-plus/es/components/drawer/style/css"
import "element-plus/es/components/dropdown/style/css"
import "element-plus/es/components/input/style/css"
import "element-plus/es/components/select/style/css"
import {
  FolderOpen,
  History,
  MessagesSquare,
  MonitorSmartphone,
  MoreHorizontal,
  Search,
  Settings2,
  Trash2,
  Users
} from "lucide-vue-next"
import { formatDateTime } from "@/utils/formatters"
import LanShareMessagesPanel from "./LanShareMessagesPanel.vue"
import LanShareFilesPanel from "./LanShareFilesPanel.vue"

const props = defineProps({
  chatMode: { type: String, default: "direct" },
  sessions: { type: Array, default: () => [] },
  currentGroup: { type: Object, default: null },
  currentDevice: { type: Object, default: null },
  selectedSessionId: { type: String, default: "" },
  currentSession: { type: Object, default: null },
  currentSessionId: { type: String, default: "" },
  service: { type: Object, default: () => ({}) },
  stateVersion: { type: Number, default: 0 }
})

const emit = defineEmits([
  "select-session",
  "delete-session",
  "create-session",
  "update-group",
  "remove-group-member",
  "clear-group-messages",
  "delete-group",
  "delete-history",
  "delete-device",
  "refresh-state",
  "preview-file",
  "download-file",
  "copy-text"
])

const messagesRef = ref(null)
const filesOpen = ref(false)
const historyOpen = ref(false)
const groupOpen = ref(false)
const groupDraft = reactive({ name: "", messageVisibility: "all" })

const sortedSessions = computed(() =>
  [...props.sessions].sort((left, right) => right.updatedAt - left.updatedAt)
)

const title = computed(() =>
  props.chatMode === "group"
    ? props.currentGroup?.name || "选择一个群聊"
    : props.currentDevice?.name || props.currentDevice?.autoName || "未连接设备"
)

const subtitle = computed(() =>
  props.chatMode === "group"
    ? `${props.currentGroup?.members?.length || 0} 位成员 · 文件与消息集中传输信道`
    : props.currentDevice
      ? `IP: ${props.currentDevice.ip || "局域网未知"} · 传输模式: ${props.currentDevice.native ? "P2P 客户端" : "HTTP 访客端"}`
      : "选择左侧设备节点开启安全直连"
)

watch(
  () => props.currentGroup,
  (group) => {
    groupDraft.name = group?.name || ""
    groupDraft.messageVisibility = group?.messageVisibility || "all"
  },
  { immediate: true }
)

watch(
  () => props.currentSessionId,
  () => {
    filesOpen.value = false
    historyOpen.value = false
  }
)

function handleCommand(command) {
  if (command === "new") emit("create-session")
  if (command === "clear") messagesRef.value?.clearCurrentSession()
  if (command === "delete") emit("delete-history")
  if (command === "delete-device") emit("delete-device")
}

function deleteSession(id) {
  if (window.confirm("删除这个会话的本机历史记录？")) emit("delete-session", id)
}

function createSession() {
  emit("create-session")
  historyOpen.value = false
}

function selectSession(sessionId) {
  emit("select-session", sessionId)
  historyOpen.value = false
}

function deleteGroup() {
  emit("delete-group", props.currentGroup.id)
  groupOpen.value = false
}
</script>

<style scoped lang="less">
.chat-workspace {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  background: var(--color-panel);

  .workspace-header {
    display: flex;
    min-width: 0;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);

    .workspace-identity {
      display: flex;
      min-width: 0;
      align-items: center;
      gap: 12px;

      .workspace-avatar {
        position: relative;
        display: flex;
        width: 38px;
        height: 38px;
        flex: none;
        align-items: center;
        justify-content: center;
        border-radius: 8px;
        background: var(--color-primary-soft);
        border: 1px solid var(--color-info-line);
        color: var(--color-primary);

        .cyber-avatar-icon {
          z-index: 1;
        }

        .cyber-avatar-ring {
          position: absolute;
          inset: -3px;
          border-radius: 11px;
          border: 1px dashed var(--color-primary);
          opacity: 0.55;
          animation: cyberSpin 12s linear infinite;
        }

        &.workspace-avatar-online {
          box-shadow: 0 0 10px var(--color-success-soft);
          color: var(--color-success);
          border-color: var(--color-success-line);
          background: var(--color-success-soft);

          .cyber-avatar-ring {
            border-color: var(--color-success);
            opacity: 0.6;
          }
        }
      }

      .workspace-heading {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 4px;

        .workspace-title-row {
          display: flex;
          align-items: center;
          gap: 8px;

          .workspace-name {
            overflow: hidden;
            color: var(--color-text);
            text-overflow: ellipsis;
            white-space: nowrap;
            font-size: 15px;
            font-weight: 600;
            letter-spacing: 0.3px;
          }

          .cyber-node-badge {
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 10px;
            padding: 1px 6px;
            border-radius: 3px;
            background: var(--color-panel);
            border: 1px solid var(--color-line);
            color: var(--color-text-muted);

            &.cyber-node-online {
              background: var(--color-success-soft);
              border-color: var(--color-success-line);
              color: var(--color-success);
            }
          }
        }

        .workspace-subtitle {
          overflow: hidden;
          color: var(--color-text-muted);
          text-overflow: ellipsis;
          white-space: nowrap;
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 11px;
        }
      }
    }

    .workspace-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 6px;

      .workspace-icon {
        display: grid;
        width: 30px;
        height: 30px;
        place-items: center;
        padding: 0;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        color: var(--color-text-muted);
        background: var(--color-panel);
        cursor: pointer;
        transition: all 0.2s;

        &:hover:not(:disabled) {
          color: var(--color-primary);
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
        }

        &:disabled {
          opacity: 0.35;
          cursor: not-allowed;
        }
      }
    }
  }
}

.workspace-files {
  height: 100%;
  min-height: 0;
}

.workspace-history {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  gap: 16px;
  color: var(--color-text);

  .workspace-history-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    font-size: 12px;
    color: var(--color-text-muted);
  }

  .workspace-history-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;

    .workspace-history-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 10px 12px;
      background: var(--color-panel-soft);
      border: 1px solid var(--color-line);
      border-radius: 6px;
      transition: all 0.2s;

      &:hover {
        background: var(--color-primary-soft);
        border-color: var(--color-info-line);
      }

      &.workspace-history-selected {
        background: var(--color-primary-soft);
        border-color: var(--color-primary);
      }

      .workspace-history-select {
        display: flex;
        align-items: center;
        gap: 10px;
        flex: 1;
        min-width: 0;
        border: 0;
        background: transparent;
        color: inherit;
        cursor: pointer;
        text-align: left;

        .history-icon {
          color: var(--color-primary);
          flex-shrink: 0;
        }

        .workspace-history-copy {
          display: flex;
          flex-direction: column;
          gap: 2px;
          min-width: 0;

          .history-session-id {
            font-size: 13px;
            font-weight: 500;
          }

          .workspace-history-time {
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 11px;
            color: var(--color-text-soft);
          }
        }
      }

      .workspace-history-delete {
        display: grid;
        width: 26px;
        height: 26px;
        place-items: center;
        border: 0;
        background: transparent;
        color: var(--color-text-soft);
        cursor: pointer;
        border-radius: 4px;

        &:hover {
          color: var(--color-danger);
          background: var(--color-danger-soft);
        }
      }
    }
  }
}

.workspace-group {
  display: flex;
  flex-direction: column;
  gap: 16px;
  color: var(--color-text);

  .workspace-group-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12.5px;
    color: var(--color-text-muted);
  }

  .workspace-group-invite {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 16px;
    border: 1px dashed var(--color-primary);
    border-radius: 8px;
    background: var(--color-primary-soft);
    font-size: 12px;

    .workspace-group-qr {
      width: 140px;
      height: 140px;
      background: #ffffff;
      padding: 8px;
      border-radius: 8px;

      :deep(svg) {
        width: 100%;
        height: 100%;
      }
    }

    .invite-code-text {
      font-family:
        ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      color: var(--color-primary);
      letter-spacing: 1px;
      font-size: 12px;
      font-weight: 600;
    }
  }

  .workspace-group-members {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12.5px;
    color: var(--color-text-muted);

    .workspace-group-member {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 8px 10px;
      background: var(--color-panel-soft);
      border: 1px solid var(--color-line);
      border-radius: 6px;

      .workspace-member-copy {
        display: flex;
        align-items: center;
        gap: 8px;
        color: var(--color-text);

        .workspace-member-status {
          font-size: 10.5px;
          color: var(--color-text-soft);

          &.status-online {
            color: var(--color-success);
          }
        }
      }
    }
  }

  .workspace-group-footer-actions {
    display: flex;
    justify-content: space-between;
    margin-top: 10px;
  }
}

@keyframes cyberSpin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
