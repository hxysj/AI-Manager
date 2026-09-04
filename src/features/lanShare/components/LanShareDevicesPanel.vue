<template>
  <section class="lan-share-devices-panel">
    <header class="lan-share-devices-head">
      <div class="lan-share-devices-title">
        <strong class="lan-share-devices-name">设备管理</strong>
        <span class="lan-share-devices-subtitle">
          {{ devices.length }} 台设备 · {{ onlineDevices || 0 }} 台在线
        </span>
      </div>
      <div class="lan-share-devices-actions">
        <input
          v-model="keyword"
          class="lan-share-devices-search"
          type="search"
          placeholder="搜索设备名称或 IP"
        />
        <select v-model.number="pageSize" class="lan-share-devices-select">
          <option v-for="item in pageSizeOptions" :key="item" :value="item">
            每页 {{ item }}
          </option>
        </select>
      </div>
    </header>

    <div class="lan-share-devices-list">
      <button
        v-for="device in pagedDevices"
        :key="device.id"
        :class="[
          'lan-share-devices-item',
          { 'lan-share-devices-item-active': selectedDeviceId === device.id }
        ]"
        type="button"
        @click="emit('open-device', device.id)"
      >
        <span class="lan-share-devices-icon">
          <MonitorSmartphone :size="18" />
          <span
            :class="[
              'lan-share-devices-status',
              { 'lan-share-devices-status-online': device.online }
            ]"
            :title="device.online ? '在线' : '离线'"
          >
            <Wifi v-if="device.online" :size="10" />
            <WifiOff v-else :size="10" />
          </span>
        </span>
        <span class="lan-share-devices-main">
          <strong class="lan-share-devices-device-name">
            {{ deviceName(device) }}
          </strong>
          <small class="lan-share-devices-meta">
            {{ device.online ? "在线" : "离线" }} ·
            {{ device.ip || "未知 IP" }} ·
            {{ deviceSessionCount(device.id) }} 个会话
          </small>
          <small class="lan-share-devices-meta">
            最近连接 {{ formatDateTime(device.lastSeenAt) }}
          </small>
        </span>
        <span class="lan-share-devices-row-actions">
          <button
            class="lan-share-devices-icon-button"
            type="button"
            title="新建会话"
            @click.stop="emit('create-session', device.id)"
          >
            <Plus :size="14" />
          </button>
          <button
            class="lan-share-devices-icon-button"
            type="button"
            title="删除设备历史"
            @click.stop="emit('delete-history', device.id)"
          >
            <Trash2 :size="14" />
          </button>
        </span>
      </button>

      <div v-if="!pagedDevices.length" class="lan-share-devices-empty">
        暂无设备。启动服务后，其他设备访问二维码地址会出现在这里。
      </div>
    </div>

    <footer v-if="filteredDevices.length" class="lan-share-devices-pagination">
      <span class="lan-share-devices-page-info">
        {{ pageStart }}-{{ pageEnd }} / {{ filteredDevices.length }}
      </span>
      <div class="lan-share-devices-page-actions">
        <button
          class="lan-share-devices-page-button"
          type="button"
          :disabled="currentPage === 1"
          @click="setPage(currentPage - 1)"
        >
          <ChevronLeft :size="14" />
          上一页
        </button>
        <strong class="lan-share-devices-current-page">
          {{ currentPage }} / {{ pageCount }}
        </strong>
        <button
          class="lan-share-devices-page-button"
          type="button"
          :disabled="currentPage === pageCount"
          @click="setPage(currentPage + 1)"
        >
          下一页
          <ChevronRight :size="14" />
        </button>
      </div>
    </footer>
  </section>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import {
  ChevronLeft,
  ChevronRight,
  MonitorSmartphone,
  Plus,
  Trash2,
  Wifi,
  WifiOff
} from "lucide-vue-next"
import { formatDateTime } from "@/utils/formatters"

const props = defineProps({
  devices: {
    type: Array,
    default: () => []
  },
  sessions: {
    type: Array,
    default: () => []
  },
  selectedDeviceId: {
    type: String,
    default: ""
  },
  onlineDevices: {
    type: Number,
    default: 0
  }
})

const emit = defineEmits(["open-device", "create-session", "delete-history"])

const keyword = ref("")
const page = ref(1)
const pageSize = ref(8)
const pageSizeOptions = [8, 12, 20]

const filteredDevices = computed(() => {
  const text = keyword.value.trim().toLowerCase()

  return [...props.devices]
    .filter((device) => {
      return (
        !text ||
        deviceName(device).toLowerCase().includes(text) ||
        String(device.ip || "")
          .toLowerCase()
          .includes(text)
      )
    })
    .sort((left, right) => {
      return Number(right.lastSeenAt || 0) - Number(left.lastSeenAt || 0)
    })
})

const pageCount = computed(() => {
  return Math.max(1, Math.ceil(filteredDevices.value.length / pageSize.value))
})

const currentPage = computed(() => {
  return Math.min(page.value, pageCount.value)
})

const pageStart = computed(() => {
  if (!filteredDevices.value.length) {
    return 0
  }

  return (currentPage.value - 1) * pageSize.value + 1
})

const pageEnd = computed(() => {
  return Math.min(
    currentPage.value * pageSize.value,
    filteredDevices.value.length
  )
})

const pagedDevices = computed(() => {
  return filteredDevices.value.slice(pageStart.value - 1, pageEnd.value)
})

watch([keyword, pageSize, () => props.devices], () => {
  page.value = 1
})

function setPage(nextPage) {
  page.value = Math.min(Math.max(nextPage, 1), pageCount.value)
}

function deviceName(device) {
  return device.name || device.autoName || "未知设备"
}

function deviceSessionCount(deviceId) {
  return props.sessions.filter((session) => session.deviceId === deviceId)
    .length
}
</script>

<style scoped lang="less">
.lan-share-devices-panel {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);

  .lan-share-devices-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 52px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);

    .lan-share-devices-title {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 2px;

      .lan-share-devices-name {
        color: var(--color-text);
        font-size: 0.94rem;
      }

      .lan-share-devices-subtitle {
        color: var(--color-text-muted);
        font-size: 0.76rem;
      }
    }

    .lan-share-devices-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 8px;

      .lan-share-devices-search,
      .lan-share-devices-select {
        height: 32px;
        min-width: 0;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel);
        color: var(--color-text);
      }

      .lan-share-devices-search {
        width: 220px;
        padding: 0 10px;
      }

      .lan-share-devices-select {
        width: 96px;
        padding: 0 8px;
      }
    }
  }

  .lan-share-devices-list {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding: 10px;

    .lan-share-devices-item {
      display: flex;
      width: 100%;
      align-items: center;
      gap: 10px;
      min-height: 68px;
      padding: 10px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-text);
      cursor: pointer;
      text-align: left;

      .lan-share-devices-icon {
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

        .lan-share-devices-status {
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

        .lan-share-devices-status-online {
          background: var(--color-success);
        }
      }

      .lan-share-devices-main {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 3px;

        .lan-share-devices-device-name,
        .lan-share-devices-meta {
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .lan-share-devices-device-name {
          color: var(--color-text);
          font-size: 0.88rem;
        }

        .lan-share-devices-meta {
          color: var(--color-text-muted);
          font-size: 0.72rem;
        }
      }

      .lan-share-devices-row-actions {
        display: flex;
        flex: none;
        align-items: center;
        gap: 8px;

        .lan-share-devices-icon-button {
          display: inline-flex;
          width: 30px;
          height: 30px;
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

        .lan-share-devices-icon-button:disabled {
          cursor: not-allowed;
          opacity: 0.5;
        }
      }
    }

    .lan-share-devices-item-active {
      border-color: var(--color-info-line);
      background: var(--color-primary-soft);
    }

    .lan-share-devices-empty {
      display: flex;
      min-height: 180px;
      align-items: center;
      justify-content: center;
      border: 1px dashed var(--color-line);
      border-radius: 8px;
      color: var(--color-text-muted);
      font-size: 0.84rem;
    }
  }

  .lan-share-devices-pagination {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 9px 10px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel-soft);

    .lan-share-devices-page-info,
    .lan-share-devices-current-page {
      color: var(--color-text-muted);
      font-size: 0.76rem;
      font-weight: 700;
    }

    .lan-share-devices-page-actions {
      display: flex;
      align-items: center;
      gap: 8px;

      .lan-share-devices-page-button {
        display: inline-flex;
        height: 30px;
        align-items: center;
        justify-content: center;
        gap: 6px;
        padding: 0 9px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel);
        color: var(--color-primary);
        cursor: pointer;
        font-size: 0.76rem;
        font-weight: 700;
      }

      .lan-share-devices-page-button:disabled {
        cursor: not-allowed;
        opacity: 0.5;
      }
    }
  }
}
</style>
