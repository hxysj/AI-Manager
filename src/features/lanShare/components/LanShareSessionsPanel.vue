<template>
  <section class="lan-share-sessions-panel">
    <header class="lan-share-sessions-head">
      <button
        class="lan-share-sessions-back"
        type="button"
        @click="emit('back-devices')"
      >
        <ArrowLeft :size="14" />
      </button>
      <div class="lan-share-sessions-title">
        <strong class="lan-share-sessions-name">
          {{ deviceName(currentDevice) }}
        </strong>
        <span class="lan-share-sessions-subtitle">
          {{ currentDevice?.online ? "在线" : "离线" }} ·
          {{ currentDevice?.ip || "未知 IP" }} · {{ sessions.length }} 个会话
        </span>
      </div>
      <div class="lan-share-sessions-actions">
        <button
          class="lan-share-sessions-mini-button"
          type="button"
          :disabled="!currentDevice"
          @click="emit('create-session')"
        >
          <Plus :size="13" />
          新会话
        </button>
        <button
          class="lan-share-sessions-mini-button"
          type="button"
          :disabled="!currentDevice"
          @click="emit('delete-history')"
        >
          <Trash2 :size="13" />
          删除历史
        </button>
      </div>
    </header>
    <div class="lan-share-sessions-list">
      <button
        v-for="session in sessions"
        :key="session.id"
        :class="[
          'lan-share-sessions-item',
          { 'lan-share-sessions-item-active': selectedSessionId === session.id }
        ]"
        type="button"
        @click="emit('select-session', session.id)"
      >
        <span class="lan-share-sessions-icon">
          <MessagesSquare :size="16" />
        </span>
        <span class="lan-share-sessions-main">
          <strong class="lan-share-sessions-session-name">
            {{ sessionTitle(session) }}
          </strong>
          <small class="lan-share-sessions-meta">
            {{ formatDateTime(session.updatedAt) }} ·
            {{ session.ip || currentDevice?.ip || "未知 IP" }}
          </small>
        </span>
      </button>
      <div v-if="!sessions.length" class="lan-share-sessions-empty">
        当前设备还没有会话记录，可以新建会话后发送消息或共享文件。
      </div>
    </div>
  </section>
</template>

<script setup>
import { ArrowLeft, MessagesSquare, Plus, Trash2 } from "lucide-vue-next"
import { formatDateTime } from "@/utils/formatters"

defineProps({
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
  }
})

const emit = defineEmits([
  "back-devices",
  "create-session",
  "delete-history",
  "select-session"
])

function deviceName(device) {
  return device?.name || device?.autoName || "未知设备"
}

function sessionTitle(session) {
  return `会话 ${String(session.id || "").slice(-6) || "未知"}`
}
</script>

<style scoped lang="less">
.lan-share-sessions-panel {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);

  .lan-share-sessions-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 52px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  .lan-share-sessions-back,
  .lan-share-sessions-mini-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-primary);
    cursor: pointer;
    font-weight: 700;
  }

  .lan-share-sessions-back {
    width: 30px;
    height: 30px;
    flex: none;
  }

  .lan-share-sessions-title {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 2px;
  }

  .lan-share-sessions-name,
  .lan-share-sessions-subtitle {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .lan-share-sessions-name {
    color: var(--color-text);
    font-size: 0.9rem;
  }

  .lan-share-sessions-subtitle {
    color: var(--color-text-muted);
    font-size: 0.76rem;
  }

  .lan-share-sessions-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
  }

  .lan-share-sessions-mini-button {
    height: 30px;
    padding: 0 9px;
    font-size: 0.76rem;
  }

  .lan-share-sessions-mini-button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .lan-share-sessions-list {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding: 10px;
  }

  .lan-share-sessions-item {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 9px;
    min-height: 58px;
    padding: 9px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
  }

  .lan-share-sessions-item-active {
    border-color: var(--color-info-line);
    background: var(--color-primary-soft);
  }

  .lan-share-sessions-icon {
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

  .lan-share-sessions-main {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 3px;
  }

  .lan-share-sessions-session-name,
  .lan-share-sessions-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .lan-share-sessions-session-name {
    color: var(--color-text);
    font-size: 0.84rem;
  }

  .lan-share-sessions-meta {
    color: var(--color-text-muted);
    font-size: 0.72rem;
  }

  .lan-share-sessions-empty {
    display: flex;
    min-height: 150px;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--color-line);
    border-radius: 8px;
    color: var(--color-text-muted);
    font-size: 0.82rem;
    line-height: 1.5;
    text-align: center;
  }
}
</style>
