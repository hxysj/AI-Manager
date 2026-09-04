<template>
  <section class="lan-share-session-workspace">
    <header class="lan-share-session-head">
      <button
        v-if="chatMode === 'direct'"
        class="lan-share-session-back"
        type="button"
        title="返回设备管理"
        @click="emit('back-devices')"
      >
        <ArrowLeft :size="15" />
      </button>
      <div v-if="chatMode === 'direct'" class="lan-share-session-device">
        <span class="lan-share-session-device-icon">
          <MonitorSmartphone :size="18" />
          <span
            :class="[
              'lan-share-session-status',
              { 'lan-share-session-status-online': currentDevice?.online }
            ]"
            :title="currentDevice?.online ? '在线' : '离线'"
          >
            <Wifi v-if="currentDevice?.online" :size="10" />
            <WifiOff v-else :size="10" />
          </span>
        </span>
        <span class="lan-share-session-device-main">
          <span data-emphasis class="lan-share-session-device-name">
            {{ deviceName }}
          </span>
          <small class="lan-share-session-device-meta">
            {{ currentDevice?.online ? "在线" : "离线" }} ·
            {{ currentDevice?.ip || "未知 IP" }} · {{ sessions.length }} 个会话
          </small>
        </span>
      </div>
      <div v-else class="lan-share-session-device">
        <span class="lan-share-session-device-icon">
          <Users :size="18" />
        </span>
        <span class="lan-share-session-device-main">
          <span data-emphasis class="lan-share-session-device-name">
            {{ currentGroup?.name || "群聊模式" }}
          </span>
          <small class="lan-share-session-device-meta">
            {{ groups.length }} 个群聊 · {{ currentGroupMemberCount }} 位成员
          </small>
        </span>
      </div>

      <div class="lan-share-session-actions">
        <span class="lan-share-session-mode">
          <button
            :class="[
              'lan-share-session-mode-button',
              { 'lan-share-session-mode-button-active': chatMode === 'direct' }
            ]"
            type="button"
            @click="emit('switch-mode', 'direct')"
          >
            <MonitorSmartphone :size="14" />
            单聊
          </button>
          <button
            :class="[
              'lan-share-session-mode-button',
              { 'lan-share-session-mode-button-active': chatMode === 'group' }
            ]"
            type="button"
            @click="emit('switch-mode', 'group')"
          >
            <Users :size="14" />
            群聊
          </button>
        </span>
        <button
          v-if="chatMode === 'direct'"
          class="lan-share-session-button"
          type="button"
          :disabled="!currentDevice"
          @click="emit('create-session')"
        >
          <Plus :size="14" />
          新会话
        </button>
        <button
          v-if="chatMode === 'direct'"
          class="lan-share-session-button"
          type="button"
          :disabled="!currentDevice"
          @click="emit('delete-history')"
        >
          <Trash2 :size="14" />
          删除历史
        </button>
        <button
          v-if="chatMode === 'group'"
          class="lan-share-session-button"
          type="button"
          @click="submitCreateGroup"
        >
          <Plus :size="14" />
          创建群
        </button>
      </div>
    </header>

    <div
      v-if="chatMode === 'direct' && currentDevice"
      class="lan-share-session-body"
    >
      <aside class="lan-share-session-sidebar">
        <header class="lan-share-session-sidebar-head">
          <span data-emphasis class="lan-share-session-sidebar-title">会话记录</span>
          <span class="lan-share-session-sidebar-count">
            {{ sortedSessions.length }} 条
          </span>
        </header>
        <div class="lan-share-session-list">
          <article
            v-for="session in sortedSessions"
            :key="session.id"
            :class="[
              'lan-share-session-item',
              {
                'lan-share-session-item-active':
                  selectedSessionId === session.id
              }
            ]"
          >
            <button
              class="lan-share-session-item-select"
              type="button"
              @click="emit('select-session', session.id)"
            >
              <span class="lan-share-session-item-icon">
                <MessagesSquare :size="15" />
              </span>
              <span class="lan-share-session-item-main">
                <span data-emphasis class="lan-share-session-item-title">
                  {{ sessionTitle(session) }}
                </span>
                <small class="lan-share-session-item-meta">
                  {{ formatDateTime(session.updatedAt) }}
                </small>
              </span>
            </button>
            <span class="lan-share-session-item-actions">
              <button
                class="lan-share-session-item-delete"
                type="button"
                title="删除会话"
                @click="emit('delete-session', session.id)"
              >
                <Trash2 :size="13" />
              </button>
            </span>
          </article>
          <div
            v-if="!sortedSessions.length"
            class="lan-share-session-list-empty"
          >
            当前设备还没有会话记录。
          </div>
        </div>
      </aside>

      <section class="lan-share-session-detail">
        <nav class="lan-share-session-tabs">
          <button
            :class="[
              'lan-share-session-tab',
              { 'lan-share-session-tab-active': detailTab === 'messages' }
            ]"
            type="button"
            @click="detailTab = 'messages'"
          >
            <MessagesSquare :size="15" />
            消息
          </button>
          <button
            :class="[
              'lan-share-session-tab',
              { 'lan-share-session-tab-active': detailTab === 'files' }
            ]"
            type="button"
            @click="detailTab = 'files'"
          >
            <Database :size="15" />
            共享文件
          </button>
        </nav>

        <LanShareMessagesPanel
          v-if="detailTab === 'messages'"
          class="lan-share-session-panel"
          :chat-mode="chatMode"
          :current-device="currentDevice"
          :current-session-id="currentSessionId"
          :current-session="currentSession"
          :state-version="stateVersion"
          @refresh-state="emit('refresh-state')"
        />
        <LanShareFilesPanel
          v-else-if="detailTab === 'files'"
          class="lan-share-session-panel"
          :current-session-id="currentSessionId"
          :can-manage-files="Boolean(currentSessionId)"
          :service-running="serviceRunning"
          :state-version="stateVersion"
          @refresh-state="emit('refresh-state')"
          @preview-file="emit('preview-file', $event)"
        />
      </section>
    </div>

    <div v-else-if="chatMode === 'group'" class="lan-share-session-body">
      <aside class="lan-share-session-sidebar">
        <header class="lan-share-session-sidebar-head">
          <span data-emphasis class="lan-share-session-sidebar-title">群聊列表</span>
          <span class="lan-share-session-sidebar-count">
            {{ groups.length }} 个
          </span>
        </header>
        <div class="lan-share-session-list">
          <article
            v-for="group in groups"
            :key="group.id"
            :class="[
              'lan-share-session-item',
              {
                'lan-share-session-item-active': currentGroup?.id === group.id
              }
            ]"
          >
            <button
              class="lan-share-session-item-select"
              type="button"
              @click="emit('select-group', group.id)"
            >
              <span class="lan-share-session-item-icon">
                <Users :size="15" />
              </span>
              <span class="lan-share-session-item-main">
                <span data-emphasis class="lan-share-session-item-title">
                  {{ group.name }}
                </span>
                <small class="lan-share-session-item-meta">
                  {{ group.members?.length || 0 }} 人 ·
                  {{ visibilityLabel(group.messageVisibility) }}
                </small>
              </span>
            </button>
          </article>
          <div v-if="!groups.length" class="lan-share-session-list-empty">
            当前还没有群聊，可以先创建一个群。
          </div>
        </div>
      </aside>

      <section class="lan-share-session-detail">
        <template v-if="currentGroup">
          <nav class="lan-share-session-tabs">
            <button
              :class="[
                'lan-share-session-tab',
                { 'lan-share-session-tab-active': detailTab === 'messages' }
              ]"
              type="button"
              @click="detailTab = 'messages'"
            >
              <MessagesSquare :size="15" />
              群消息
            </button>
            <button
              :class="[
                'lan-share-session-tab',
                { 'lan-share-session-tab-active': detailTab === 'files' }
              ]"
              type="button"
              @click="detailTab = 'files'"
            >
              <Database :size="15" />
              群文件
            </button>
            <span class="lan-share-session-tabs-spacer"></span>
            <button
              class="lan-share-session-manage-button"
              type="button"
              @click="toggleGroupManager"
            >
              <Settings2 :size="15" />
              群管理
            </button>
          </nav>

          <div v-if="groupManagerOpen" class="lan-share-session-group-manager">
            <header class="lan-share-session-manager-head">
              <span data-emphasis class="lan-share-session-manager-title">群管理</span>
              <button
                class="lan-share-session-manager-close"
                type="button"
                title="关闭"
                @click="groupManagerOpen = false"
              >
                <X :size="14" />
              </button>
            </header>
            <div class="lan-share-session-manager-grid">
              <label class="lan-share-session-group-field">
                <span class="lan-share-session-group-label">群名称</span>
                <input
                  v-model="groupDraft.name"
                  class="lan-share-session-group-input"
                  type="text"
                />
              </label>
              <label class="lan-share-session-group-field">
                <span class="lan-share-session-group-label">消息可见范围</span>
                <select
                  v-model="groupDraft.messageVisibility"
                  class="lan-share-session-group-select"
                >
                  <option value="all">可见所有历史消息</option>
                  <option value="afterJoin">仅可见加入后的消息</option>
                  <option value="recent10">加入前最多 10 条</option>
                </select>
              </label>
            </div>
            <div class="lan-share-session-manager-actions">
              <button
                class="lan-share-session-button"
                type="button"
                @click="submitUpdateGroup"
              >
                <Settings2 :size="14" />
                保存设置
              </button>
              <button
                class="lan-share-session-button lan-share-session-button-ghost"
                type="button"
                @click="emit('clear-group-messages', currentGroup.id)"
              >
                <MessagesSquare :size="14" />
                清空群消息
              </button>
              <button
                class="lan-share-session-button lan-share-session-button-danger"
                type="button"
                @click="emit('delete-group', currentGroup.id)"
              >
                <Trash2 :size="14" />
                解散群聊
              </button>
            </div>
            <div class="lan-share-session-invite">
              <div
                v-if="currentGroup.qrSvg"
                class="lan-share-session-invite-qr"
                v-html="currentGroup.qrSvg"
              ></div>
              <div class="lan-share-session-invite-copy">
                <span data-emphasis class="lan-share-session-invite-code">
                  邀请码 {{ currentGroup.inviteCode }}
                </span>
                <span class="lan-share-session-invite-url">
                  {{ currentGroup.inviteUrl || "启动服务后生成群二维码" }}
                </span>
              </div>
              <button
                class="lan-share-session-copy-button"
                type="button"
                @click="copyInviteText"
              >
                复制
              </button>
            </div>
            <div class="lan-share-session-members">
              <span
                v-for="member in currentGroup.members || []"
                :key="member.deviceId"
                class="lan-share-session-member"
              >
                <span class="lan-share-session-member-main">
                  <span data-emphasis class="lan-share-session-member-name">
                    {{ member.deviceName || member.deviceId }}
                  </span>
                  <small class="lan-share-session-member-status">
                    {{ member.online ? "在线" : "离线" }}
                  </small>
                </span>
                <button
                  class="lan-share-session-member-remove"
                  type="button"
                  title="移出群聊"
                  @click="
                    emit('remove-group-member', {
                      groupId: currentGroup.id,
                      deviceId: member.deviceId
                    })
                  "
                >
                  <X :size="12" />
                </button>
              </span>
              <span
                v-if="!(currentGroup.members || []).length"
                class="lan-share-session-member-empty"
              >
                还没有设备加入群聊。
              </span>
            </div>
          </div>

          <LanShareMessagesPanel
            v-if="detailTab === 'messages'"
            class="lan-share-session-panel"
            :chat-mode="chatMode"
            :current-device="groupMessageDevice"
            :current-session-id="currentSessionId"
            :current-session="currentSession"
            :state-version="stateVersion"
            @refresh-state="emit('refresh-state')"
          />
          <LanShareFilesPanel
            v-else-if="detailTab === 'files'"
            class="lan-share-session-panel"
            :current-session-id="currentSessionId"
            :can-manage-files="Boolean(currentSessionId)"
            :service-running="serviceRunning"
            :state-version="stateVersion"
            @refresh-state="emit('refresh-state')"
            @preview-file="emit('preview-file', $event)"
          />
        </template>
        <div
          v-else
          class="lan-share-session-empty lan-share-session-empty-inline"
        >
          请先创建或选择一个群聊后查看群消息和群文件。
        </div>
      </section>
    </div>

    <div v-else class="lan-share-session-empty">
      {{ emptyText }}
    </div>
  </section>
</template>

<script setup>
import { computed, reactive, ref, watch } from "vue"
import {
  ArrowLeft,
  Database,
  MessagesSquare,
  MonitorSmartphone,
  Plus,
  Settings2,
  Trash2,
  Users,
  Wifi,
  WifiOff,
  X
} from "lucide-vue-next"
import { formatDateTime } from "@/utils/formatters"
import LanShareFilesPanel from "./LanShareFilesPanel.vue"
import LanShareMessagesPanel from "./LanShareMessagesPanel.vue"

const props = defineProps({
  chatMode: {
    type: String,
    default: "direct"
  },
  groups: {
    type: Array,
    default: () => []
  },
  sessions: {
    type: Array,
    default: () => []
  },
  groupSessions: {
    type: Array,
    default: () => []
  },
  currentGroup: {
    type: Object,
    default: null
  },
  currentDevice: {
    type: Object,
    default: null
  },
  selectedSessionId: {
    type: String,
    default: ""
  },
  currentSession: {
    type: Object,
    default: null
  },
  currentSessionId: {
    type: String,
    default: ""
  },
  serviceRunning: {
    type: Boolean,
    default: false
  },
  stateVersion: {
    type: Number,
    default: 0
  }
})

const emit = defineEmits([
  "back-devices",
  "switch-mode",
  "select-session",
  "select-group",
  "delete-session",
  "create-session",
  "create-group",
  "update-group",
  "remove-group-member",
  "clear-group-messages",
  "delete-group",
  "delete-history",
  "refresh-state",
  "preview-file",
  "copy-text"
])

const detailTab = ref("messages")
const groupManagerOpen = ref(false)
const groupDraft = reactive({
  name: "",
  messageVisibility: "all"
})

const sortedSessions = computed(() => {
  return [...props.sessions].sort((left, right) => {
    return Number(right.updatedAt || 0) - Number(left.updatedAt || 0)
  })
})

const deviceName = computed(() => {
  return (
    props.currentDevice?.name || props.currentDevice?.autoName || "未知设备"
  )
})

const currentGroupMemberCount = computed(() => {
  return props.currentGroup?.members?.length || 0
})

const groupMessageDevice = computed(() => {
  return {
    id: props.currentSession?.deviceId || "",
    name: props.currentGroup?.name || "群聊"
  }
})

const emptyText = computed(() => {
  if (props.chatMode === "group") {
    return "请先创建或选择一个群聊。"
  }

  return "请返回设备管理选择设备后查看详情。"
})

watch(
  () => [
    props.currentGroup?.id || "",
    props.currentGroup?.name || "",
    props.currentGroup?.messageVisibility || ""
  ],
  () => {
    syncGroupDraft()
    groupManagerOpen.value = false
  },
  { immediate: true }
)

function sessionTitle(session) {
  return `会话 ${String(session.id || "").slice(-6) || "未知"}`
}

function visibilityLabel(value) {
  const map = {
    all: "全部历史",
    afterJoin: "加入后",
    recent10: "前 10 条"
  }

  return map[value] || map.all
}

function syncGroupDraft() {
  groupDraft.name = props.currentGroup?.name || ""
  groupDraft.messageVisibility = props.currentGroup?.messageVisibility || "all"
}

function submitCreateGroup() {
  emit("create-group", {
    name: "新的群聊",
    messageVisibility: "all"
  })
}

function submitUpdateGroup() {
  if (!props.currentGroup?.id) {
    return
  }

  emit("update-group", {
    groupId: props.currentGroup.id,
    name: groupDraft.name,
    messageVisibility: groupDraft.messageVisibility
  })
}

function toggleGroupManager() {
  groupManagerOpen.value = !groupManagerOpen.value
}

function copyInviteText() {
  emit(
    "copy-text",
    props.currentGroup?.inviteUrl || props.currentGroup?.inviteCode || ""
  )
}
</script>

<style scoped lang="less">
.lan-share-session-workspace {
  display: flex;
  min-width: 0;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;

  .lan-share-session-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 58px;
    padding: 10px 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: linear-gradient(180deg, var(--color-panel) 0%, var(--color-panel-soft) 100%);

    .lan-share-session-back {
      display: inline-flex;
      width: 34px;
      height: 34px;
      flex: none;
      align-items: center;
      justify-content: center;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
      color: var(--color-primary);
      cursor: pointer;
    }

    .lan-share-session-device {
      display: flex;
      min-width: 0;
      flex: 1;
      align-items: center;
      gap: 10px;

      .lan-share-session-device-icon {
        position: relative;
        display: inline-flex;
        width: 38px;
        height: 38px;
        flex: 0 0 38px;
        align-items: center;
        justify-content: center;
        border-radius: 8px;
        background: var(--color-primary-soft);
        color: var(--color-primary);

        .lan-share-session-status {
          position: absolute;
          right: -4px;
          bottom: -4px;
          display: inline-flex;
          width: 18px;
          height: 18px;
          align-items: center;
          justify-content: center;
          border: 2px solid #ffffff;
          border-radius: 999px;
          background: #a8b3c1;
          color: #ffffff;
        }

        .lan-share-session-status-online {
          background: var(--color-success);
        }
      }

      .lan-share-session-device-main {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 3px;

        .lan-share-session-device-name,
        .lan-share-session-device-meta {
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .lan-share-session-device-name {
          color: var(--color-text);
          font-size: 0.94rem;
        }

        .lan-share-session-device-meta {
          color: var(--color-text-muted);
          font-size: 0.76rem;
        }
      }
    }

    .lan-share-session-actions {
      display: flex;
      min-width: 0;
      flex: none;
      align-items: center;
      gap: 8px;

      .lan-share-session-mode {
        display: inline-flex;
        height: 32px;
        padding: 3px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel-soft);
        gap: 3px;

        .lan-share-session-mode-button {
          display: inline-flex;
          height: 24px;
          align-items: center;
          justify-content: center;
          gap: 5px;
          padding: 0 8px;
          border: 0;
          border-radius: 6px;
          background: transparent;
          color: var(--color-text-muted);
          cursor: pointer;
          font-size: 0.74rem;
        }

        .lan-share-session-mode-button-active {
          background: var(--color-panel);
          color: var(--color-primary);
          box-shadow: 0 4px 12px rgba(42, 67, 101, 0.08);
        }
      }

      .lan-share-session-button {
        display: inline-flex;
        height: 32px;
        flex: none;
        align-items: center;
        justify-content: center;
        gap: 6px;
        padding: 0 10px;
        border: 1px solid var(--color-primary);
        border-radius: 7px;
        background: var(--color-primary-solid);
        color: #ffffff;
        cursor: pointer;
      }

      .lan-share-session-button-ghost {
        border-color: var(--color-line);
        background: var(--color-panel);
        color: var(--color-primary);
      }

      .lan-share-session-button:disabled {
        cursor: not-allowed;
        opacity: 0.5;
      }
    }
  }

  .lan-share-session-body {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex: 1;
    gap: 12px;
    overflow: hidden;

    .lan-share-session-sidebar {
      display: flex;
      width: 270px;
      min-height: 0;
      flex: 0 0 270px;
      flex-direction: column;
      overflow: hidden;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);

      .lan-share-session-sidebar-head {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
        min-height: 46px;
        padding: 10px 12px;
        border-bottom: 1px solid var(--color-line);
        background: var(--color-panel-soft);

        .lan-share-session-sidebar-title {
          color: var(--color-text);
          font-size: 0.88rem;
        }

        .lan-share-session-sidebar-count {
          color: var(--color-text-muted);
          font-size: 0.74rem;
        }
      }

      .lan-share-session-list {
        display: flex;
        min-height: 0;
        flex: 1;
        flex-direction: column;
        gap: 8px;
        overflow: auto;
        padding: 10px;

        .lan-share-session-item {
          display: flex;
          width: 100%;
          align-items: center;
          gap: 6px;
          min-height: 56px;
          padding: 6px;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel);
          color: var(--color-text);
          text-align: left;

          .lan-share-session-item-select {
            display: flex;
            align-items: center;
            gap: 9px;
            min-width: 0;
            flex: 1;
            min-height: 42px;
            padding: 3px;
            border: 0;
            background: transparent;
            color: inherit;
            cursor: pointer;
            text-align: left;

            .lan-share-session-item-icon {
              display: inline-flex;
              width: 32px;
              height: 32px;
              flex: 0 0 32px;
              align-items: center;
              justify-content: center;
              border-radius: 7px;
              background: var(--color-primary-soft);
              color: var(--color-primary);
            }

            .lan-share-session-item-main {
              display: flex;
              min-width: 0;
              flex: 1;
              flex-direction: column;
              gap: 3px;

              .lan-share-session-item-title,
              .lan-share-session-item-meta {
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
              }

              .lan-share-session-item-title {
                color: var(--color-text);
                font-size: 0.84rem;
              }

              .lan-share-session-item-meta {
                color: var(--color-text-muted);
                font-size: 0.72rem;
              }
            }
          }

          .lan-share-session-item-actions {
            display: inline-flex;
            flex: none;
            align-items: center;
            justify-content: center;

            .lan-share-session-item-delete {
              display: inline-flex;
              width: 28px;
              height: 28px;
              align-items: center;
              justify-content: center;
              border: 1px solid var(--color-line);
              border-radius: 7px;
              background: var(--color-panel);
              color: var(--color-text-muted);
              cursor: pointer;
            }
          }
        }

        .lan-share-session-item-active {
          border-color: var(--color-info-line);
          background: var(--color-primary-soft);
        }

        .lan-share-session-list-empty {
          display: flex;
          min-height: 120px;
          align-items: center;
          justify-content: center;
          border: 1px dashed var(--color-line);
          border-radius: 8px;
          color: var(--color-text-muted);
          font-size: 0.82rem;
          text-align: center;
        }
      }
    }

    .lan-share-session-detail {
      display: flex;
      min-width: 0;
      min-height: 0;
      flex: 1;
      flex-direction: column;
      gap: 10px;
      overflow: hidden;

      .lan-share-session-button {
        display: inline-flex;
        height: 32px;
        flex: none;
        align-items: center;
        justify-content: center;
        gap: 6px;
        padding: 0 10px;
        border: 1px solid var(--color-primary);
        border-radius: 7px;
        background: var(--color-primary-solid);
        color: #ffffff;
        cursor: pointer;
      }

      .lan-share-session-button-ghost {
        border-color: var(--color-line);
        background: var(--color-panel);
        color: var(--color-primary);
      }

      .lan-share-session-button-danger {
        border-color: var(--color-danger);
        background: var(--color-danger);
      }

      .lan-share-session-group-field {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 4px;
        color: var(--color-text-muted);
        font-size: 0.72rem;
      }

      .lan-share-session-group-input,
      .lan-share-session-group-select {
        height: 32px;
        min-width: 0;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel);
        color: var(--color-text);
        padding: 0 9px;
      }

      .lan-share-session-group-manager {
        display: flex;
        flex: none;
        flex-direction: column;
        gap: 10px;
        overflow: hidden;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        padding: 10px;

        .lan-share-session-manager-head {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 8px;

          .lan-share-session-manager-title {
            color: var(--color-text);
            font-size: 0.86rem;
          }

          .lan-share-session-manager-close {
            display: inline-flex;
            width: 26px;
            height: 26px;
            align-items: center;
            justify-content: center;
            border: 1px solid var(--color-line);
            border-radius: 7px;
            background: var(--color-panel);
            color: var(--color-text-muted);
            cursor: pointer;
          }
        }

        .lan-share-session-manager-grid,
        .lan-share-session-manager-actions {
          display: flex;
          align-items: flex-end;
          gap: 8px;
        }

        .lan-share-session-invite {
          display: flex;
          align-items: center;
          gap: 10px;
          min-height: 70px;
          border: 1px dashed var(--color-line-strong);
          border-radius: 8px;
          background: var(--color-panel-soft);
          padding: 8px;

          .lan-share-session-invite-qr {
            display: flex;
            width: 58px;
            height: 58px;
            flex: none;
            align-items: center;
            justify-content: center;
            overflow: hidden;
            border: 1px solid var(--color-line);
            border-radius: 7px;
            background: var(--color-panel);
          }

          .lan-share-session-invite-qr :deep(svg) {
            width: 54px;
            height: 54px;
          }

          .lan-share-session-invite-copy {
            display: flex;
            min-width: 0;
            flex-direction: column;
            gap: 4px;

            .lan-share-session-invite-code {
              color: var(--color-text);
              font-size: 0.86rem;
            }

            .lan-share-session-invite-url {
              overflow: hidden;
              color: var(--color-text-muted);
              font-size: 0.74rem;
              text-overflow: ellipsis;
              white-space: nowrap;
            }
          }

          .lan-share-session-copy-button {
            display: inline-flex;
            height: 30px;
            flex: none;
            align-items: center;
            justify-content: center;
            padding: 0 10px;
            border: 1px solid var(--color-line);
            border-radius: 7px;
            background: var(--color-panel);
            color: var(--color-primary);
            cursor: pointer;
          }
        }

        .lan-share-session-members {
          display: flex;
          min-height: 34px;
          align-items: center;
          gap: 8px;
          overflow: auto;

          .lan-share-session-member {
            display: inline-flex;
            height: 34px;
            flex: none;
            align-items: center;
            gap: 8px;
            padding: 0 7px 0 9px;
            border: 1px solid var(--color-line);
            border-radius: 8px;
            background: var(--color-panel);

            .lan-share-session-member-main {
              display: flex;
              flex-direction: column;
              gap: 1px;
            }

            .lan-share-session-member-name {
              color: var(--color-text);
              font-size: 0.74rem;
            }

            .lan-share-session-member-status {
              color: var(--color-text-muted);
              font-size: 0.66rem;
            }

            .lan-share-session-member-remove {
              display: inline-flex;
              width: 20px;
              height: 20px;
              align-items: center;
              justify-content: center;
              border: 0;
              border-radius: 6px;
              background: var(--color-panel-soft);
              color: var(--color-text-muted);
              cursor: pointer;
            }
          }

          .lan-share-session-member-empty {
            color: var(--color-text-muted);
            font-size: 0.78rem;
          }
        }
      }

      .lan-share-session-tabs {
        display: flex;
        flex: none;
        align-items: center;
        gap: 8px;
        padding: 8px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel-soft);

        .lan-share-session-tab {
          display: inline-flex;
          height: 32px;
          align-items: center;
          justify-content: center;
          gap: 6px;
          padding: 0 12px;
          border: 1px solid transparent;
          border-radius: 7px;
          background: transparent;
          color: var(--color-text-muted);
          cursor: pointer;
        }

        .lan-share-session-tab-active {
          border-color: var(--color-info-line);
          background: var(--color-panel);
          color: var(--color-primary);
          box-shadow: 0 6px 18px rgba(42, 67, 101, 0.08);
        }

        .lan-share-session-tabs-spacer {
          min-width: 0;
          flex: 1;
        }

        .lan-share-session-manage-button {
          display: inline-flex;
          height: 32px;
          flex: none;
          align-items: center;
          justify-content: center;
          gap: 6px;
          padding: 0 11px;
          border: 1px solid var(--color-line);
          border-radius: 7px;
          background: var(--color-panel);
          color: var(--color-primary);
          cursor: pointer;
        }
      }

      .lan-share-session-panel {
        min-height: 0;
        flex: 1;
      }
    }
  }

  .lan-share-session-empty {
    display: flex;
    min-height: 0;
    flex: 1;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    font-size: 0.86rem;
  }

  .lan-share-session-empty-inline {
    min-height: 0;
  }
}
</style>
