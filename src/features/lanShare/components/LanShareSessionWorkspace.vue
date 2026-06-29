<template>
  <section class="lan-share-session-workspace">
    <header class="lan-share-session-head">
      <button
        class="lan-share-session-back"
        type="button"
        title="返回设备管理"
        @click="emit('back-devices')"
      >
        <ArrowLeft :size="15" />
      </button>
      <div class="lan-share-session-device">
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
          <strong class="lan-share-session-device-name">
            {{ deviceName }}
          </strong>
          <small class="lan-share-session-device-meta">
            {{ currentDevice?.online ? "在线" : "离线" }} ·
            {{ currentDevice?.ip || "未知 IP" }} · {{ sessions.length }} 个会话
          </small>
        </span>
      </div>

      <div class="lan-share-session-actions">
        <button
          class="lan-share-session-button"
          type="button"
          :disabled="!currentDevice"
          @click="emit('create-session')"
        >
          <Plus :size="14" />
          新会话
        </button>
        <button
          class="lan-share-session-button"
          type="button"
          :disabled="!currentDevice"
          @click="emit('delete-history')"
        >
          <Trash2 :size="14" />
          删除历史
        </button>
      </div>
    </header>

    <div v-if="currentDevice" class="lan-share-session-body">
      <aside class="lan-share-session-sidebar">
        <header class="lan-share-session-sidebar-head">
          <strong class="lan-share-session-sidebar-title">会话记录</strong>
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
                <strong class="lan-share-session-item-title">
                  {{ sessionTitle(session) }}
                </strong>
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
          <div v-if="!sortedSessions.length" class="lan-share-session-list-empty">
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

    <div v-else class="lan-share-session-empty">
      请返回设备管理选择设备后查看详情。
    </div>
  </section>
</template>

<script setup>
import { computed, ref } from "vue"
import {
  ArrowLeft,
  Database,
  MessagesSquare,
  MonitorSmartphone,
  Plus,
  Trash2,
  Wifi,
  WifiOff
} from "lucide-vue-next"
import { formatDateTime } from "@/utils/formatters"
import LanShareFilesPanel from "./LanShareFilesPanel.vue"
import LanShareMessagesPanel from "./LanShareMessagesPanel.vue"

const props = defineProps({
  sessions: {
    type: Array,
    default: () => []
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
  "select-session",
  "delete-session",
  "create-session",
  "delete-history",
  "refresh-state",
  "preview-file"
])

const detailTab = ref("messages")

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

function sessionTitle(session) {
  return `会话 ${String(session.id || "").slice(-6) || "未知"}`
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
    background: linear-gradient(180deg, #ffffff 0%, #f8fafc 100%);

    .lan-share-session-back {
      display: inline-flex;
      width: 34px;
      height: 34px;
      flex: none;
      align-items: center;
      justify-content: center;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: #ffffff;
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
        background: #eef5fb;
        color: #356b9b;

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
          background: #22a35a;
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
        background: var(--color-primary);
        color: #ffffff;
        cursor: pointer;
        font-weight: 700;
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
        background: #f8fafc;

        .lan-share-session-sidebar-title {
          color: var(--color-text);
          font-size: 0.88rem;
        }

        .lan-share-session-sidebar-count {
          color: var(--color-text-muted);
          font-size: 0.74rem;
          font-weight: 700;
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
          background: #ffffff;
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
              background: #eef5fb;
              color: #356b9b;
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
              background: #ffffff;
              color: var(--color-text-muted);
              cursor: pointer;
            }
          }
        }

        .lan-share-session-item-active {
          border-color: #8db7dc;
          background: #eef6ff;
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

      .lan-share-session-tabs {
        display: flex;
        flex: none;
        align-items: center;
        gap: 8px;
        padding: 8px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: #f8fafc;

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
          font-weight: 700;
        }

        .lan-share-session-tab-active {
          border-color: #8db7dc;
          background: #ffffff;
          color: var(--color-primary);
          box-shadow: 0 6px 18px rgba(42, 67, 101, 0.08);
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
    background: #ffffff;
    color: var(--color-text-muted);
    font-size: 0.86rem;
  }
}
</style>
