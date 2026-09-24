<template>
  <section class="port-monitor">
    <!-- 顶部概览与检索控制栏 -->
    <section class="port-monitor-toolbar-card">
      <div class="port-overview-block">
        <div class="port-overview-icon-box">
          <!-- Monitor with pulse/activity wave inside matching screenshot -->
          <svg
            viewBox="0 0 24 24"
            width="22"
            height="22"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <rect width="20" height="14" x="2" y="3" rx="2" />
            <path d="M8 21h8" />
            <path d="M12 17v4" />
            <path d="m6 10 3-3 3 6 3-3" stroke-width="1.8" />
          </svg>
        </div>
        <div class="port-overview-info">
          <span class="port-overview-label">本机端口概览</span>
          <div class="port-overview-stats">
            <span class="stat-group">
              <strong class="stat-num">{{ ports.length }}</strong>
              <span class="stat-unit">个端口</span>
            </span>
            <span class="stat-divider">|</span>
            <span class="stat-group">
              <strong class="stat-num stat-num--blue">{{ processCount }}</strong>
              <span class="stat-unit">个程序</span>
            </span>
          </div>
        </div>
      </div>

      <div class="port-search-wrapper">
        <Search :size="15" class="port-search-icon" />
        <input
          v-model.trim="keyword"
          class="port-search-input"
          type="search"
          placeholder="搜索端口、程序、服务或 PID"
        />
        <button
          v-if="keyword"
          class="port-search-clear-btn"
          type="button"
          title="清空搜索"
          @click="keyword = ''"
        >
          <X :size="12" />
        </button>
      </div>

      <div class="port-protocol-tabs" role="group" aria-label="端口协议筛选">
        <button
          v-for="option in protocolOptions"
          :key="option.value"
          class="protocol-tab-btn"
          :class="{ active: protocolFilter === option.value }"
          type="button"
          @click="protocolFilter = option.value"
        >
          <span class="protocol-tab-label">{{ option.label }}</span>
          <span class="protocol-tab-count">{{ option.count }}</span>
        </button>
      </div>

      <div class="port-updated-text">
        最后刷新: {{ lastUpdatedAt || "尚未刷新" }}
      </div>
    </section>

    <!-- 端口表格展示容器 -->
    <section class="port-monitor-table-shell">
      <div v-if="loading && !ports.length" class="port-monitor-state">
        <RefreshCw class="spinning" :size="24" />
        <span class="port-monitor-state-text">正在读取本机端口...</span>
      </div>

      <div
        v-else-if="loadError && !ports.length"
        class="port-monitor-state error"
      >
        <TriangleAlert :size="24" />
        <span class="port-monitor-state-title">端口读取失败</span>
        <span class="port-monitor-state-text">{{ loadError }}</span>
        <button
          class="port-monitor-retry-button"
          type="button"
          @click="loadPorts"
        >
          重新读取
        </button>
      </div>

      <div v-else-if="!filteredPorts.length" class="port-monitor-state">
        <Network :size="28" />
        <span class="port-monitor-state-title">
          {{ ports.length ? "没有匹配的端口" : "未检测到监听端口" }}
        </span>
      </div>

      <div v-else class="port-monitor-table-scroll">
        <table class="port-monitor-table">
          <colgroup>
            <col class="col-program" />
            <col class="col-endpoint" />
            <col class="col-protocol" />
            <col class="col-pid" />
            <col class="col-path" />
            <col class="col-action" />
          </colgroup>
          <thead class="port-monitor-table-head">
            <tr>
              <th
                class="table-heading sortable"
                title="按程序名排序"
                @click="toggleSort('process')"
              >
                <div class="heading-content">
                  <span>程序 / 服务</span>
                  <ArrowUpDown
                    :size="12"
                    class="sort-icon"
                    :class="{ active: sortField === 'process' }"
                  />
                </div>
              </th>
              <th
                class="table-heading sortable"
                title="按端口排序"
                @click="toggleSort('port')"
              >
                <div class="heading-content">
                  <span>端口 / 地址</span>
                  <ArrowUpDown
                    :size="12"
                    class="sort-icon"
                    :class="{ active: sortField === 'port' }"
                  />
                </div>
              </th>
              <th
                class="table-heading sortable"
                title="按协议排序"
                @click="toggleSort('protocol')"
              >
                <div class="heading-content">
                  <span>协议</span>
                  <ArrowUpDown
                    :size="12"
                    class="sort-icon"
                    :class="{ active: sortField === 'protocol' }"
                  />
                </div>
              </th>
              <th
                class="table-heading sortable"
                title="按 PID 排序"
                @click="toggleSort('pid')"
              >
                <div class="heading-content">
                  <span>PID</span>
                  <ArrowUpDown
                    :size="12"
                    class="sort-icon"
                    :class="{ active: sortField === 'pid' }"
                  />
                </div>
              </th>
              <th class="table-heading">可执行文件</th>
              <th class="table-heading action">操作</th>
            </tr>
          </thead>
          <tbody class="port-monitor-table-body">
            <tr
              v-for="(port, portIndex) in filteredPorts"
              :key="`${port.id}-${portIndex}`"
              class="port-table-row"
            >
              <!-- 程序 / 服务 -->
              <td class="table-cell cell-program">
                <div class="process-flex">
                  <div
                    class="process-icon-box"
                    :class="getProcessVisual(port).type"
                  >
                    <!-- Windows 4-tiles logo -->
                    <svg
                      v-if="getProcessVisual(port).type === 'windows'"
                      viewBox="0 0 24 24"
                      width="16"
                      height="16"
                      fill="currentColor"
                    >
                      <rect x="3" y="3" width="8" height="8" rx="1.5" />
                      <rect x="13" y="3" width="8" height="8" rx="1.5" />
                      <rect x="3" y="13" width="8" height="8" rx="1.5" />
                      <rect x="13" y="13" width="8" height="8" rx="1.5" />
                    </svg>
                    <!-- Settings Gear -->
                    <Settings
                      v-else-if="getProcessVisual(port).type === 'service'"
                      :size="16"
                    />
                    <!-- Network / Share2 Nodes -->
                    <Share2
                      v-else-if="getProcessVisual(port).type === 'network'"
                      :size="16"
                    />
                    <!-- Shield for protected system -->
                    <ShieldCheck
                      v-else-if="getProcessVisual(port).type === 'shield'"
                      :size="16"
                    />
                    <!-- Terminal for cli/dev processes -->
                    <Terminal
                      v-else-if="getProcessVisual(port).type === 'terminal'"
                      :size="16"
                    />
                    <!-- Default App Window -->
                    <AppWindow v-else :size="16" />
                  </div>
                  <div class="process-meta">
                    <span class="process-name" :title="port.processName">
                      {{ port.processName || "未知进程" }}
                    </span>
                    <span
                      v-if="port.serviceNames?.length"
                      class="process-sub"
                      :title="port.serviceNames.join('、')"
                    >
                      {{ port.serviceNames.join("、") }}
                    </span>
                    <span
                      v-else-if="port.protectedReason"
                      class="process-sub"
                      :title="port.protectedReason"
                    >
                      {{ port.protectedReason }}
                    </span>
                    <span
                      v-else
                      class="process-sub"
                    >
                      PID {{ port.pid }}
                    </span>
                  </div>
                </div>
              </td>

              <!-- 端口 / 地址 -->
              <td class="table-cell cell-endpoint">
                <div class="endpoint-flex">
                  <span class="endpoint-port">{{ port.localPort }}</span>
                  <span class="endpoint-address" :title="port.localAddress">
                    {{ formatAddress(port.localAddress) }}
                  </span>
                </div>
              </td>

              <!-- 协议 -->
              <td class="table-cell cell-protocol">
                <span
                  class="protocol-pill"
                  :class="port.protocol.toLowerCase()"
                >
                  {{ port.protocol }}
                </span>
              </td>

              <!-- PID -->
              <td class="table-cell cell-pid">
                <span class="pid-text">{{ port.pid }}</span>
              </td>

              <!-- 可执行文件 -->
              <td class="table-cell cell-path">
                <span
                  class="path-text"
                  :class="{ 'is-empty': !port.executablePath }"
                  :title="
                    port.executablePath || '当前权限无法读取可执行文件路径'
                  "
                >
                  {{ port.executablePath || "路径不可用" }}
                </span>
              </td>

              <!-- 操作 -->
              <td class="table-cell cell-action">
                <button
                  class="stop-proc-btn"
                  type="button"
                  :title="
                    port.canTerminate
                      ? '关闭进程'
                      : port.protectedReason || '系统核心进程不可关闭'
                  "
                  :disabled="!port.canTerminate || Boolean(terminatingPid)"
                  @click="terminateProcess(port)"
                >
                  <LoaderCircle
                    v-if="terminatingPid === port.pid"
                    class="spinning"
                    :size="15"
                  />
                  <!-- Target/bullseye/circle-dot matching screenshot -->
                  <svg
                    v-else
                    viewBox="0 0 24 24"
                    width="15"
                    height="15"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <circle cx="12" cy="12" r="9" />
                    <circle cx="12" cy="12" r="2.5" fill="currentColor" />
                  </svg>
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  </section>
</template>

<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from "vue"
import {
  AppWindow,
  ArrowUpDown,
  LoaderCircle,
  Network,
  RefreshCw,
  Search,
  Settings,
  Share2,
  ShieldCheck,
  Terminal,
  TriangleAlert,
  X
} from "lucide-vue-next"
import { toolboxApi } from "@/api"
import { createMessage } from "@/utils/message"

// 自动刷新与父级（工具栏头部）保持双向同步
const autoRefresh = defineModel("autoRefresh", {
  type: Boolean,
  default: false
})

const ports = ref([])
const loading = ref(false)
const loadError = ref("")
const keyword = ref("")
const protocolFilter = ref("all")
const lastUpdatedAt = ref("")
const terminatingPid = ref(0)
const sortField = ref("")
const sortOrder = ref("asc")
let refreshTimer = null

const processCount = computed(
  () => new Set(ports.value.map((port) => port.pid).filter(Boolean)).size
)

const protocolOptions = computed(() => [
  { value: "all", label: "全部", count: ports.value.length },
  {
    value: "TCP",
    label: "TCP",
    count: ports.value.filter((port) => port.protocol === "TCP").length
  },
  {
    value: "UDP",
    label: "UDP",
    count: ports.value.filter((port) => port.protocol === "UDP").length
  }
])

function toggleSort(field) {
  if (sortField.value === field) {
    if (sortOrder.value === "asc") {
      sortOrder.value = "desc"
    } else {
      sortField.value = ""
      sortOrder.value = "asc"
    }
  } else {
    sortField.value = field
    sortOrder.value = "asc"
  }
}

const filteredPorts = computed(() => {
  const normalizedKeyword = keyword.value.toLowerCase()

  let result = ports.value.filter((port) => {
    if (
      protocolFilter.value !== "all" &&
      port.protocol !== protocolFilter.value
    ) {
      return false
    }
    if (!normalizedKeyword) {
      return true
    }

    return [
      port.localAddress,
      port.localPort,
      port.pid,
      port.processName,
      port.executablePath,
      ...(port.serviceNames || [])
    ]
      .join(" ")
      .toLowerCase()
      .includes(normalizedKeyword)
  })

  if (sortField.value) {
    result = [...result].sort((a, b) => {
      let comparison = 0
      if (sortField.value === "process") {
        comparison = (a.processName || "").localeCompare(b.processName || "")
      } else if (sortField.value === "port") {
        comparison = Number(a.localPort || 0) - Number(b.localPort || 0)
      } else if (sortField.value === "protocol") {
        comparison = (a.protocol || "").localeCompare(b.protocol || "")
      } else if (sortField.value === "pid") {
        comparison = Number(a.pid || 0) - Number(b.pid || 0)
      }
      return sortOrder.value === "asc" ? comparison : -comparison
    })
  }

  return result
})

function getProcessVisual(port) {
  const name = String(port.processName || "").toLowerCase()
  if (name === "system" || port.pid === 4) {
    return { type: "windows" }
  }
  if (name === "svchost") {
    const isNetwork =
      port.localPort === 1900 ||
      port.serviceNames?.some((s) => {
        const lower = s.toLowerCase()
        return lower.includes("ssdp") || lower.includes("discovery")
      })
    return { type: isNetwork ? "network" : "service" }
  }
  if (!port.canTerminate) {
    return { type: "shield" }
  }
  if (
    ["node", "python", "python3", "pwsh", "powershell", "cmd", "bash", "java", "go"].some(
      (term) => name.includes(term)
    )
  ) {
    return { type: "terminal" }
  }
  return { type: "app" }
}

async function loadPorts() {
  if (loading.value || terminatingPid.value) {
    return
  }

  loading.value = true
  loadError.value = ""

  try {
    const result = await toolboxApi.listPorts()
    ports.value = result.ports || []
    lastUpdatedAt.value = new Intl.DateTimeFormat("zh-CN", {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false
    }).format(new Date())
  } catch (error) {
    loadError.value = error.message || String(error)
    createMessage.error(loadError.value)
  } finally {
    loading.value = false
  }
}

async function terminateProcess(port) {
  const processLabel = port.processName || `PID ${port.pid}`
  const shouldTerminate = window.confirm(
    `关闭进程「${processLabel}」(PID ${port.pid})？这会同时停止其子进程，未保存的数据可能丢失。`
  )

  if (!shouldTerminate) {
    return
  }

  terminatingPid.value = port.pid

  try {
    await toolboxApi.terminatePortProcess({
      pid: port.pid,
      startedAt: port.startedAt
    })
    createMessage.success(`进程 ${port.pid} 已关闭。`)
    terminatingPid.value = 0
    await loadPorts()
  } catch (error) {
    createMessage.error(error.message || String(error))
  } finally {
    terminatingPid.value = 0
  }
}

function formatAddress(address) {
  if (address === "0.0.0.0") {
    return "所有 IPv4"
  }
  if (address === "::") {
    return "所有 IPv6"
  }
  if (address === "127.0.0.1" || address === "::1") {
    return "仅本机"
  }
  return address
}

watch(autoRefresh, (enabled) => {
  if (refreshTimer) {
    window.clearInterval(refreshTimer)
    refreshTimer = null
  }
  if (enabled) {
    refreshTimer = window.setInterval(loadPorts, 8000)
  }
})

onMounted(loadPorts)
onUnmounted(() => {
  if (refreshTimer) {
    window.clearInterval(refreshTimer)
  }
})

defineExpose({
  loadPorts,
  loading,
  terminating: computed(() => Boolean(terminatingPid.value))
})
</script>

<style scoped lang="less">
.port-monitor {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;
  color: var(--color-text);
  font-size: var(--font-size-base);

  /* 顶部概览与筛选卡片 */
  .port-monitor-toolbar-card {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 18px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);

    .port-overview-block {
      display: flex;
      align-items: center;
      gap: 12px;
      flex-shrink: 0;

      .port-overview-icon-box {
        display: grid;
        width: 40px;
        height: 40px;
        place-items: center;
        border-radius: 10px;
        background: #eff6ff;
        color: #2563eb;
        border: 1px solid #dbeafe;
        flex-shrink: 0;
      }

      .port-overview-info {
        display: flex;
        flex-direction: column;
        gap: 2px;

        .port-overview-label {
          font-size: 11.5px;
          color: var(--color-text-muted);
          font-weight: 500;
        }

        .port-overview-stats {
          display: flex;
          align-items: baseline;
          gap: 6px;

          .stat-group {
            display: inline-flex;
            align-items: baseline;
            gap: 4px;

            .stat-num {
              font-size: 18px;
              font-weight: 700;
              color: var(--color-text);
              line-height: 1;

              &--blue {
                color: #2563eb;
              }
            }

            .stat-unit {
              font-size: 13px;
              color: var(--color-text);
              font-weight: 500;
            }
          }

          .stat-divider {
            color: var(--color-line-strong);
            margin: 0 4px;
            font-size: 13px;
          }
        }
      }
    }

    .port-search-wrapper {
      position: relative;
      display: flex;
      align-items: center;
      width: 300px;
      height: 36px;
      flex-shrink: 1;

      .port-search-icon {
        position: absolute;
        left: 12px;
        color: var(--color-text-soft);
        pointer-events: none;
      }

      .port-search-input {
        width: 100%;
        height: 100%;
        padding: 0 32px 0 34px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel-soft);
        color: var(--color-text);
        font-size: 13px;
        outline: none;
        transition: all 0.2s ease;

        &::placeholder {
          color: var(--color-text-soft);
        }

        &:focus {
          border-color: #2563eb;
          background: var(--color-panel);
          box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.1);
        }
      }

      .port-search-clear-btn {
        position: absolute;
        right: 8px;
        display: grid;
        place-items: center;
        width: 18px;
        height: 18px;
        border-radius: 50%;
        border: 0;
        background: var(--color-line);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.2s;

        &:hover {
          background: var(--color-line-strong);
          color: var(--color-text);
        }
      }
    }

    .port-protocol-tabs {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      flex-shrink: 0;

      .protocol-tab-btn {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        height: 32px;
        padding: 0 14px;
        border-radius: 8px;
        border: 1px solid transparent;
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.2s ease;

        .protocol-tab-count {
          color: #2563eb;
          font-weight: 600;
          font-size: 12.5px;
        }

        &:hover:not(.active) {
          border-color: var(--color-line);
          background: var(--color-panel);
          color: var(--color-text);
        }

        &.active {
          background: #2563eb;
          color: #ffffff;
          font-weight: 600;
          box-shadow: 0 1px 3px rgba(37, 99, 235, 0.25);

          .protocol-tab-count {
            color: #ffffff;
          }
        }
      }
    }

    .port-updated-text {
      font-size: 12px;
      color: var(--color-text-soft);
      font-family:
        ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      white-space: nowrap;
      margin-left: auto;
      flex-shrink: 0;
    }
  }

  /* 表格卡片容器 */
  .port-monitor-table-shell {
    display: flex;
    min-height: 0;
    flex: 1;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);

    .port-monitor-state {
      display: flex;
      min-height: 240px;
      flex: 1;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 10px;
      padding: 28px;
      color: var(--color-text-soft);

      .port-monitor-state-title {
        color: var(--color-text);
        font-size: var(--font-size-lg);
        font-weight: 600;
      }

      .port-monitor-state-text {
        max-width: 560px;
        color: var(--color-text-muted);
        font-size: var(--font-size-base);
        line-height: 1.55;
        text-align: center;
      }

      .port-monitor-retry-button {
        height: 32px;
        margin-top: 4px;
        padding: 0 16px;
        border: 0;
        border-radius: 6px;
        background: #2563eb;
        color: #ffffff;
        cursor: pointer;
        font-size: 13px;
        font-weight: 500;
        transition: background 0.2s;

        &:hover {
          background: #1d4ed8;
        }
      }

      &.error {
        color: var(--color-danger);
      }
    }

    .port-monitor-table-scroll {
      min-width: 0;
      flex: 1;
      overflow: auto;

      .port-monitor-table {
        width: 100%;
        min-width: 920px;
        border-collapse: collapse;
        table-layout: fixed;

        .col-program {
          width: 32%;
        }

        .col-endpoint {
          width: 17%;
        }

        .col-protocol {
          width: 10%;
        }

        .col-pid {
          width: 10%;
        }

        .col-path {
          width: auto;
        }

        .col-action {
          width: 68px;
        }

        .port-monitor-table-head {
          position: sticky;
          top: 0;
          z-index: 2;
          background: #f8fafc;

          .table-heading {
            height: 42px;
            padding: 0 16px;
            border-bottom: 1px solid var(--color-line);
            color: #475569;
            font-size: 13px;
            font-weight: 600;
            text-align: left;
            white-space: nowrap;
            user-select: none;

            &.sortable {
              cursor: pointer;

              &:hover {
                background: #f1f5f9;
                color: #0f172a;
              }
            }

            .heading-content {
              display: inline-flex;
              align-items: center;
              gap: 5px;

              .sort-icon {
                color: #94a3b8;
                transition: color 0.2s;

                &.active {
                  color: #2563eb;
                }
              }
            }

            &.action {
              text-align: center;
            }
          }
        }

        .port-monitor-table-body {
          .port-table-row {
            height: 62px;
            border-bottom: 1px solid #f1f5f9;
            transition: background-color 0.15s ease;

            &:hover {
              background-color: #f8fafc;
            }

            &:last-child {
              border-bottom: 0;
            }

            .table-cell {
              padding: 8px 16px;
              color: var(--color-text-muted);
              font-size: 13px;
              vertical-align: middle;

              &.cell-program {
                .process-flex {
                  display: flex;
                  align-items: center;
                  gap: 12px;
                  min-width: 0;

                  .process-icon-box {
                    display: grid;
                    width: 34px;
                    height: 34px;
                    flex-shrink: 0;
                    place-items: center;
                    border-radius: 8px;

                    &.windows {
                      background: #eff6ff;
                      color: #0284c7;
                    }

                    &.service {
                      background: #eff6ff;
                      color: #2563eb;
                    }

                    &.network {
                      background: #ecfdf5;
                      color: #10b981;
                    }

                    &.shield {
                      background: #eff6ff;
                      color: #2563eb;
                    }

                    &.terminal {
                      background: #f0f9ff;
                      color: #0284c7;
                    }

                    &.app {
                      background: #f1f5f9;
                      color: #475569;
                    }
                  }

                  .process-meta {
                    display: flex;
                    flex-direction: column;
                    gap: 3px;
                    min-width: 0;

                    .process-name {
                      font-size: 13.5px;
                      font-weight: 700;
                      color: #0f172a;
                      overflow: hidden;
                      text-overflow: ellipsis;
                      white-space: nowrap;
                    }

                    .process-sub {
                      font-size: 12px;
                      color: #64748b;
                      overflow: hidden;
                      text-overflow: ellipsis;
                      white-space: nowrap;
                      max-width: 320px;
                    }
                  }
                }
              }

              &.cell-endpoint {
                .endpoint-flex {
                  display: flex;
                  flex-direction: column;
                  gap: 3px;
                  min-width: 0;

                  .endpoint-port {
                    font-size: 14px;
                    font-weight: 700;
                    color: #0f172a;
                    font-family:
                      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                      monospace;
                  }

                  .endpoint-address {
                    font-size: 12px;
                    color: #64748b;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                    max-width: 200px;
                  }
                }
              }

              &.cell-protocol {
                .protocol-pill {
                  display: inline-flex;
                  align-items: center;
                  justify-content: center;
                  height: 24px;
                  min-width: 48px;
                  padding: 0 10px;
                  border-radius: 6px;
                  font-size: 11.5px;
                  font-weight: 700;
                  letter-spacing: 0.02em;

                  &.tcp {
                    background: #eff6ff;
                    color: #2563eb;
                    border: 1px solid #dbeafe;
                  }

                  &.udp {
                    background: #fef3c7;
                    color: #d97706;
                    border: 1px solid #fde68a;
                  }
                }
              }

              &.cell-pid {
                .pid-text {
                  font-size: 13.5px;
                  font-weight: 600;
                  color: #0f172a;
                  font-family:
                    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                    monospace;
                }
              }

              &.cell-path {
                .path-text {
                  font-size: 12.5px;
                  color: #64748b;
                  overflow: hidden;
                  text-overflow: ellipsis;
                  white-space: nowrap;
                  display: block;
                  max-width: 260px;
                  font-family:
                    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                    monospace;

                  &.is-empty {
                    color: #94a3b8;
                    font-family: inherit;
                  }
                }
              }

              &.cell-action {
                text-align: center;

                .stop-proc-btn {
                  display: inline-flex;
                  width: 32px;
                  height: 32px;
                  align-items: center;
                  justify-content: center;
                  border: 1px solid #cbd5e1;
                  border-radius: 50%;
                  background: #ffffff;
                  color: #475569;
                  cursor: pointer;
                  transition: all 0.2s ease;

                  &:hover:not(:disabled) {
                    border-color: #ef4444;
                    color: #ef4444;
                    background: #fef2f2;
                    transform: scale(1.05);
                  }

                  &:disabled {
                    border-color: #e2e8f0;
                    background: #f8fafc;
                    color: #94a3b8;
                    opacity: 0.45;
                    cursor: not-allowed;
                  }
                }
              }
            }
          }
        }
      }
    }
  }

  /* 暗色模式适配 */
  :global(:root[data-theme="dark"]),
  .tools-view--dark & {
    .port-monitor-toolbar-card {
      background: var(--color-panel);
      border-color: var(--color-line);

      .port-overview-block {
        .port-overview-icon-box {
          background: rgba(37, 99, 235, 0.16);
          border-color: rgba(96, 165, 250, 0.3);
          color: #60a5fa;
        }

        .port-overview-info .port-overview-stats .stat-group {
          .stat-num {
            color: #f1f5f9;

            &--blue {
              color: #60a5fa;
            }
          }
          .stat-unit {
            color: #cbd5e1;
          }
        }
      }

      .port-search-wrapper {
        .port-search-input {
          background: var(--color-panel-soft);
          border-color: var(--color-line);
          color: var(--color-text);

          &:focus {
            border-color: #60a5fa;
            background: var(--color-panel);
          }
        }

        .port-search-clear-btn {
          background: rgba(255, 255, 255, 0.1);
          color: #94a3b8;

          &:hover {
            background: rgba(255, 255, 255, 0.2);
            color: #ffffff;
          }
        }
      }

      .port-protocol-tabs .protocol-tab-btn {
        background: var(--color-panel-soft);
        color: var(--color-text-muted);

        .protocol-tab-count {
          color: #60a5fa;
        }

        &:hover:not(.active) {
          border-color: var(--color-line-strong);
          color: var(--color-text);
        }

        &.active {
          background: #2563eb;
          color: #ffffff;

          .protocol-tab-count {
            color: #ffffff;
          }
        }
      }
    }

    .port-monitor-table-shell {
      background: var(--color-panel);
      border-color: var(--color-line);

      .port-monitor-table-scroll .port-monitor-table {
        .port-monitor-table-head {
          background: var(--color-panel-soft);

          .table-heading {
            border-bottom-color: var(--color-line);
            color: #94a3b8;

            &.sortable:hover {
              background: rgba(255, 255, 255, 0.04);
              color: #ffffff;
            }
          }
        }

        .port-monitor-table-body .port-table-row {
          border-bottom-color: var(--color-line);

          &:hover {
            background-color: rgba(255, 255, 255, 0.02);
          }

          .table-cell {
            &.cell-program .process-flex {
              .process-icon-box {
                &.windows,
                &.service,
                &.shield {
                  background: rgba(37, 99, 235, 0.18);
                  color: #60a5fa;
                }

                &.network {
                  background: rgba(16, 185, 129, 0.18);
                  color: #34d399;
                }

                &.terminal {
                  background: rgba(2, 132, 199, 0.18);
                  color: #38bdf8;
                }

                &.app {
                  background: rgba(255, 255, 255, 0.06);
                  color: #94a3b8;
                }
              }

              .process-meta {
                .process-name {
                  color: #f1f5f9;
                }

                .process-sub {
                  color: #94a3b8;
                }
              }
            }

            &.cell-endpoint .endpoint-flex {
              .endpoint-port {
                color: #f1f5f9;
              }

              .endpoint-address {
                color: #94a3b8;
              }
            }

            &.cell-protocol .protocol-pill {
              &.tcp {
                background: rgba(37, 99, 235, 0.18);
                color: #60a5fa;
                border-color: rgba(96, 165, 250, 0.3);
              }

              &.udp {
                background: rgba(245, 158, 11, 0.18);
                color: #fbbf24;
                border-color: rgba(245, 158, 11, 0.3);
              }
            }

            &.cell-pid .pid-text {
              color: #f1f5f9;
            }

            &.cell-path .path-text {
              color: #94a3b8;
            }

            &.cell-action .stop-proc-btn {
              background: var(--color-panel-soft);
              border-color: var(--color-line);
              color: var(--color-text-muted);

              &:hover:not(:disabled) {
                border-color: #ef4444;
                background: rgba(239, 68, 68, 0.15);
                color: #f87171;
              }

              &:disabled {
                border-color: var(--color-line);
                background: transparent;
                opacity: 0.3;
              }
            }
          }
        }
      }
    }
  }

  .spinning {
    animation: port-monitor-spin 0.8s linear infinite;
  }
}

@keyframes port-monitor-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
