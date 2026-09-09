<template>
  <section class="chat-workspace">
    <header class="workspace-header">
      <div class="workspace-identity">
        <span class="workspace-avatar"
          ><Users v-if="chatMode === 'group'" :size="20" /><MonitorSmartphone
            v-else
            :size="20"
        /></span>
        <div class="workspace-heading">
          <span class="workspace-name">{{ title }}</span
          ><span class="workspace-subtitle">{{ subtitle }}</span>
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
          <Search :size="17" />
        </button>
        <button
          class="workspace-icon"
          type="button"
          title="查看会话文件"
          aria-label="查看会话文件"
          :disabled="!currentSessionId"
          @click="filesOpen = true"
        >
          <FolderOpen :size="17" />
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
          <History :size="17" />
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
          <Settings2 :size="17" />
        </button>
        <el-dropdown trigger="click" @command="handleCommand">
          <button
            class="workspace-icon"
            type="button"
            aria-label="更多会话操作"
            :disabled="chatMode === 'direct' ? !currentDevice : !currentSessionId"
          >
            <MoreHorizontal :size="19" />
          </button>
          <template #dropdown
            ><el-dropdown-menu
              ><el-dropdown-item v-if="chatMode === 'direct'" command="new"
                >新建会话</el-dropdown-item
              ><el-dropdown-item command="clear">清空本机会话</el-dropdown-item
              ><el-dropdown-item v-if="chatMode === 'direct'" command="delete"
                >删除设备历史</el-dropdown-item
              ><el-dropdown-item v-if="chatMode === 'direct'" command="delete-device" divided
                >删除设备</el-dropdown-item
              ></el-dropdown-menu
            ></template
          >
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
    />

    <el-drawer
      v-model="historyOpen"
      title="历史会话"
      size="420px"
      append-to-body
      destroy-on-close
    >
      <div class="workspace-history">
        <div class="workspace-history-head">
          <span>保留旧记录，主界面只显示当前聊天。</span
          ><el-button size="small" @click="createSession">新会话</el-button>
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
              <MessagesSquare :size="16" /><span class="workspace-history-copy"
                >会话 {{ session.id.slice(-6)
                }}<span class="workspace-history-time">{{
                  formatDateTime(session.updatedAt)
                }}</span></span
              >
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

    <el-drawer
      v-model="filesOpen"
      title="会话文件"
      size="560px"
      append-to-body
      destroy-on-close
    >
      <LanShareFilesPanel
        class="workspace-files"
        :current-session-id="currentSessionId"
        :can-manage-files="Boolean(currentSessionId)"
        :service-running="service.running"
        :state-version="stateVersion"
        @refresh-state="$emit('refresh-state')"
        @preview-file="$emit('preview-file', $event)"
      />
    </el-drawer>

    <el-drawer
      v-model="groupOpen"
      title="群聊设置"
      size="430px"
      append-to-body
      destroy-on-close
    >
      <div v-if="currentGroup" class="workspace-group">
        <label class="workspace-group-field"
          ><span>群名称</span><el-input v-model="groupDraft.name"
        /></label>
        <label class="workspace-group-field"
          ><span>新成员消息可见范围</span
          ><el-select v-model="groupDraft.messageVisibility"
            ><el-option label="全部历史" value="all" /><el-option
              label="加入后消息"
              value="afterJoin" /><el-option
              label="加入前最近 10 条"
              value="recent10" /></el-select
        ></label>
        <el-button
          type="primary"
          @click="
            $emit('update-group', { groupId: currentGroup.id, ...groupDraft })
          "
          >保存群设置</el-button
        >
        <div class="workspace-group-invite">
          <span>邀请访客加入</span>
          <div
            v-if="currentGroup.qrSvg"
            class="workspace-group-qr"
            v-html="currentGroup.qrSvg"
          ></div>
          <small>邀请码 {{ currentGroup.inviteCode }}</small
          ><el-button
            size="small"
            @click="
              $emit(
                'copy-text',
                currentGroup.inviteUrl || currentGroup.inviteCode
              )
            "
            >复制邀请链接</el-button
          >
        </div>
        <div class="workspace-group-members">
          <span>群成员 · {{ currentGroup.members?.length || 0 }}</span>
          <div
            v-for="member in currentGroup.members || []"
            :key="member.deviceId"
            class="workspace-group-member"
          >
            <span class="workspace-member-copy"
              >{{ member.deviceName || member.deviceId
              }}<span class="workspace-member-status">{{
                member.online ? "在线" : "离线"
              }}</span></span
            ><el-button
              size="small"
              text
              type="danger"
              @click="
                $emit('remove-group-member', {
                  groupId: currentGroup.id,
                  deviceId: member.deviceId
                })
              "
              >移出</el-button
            >
          </div>
        </div>
        <el-button
          type="danger"
          plain
          @click="$emit('clear-group-messages', currentGroup.id)"
          >清空群消息</el-button
        ><el-button type="danger" text @click="deleteGroup">解散群聊</el-button>
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
    : props.currentDevice?.name ||
      props.currentDevice?.autoName ||
      "选择设备，开始聊天"
)
const subtitle = computed(() =>
  props.chatMode === "group"
    ? `${props.currentGroup?.members?.length || 0} 位成员 · 文件与消息集中查看`
    : props.currentDevice
      ? `${props.currentDevice.online ? "在线" : props.currentDevice.native ? "已配对" : "离线"} · ${props.currentDevice.ip || ""}`
      : "文字、文件和图片，都在一条对话里"
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
    padding: 13px 18px;
    border-bottom: 1px solid var(--color-line);
    .workspace-identity {
      display: flex;
      min-width: 0;
      align-items: center;
      gap: 10px;
      .workspace-avatar {
        display: grid;
        width: 36px;
        height: 36px;
        flex: none;
        place-items: center;
        border-radius: 10px;
        color: var(--color-primary);
        background: var(--color-primary-soft);
      }
      .workspace-heading {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 5px;
        .workspace-name {
          overflow: hidden;
          color: var(--color-text);
          text-overflow: ellipsis;
          white-space: nowrap;
          font-size: var(--font-size-lg);
        }
        .workspace-subtitle {
          overflow: hidden;
          color: var(--color-text-muted);
          text-overflow: ellipsis;
          white-space: nowrap;
          font-size: var(--font-size-sm);
        }
      }
    }
    .workspace-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 3px;
      .workspace-icon {
        display: grid;
        width: 30px;
        height: 32px;
        place-items: center;
        padding: 0;
        border: 0;
        border-radius: 6px;
        color: var(--color-text-muted);
        background: transparent;
        &:hover {
          color: var(--color-primary);
          background: var(--color-primary-soft);
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
  .workspace-history-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
  .workspace-history-list {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    .workspace-history-row {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 8px;
      padding: 7px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      &.workspace-history-selected {
        border-color: var(--color-info-line);
        background: var(--color-primary-soft);
      }
      .workspace-history-select {
        display: flex;
        min-width: 0;
        flex: 1;
        align-items: center;
        gap: 10px;
        padding: 5px;
        border: 0;
        background: transparent;
        color: var(--color-text);
        text-align: left;
        .workspace-history-copy {
          min-width: 0;
          .workspace-history-time {
            display: block;
            margin-top: 5px;
            color: var(--color-text-muted);
            font-size: var(--font-size-sm);
          }
        }
      }
      .workspace-history-delete {
        padding: 5px;
        border: 0;
        background: transparent;
        color: var(--color-danger);
      }
    }
  }
}
.workspace-group {
  display: flex;
  flex-direction: column;
  gap: 18px;
  .workspace-group-field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: var(--font-size-base);
  }
  .workspace-group-invite {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 16px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    font-size: var(--font-size-base);
    .workspace-group-qr {
      width: 160px;
      padding: 8px;
      background: #fff;
      :deep(svg) {
        display: block;
        width: 100%;
        height: auto;
      }
    }
  }
  .workspace-group-members {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: var(--font-size-base);
    .workspace-group-member {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 10px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      .workspace-member-copy {
        min-width: 0;
        .workspace-member-status {
          display: block;
          margin-top: 5px;
          color: var(--color-text-muted);
        }
      }
    }
  }
}
</style>
