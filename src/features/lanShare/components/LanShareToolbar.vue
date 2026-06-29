<template>
  <header class="lan-share-toolbar">
    <div class="lan-share-toolbar-title">
      <span class="lan-share-toolbar-mark">Device Drop</span>
      <strong class="lan-share-toolbar-name">设备快传</strong>
      <small class="lan-share-toolbar-summary">{{ serviceSummary }}</small>
    </div>
    <div class="lan-share-toolbar-status">
      <span
        :class="[
          'lan-share-toolbar-dot',
          { 'lan-share-toolbar-dot-active': service.running }
        ]"
      ></span>
      <span>{{ service.running ? "运行中" : "未启动" }}</span>
    </div>
    <div class="lan-share-toolbar-actions">
      <button
        class="lan-share-toolbar-button lan-share-toolbar-button-primary"
        type="button"
        :disabled="loading || service.running"
        @click="emit('start')"
      >
        <Play :size="14" />
        启动服务
      </button>
      <button
        class="lan-share-toolbar-button"
        type="button"
        :disabled="loading || !service.running"
        @click="emit('show-access')"
      >
        <QrCode :size="14" />
        访问二维码
      </button>
      <button
        class="lan-share-toolbar-button"
        type="button"
        :disabled="loading || !service.running"
        @click="emit('stop')"
      >
        <Square :size="14" />
        关闭服务
      </button>
    </div>
  </header>
</template>

<script setup>
import { Play, QrCode, Square } from "lucide-vue-next"

defineProps({
  service: {
    type: Object,
    required: true
  },
  loading: {
    type: Boolean,
    default: false
  },
  serviceSummary: {
    type: String,
    required: true
  }
})

const emit = defineEmits(["start", "show-access", "stop"])
</script>

<style scoped lang="less">
.lan-share-toolbar {
  display: flex;
  flex: none;
  align-items: center;
  gap: 14px;
  padding: 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: linear-gradient(135deg, #ffffff 0%, #f8fbfe 100%);

  .lan-share-toolbar-title {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 3px;

    .lan-share-toolbar-mark {
      color: var(--color-text-soft);
      font-size: 0.68rem;
      font-weight: 800;
      letter-spacing: 0.12em;
      text-transform: uppercase;
    }

    .lan-share-toolbar-name {
      color: var(--color-primary);
      font-size: 1rem;
      line-height: 1.2;
    }

    .lan-share-toolbar-summary {
      overflow: hidden;
      color: var(--color-text-muted);
      font-size: 0.76rem;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .lan-share-toolbar-status {
    display: inline-flex;
    height: 30px;
    flex: none;
    align-items: center;
    gap: 7px;
    padding: 0 10px;
    border: 1px solid #d6e0eb;
    border-radius: 999px;
    background: #ffffff;
    color: #526176;
    font-size: 0.76rem;
    font-weight: 800;

    .lan-share-toolbar-dot {
      width: 7px;
      height: 7px;
      border-radius: 999px;
      background: #a7b1bf;
    }

    .lan-share-toolbar-dot-active {
      background: #19a35b;
    }
  }

  .lan-share-toolbar-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;

    .lan-share-toolbar-button {
      display: inline-flex;
      height: 34px;
      align-items: center;
      justify-content: center;
      gap: 6px;
      padding: 0 12px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: #ffffff;
      color: var(--color-primary);
      cursor: pointer;
      font-weight: 700;
    }

    .lan-share-toolbar-button-primary {
      border-color: var(--color-primary);
      background: var(--color-primary);
      color: #ffffff;
    }

    .lan-share-toolbar-button:disabled {
      cursor: not-allowed;
      opacity: 0.5;
    }
  }
}
</style>
