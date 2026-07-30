<template>
  <section class="port-monitor">
    <header class="port-monitor-head">
      <div class="port-monitor-title">
        <p class="port-monitor-mark">Local Ports</p>
        <div class="port-monitor-title-row">
          <h2 class="port-monitor-title-text">端口监测</h2>
          <span class="port-monitor-summary">
            {{ ports.length }} 个端口 · {{ processCount }} 个程序
          </span>
        </div>
      </div>

      <div class="port-monitor-head-actions">
        <label class="port-monitor-auto-refresh" title="每 8 秒自动刷新">
          <input
            v-model="autoRefresh"
            class="port-monitor-auto-refresh-input"
            type="checkbox"
          />
          <span class="port-monitor-auto-refresh-track">
            <span class="port-monitor-auto-refresh-thumb"></span>
          </span>
          <span class="port-monitor-auto-refresh-label">自动刷新</span>
        </label>
        <button
          class="port-monitor-icon-button"
          type="button"
          title="刷新端口列表"
          :disabled="loading || Boolean(terminatingPid)"
          @click="loadPorts"
        >
          <RefreshCw :class="{ spinning: loading }" :size="16" />
        </button>
      </div>
    </header>

    <section class="port-monitor-toolbar">
      <label class="port-monitor-search">
        <Search class="port-monitor-search-icon" :size="15" />
        <input
          v-model.trim="keyword"
          class="port-monitor-search-input"
          type="search"
          placeholder="搜索端口、程序、服务或 PID"
        />
        <button
          v-if="keyword"
          class="port-monitor-search-clear"
          type="button"
          title="清空搜索"
          @click="keyword = ''"
        >
          <X :size="14" />
        </button>
      </label>

      <div class="port-monitor-protocols" role="group" aria-label="端口协议筛选">
        <button
          v-for="option in protocolOptions"
          :key="option.value"
          :class="[
            'port-monitor-protocol-button',
            { active: protocolFilter === option.value }
          ]"
          type="button"
          @click="protocolFilter = option.value"
        >
          {{ option.label }}
          <span class="port-monitor-protocol-count">{{ option.count }}</span>
        </button>
      </div>

      <span class="port-monitor-updated-at">
        <Clock3 :size="13" />
        {{ lastUpdatedAt || '尚未刷新' }}
      </span>
    </section>

    <section class="port-monitor-table-shell">
      <div v-if="loading && !ports.length" class="port-monitor-state">
        <RefreshCw class="spinning" :size="20" />
        <span class="port-monitor-state-text">正在读取本机端口...</span>
      </div>

      <div v-else-if="loadError && !ports.length" class="port-monitor-state error">
        <TriangleAlert :size="21" />
        <strong class="port-monitor-state-title">端口读取失败</strong>
        <span class="port-monitor-state-text">{{ loadError }}</span>
        <button class="port-monitor-retry-button" type="button" @click="loadPorts">
          重新读取
        </button>
      </div>

      <div v-else-if="!filteredPorts.length" class="port-monitor-state">
        <Network :size="23" />
        <strong class="port-monitor-state-title">
          {{ ports.length ? '没有匹配的端口' : '未检测到监听端口' }}
        </strong>
      </div>

      <div v-else class="port-monitor-table-scroll">
        <table class="port-monitor-table">
          <colgroup>
            <col class="port-monitor-program-column" />
            <col class="port-monitor-port-column" />
            <col class="port-monitor-protocol-column" />
            <col class="port-monitor-pid-column" />
            <col class="port-monitor-path-column" />
            <col class="port-monitor-action-column" />
          </colgroup>
          <thead class="port-monitor-table-head">
            <tr class="port-monitor-table-row">
              <th class="port-monitor-table-heading">程序 / 服务</th>
              <th class="port-monitor-table-heading">端口 / 地址</th>
              <th class="port-monitor-table-heading">协议</th>
              <th class="port-monitor-table-heading">PID</th>
              <th class="port-monitor-table-heading">可执行文件</th>
              <th class="port-monitor-table-heading action">操作</th>
            </tr>
          </thead>
          <tbody class="port-monitor-table-body">
            <tr
              v-for="(port, portIndex) in filteredPorts"
              :key="`${port.id}-${portIndex}`"
              class="port-monitor-table-row"
            >
              <td class="port-monitor-table-cell">
                <div class="port-monitor-process">
                  <span
                    :class="[
                      'port-monitor-process-icon',
                      { protected: !port.canTerminate }
                    ]"
                  >
                    <ShieldCheck v-if="!port.canTerminate" :size="16" />
                    <AppWindow v-else :size="16" />
                  </span>
                  <span class="port-monitor-process-main">
                    <strong class="port-monitor-process-name">
                      {{ port.processName || '未知进程' }}
                    </strong>
                    <span
                      v-if="port.serviceNames?.length"
                      class="port-monitor-service-name"
                      :title="port.serviceNames.join('、')"
                    >
                      {{ port.serviceNames.join('、') }}
                    </span>
                    <span
                      v-else-if="port.protectedReason"
                      class="port-monitor-protected-reason"
                    >
                      {{ port.protectedReason }}
                    </span>
                  </span>
                </div>
              </td>
              <td class="port-monitor-table-cell">
                <div class="port-monitor-endpoint">
                  <strong class="port-monitor-port">{{ port.localPort }}</strong>
                  <span class="port-monitor-address" :title="port.localAddress">
                    {{ formatAddress(port.localAddress) }}
                  </span>
                </div>
              </td>
              <td class="port-monitor-table-cell">
                <span :class="['port-monitor-protocol', port.protocol.toLowerCase()]">
                  {{ port.protocol }}
                </span>
              </td>
              <td class="port-monitor-table-cell">
                <span class="port-monitor-pid">{{ port.pid }}</span>
              </td>
              <td class="port-monitor-table-cell">
                <span
                  class="port-monitor-path"
                  :title="port.executablePath || '当前权限无法读取可执行文件路径'"
                >
                  {{ port.executablePath || '路径不可用' }}
                </span>
              </td>
              <td class="port-monitor-table-cell action">
                <button
                  class="port-monitor-stop-button"
                  type="button"
                  :title="port.canTerminate ? '关闭进程' : port.protectedReason"
                  :disabled="!port.canTerminate || Boolean(terminatingPid)"
                  @click="terminateProcess(port)"
                >
                  <LoaderCircle
                    v-if="terminatingPid === port.pid"
                    class="spinning"
                    :size="15"
                  />
                  <CircleStop v-else :size="15" />
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
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import {
  AppWindow,
  CircleStop,
  Clock3,
  LoaderCircle,
  Network,
  RefreshCw,
  Search,
  ShieldCheck,
  TriangleAlert,
  X
} from 'lucide-vue-next'
import { toolboxApi } from '@/api'
import { createMessage } from '@/utils/message'

const ports = ref([])
const loading = ref(false)
const loadError = ref('')
const keyword = ref('')
const protocolFilter = ref('all')
const autoRefresh = ref(false)
const lastUpdatedAt = ref('')
const terminatingPid = ref(0)
let refreshTimer = null

const processCount = computed(
  () => new Set(ports.value.map(port => port.pid).filter(Boolean)).size
)

const protocolOptions = computed(() => [
  { value: 'all', label: '全部', count: ports.value.length },
  {
    value: 'TCP',
    label: 'TCP',
    count: ports.value.filter(port => port.protocol === 'TCP').length
  },
  {
    value: 'UDP',
    label: 'UDP',
    count: ports.value.filter(port => port.protocol === 'UDP').length
  }
])

const filteredPorts = computed(() => {
  const normalizedKeyword = keyword.value.toLowerCase()

  return ports.value.filter(port => {
    if (
      protocolFilter.value !== 'all' &&
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
      .join(' ')
      .toLowerCase()
      .includes(normalizedKeyword)
  })
})

// 后端返回的是端口快照，刷新时整批替换以避免残留已退出进程。
async function loadPorts() {
  if (loading.value || terminatingPid.value) {
    return
  }

  loading.value = true
  loadError.value = ''

  try {
    const result = await toolboxApi.listPorts()
    ports.value = result.ports || []
    lastUpdatedAt.value = new Intl.DateTimeFormat('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
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
  if (address === '0.0.0.0') {
    return '所有 IPv4'
  }
  if (address === '::') {
    return '所有 IPv6'
  }
  if (address === '127.0.0.1' || address === '::1') {
    return '仅本机'
  }

  return address
}

// 自动刷新只在组件存活期间运行，离开工具页后立即释放定时器。
watch(autoRefresh, enabled => {
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
</script>

<style scoped lang="less">
.port-monitor {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;

  .port-monitor-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 2px 2px 0;

    .port-monitor-title {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 4px;

      .port-monitor-mark {
        margin: 0;
        color: var(--color-text-soft);
        font-size: 0.7rem;
        font-weight: 700;
        letter-spacing: 0;
        text-transform: uppercase;
      }

      .port-monitor-title-row {
        display: flex;
        align-items: baseline;
        gap: 10px;

        .port-monitor-title-text {
          margin: 0;
          color: var(--color-text);
          font-size: 1.08rem;
          line-height: 1.25;
        }

        .port-monitor-summary {
          color: var(--color-text-muted);
          font-size: 0.77rem;
          font-weight: 700;
        }
      }
    }

    .port-monitor-head-actions {
      display: flex;
      flex: none;
      align-items: center;
      gap: 10px;

      .port-monitor-auto-refresh {
        display: inline-flex;
        align-items: center;
        gap: 7px;
        color: var(--color-text-muted);
        cursor: pointer;
        font-size: 0.76rem;
        font-weight: 700;

        .port-monitor-auto-refresh-input {
          position: absolute;
          width: 1px;
          height: 1px;
          overflow: hidden;
          opacity: 0;
        }

        .port-monitor-auto-refresh-track {
          display: flex;
          width: 30px;
          height: 18px;
          align-items: center;
          padding: 2px;
          border: 1px solid #c9d5df;
          border-radius: 999px;
          background: #e8edf2;
          transition:
            border-color 0.18s ease,
            background-color 0.18s ease;

          .port-monitor-auto-refresh-thumb {
            width: 12px;
            height: 12px;
            border-radius: 50%;
            background: #ffffff;
            box-shadow: 0 1px 3px rgba(34, 56, 83, 0.26);
            transition: transform 0.18s ease;
          }
        }

        .port-monitor-auto-refresh-input:checked + .port-monitor-auto-refresh-track {
          border-color: var(--color-primary);
          background: var(--color-primary);

          .port-monitor-auto-refresh-thumb {
            transform: translateX(12px);
          }
        }

        .port-monitor-auto-refresh-input:focus-visible + .port-monitor-auto-refresh-track {
          outline: 2px solid rgba(47, 95, 145, 0.22);
          outline-offset: 2px;
        }
      }

      .port-monitor-icon-button {
        display: inline-flex;
        width: 34px;
        height: 34px;
        align-items: center;
        justify-content: center;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: #ffffff;
        color: var(--color-primary);
        cursor: pointer;
      }

      .port-monitor-icon-button:hover:not(:disabled) {
        border-color: #b9ccda;
        background: #f7f9fc;
      }

      .port-monitor-icon-button:disabled {
        cursor: not-allowed;
        opacity: 0.5;
      }
    }
  }

  .port-monitor-toolbar {
    display: flex;
    flex: none;
    align-items: center;
    gap: 10px;
    min-height: 38px;
    padding: 6px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);

    .port-monitor-search {
      position: relative;
      display: flex;
      width: 320px;
      height: 32px;
      flex: none;
      align-items: center;

      .port-monitor-search-icon {
        position: absolute;
        left: 10px;
        color: var(--color-text-soft);
        pointer-events: none;
      }

      .port-monitor-search-input {
        width: 100%;
        height: 100%;
        padding: 0 34px 0 32px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: #ffffff;
        color: var(--color-text);
        font-size: 0.78rem;
      }

      .port-monitor-search-input::placeholder {
        color: var(--color-text-soft);
      }

      .port-monitor-search-clear {
        position: absolute;
        right: 6px;
        display: inline-flex;
        width: 24px;
        height: 24px;
        align-items: center;
        justify-content: center;
        border: 0;
        border-radius: 5px;
        background: transparent;
        color: var(--color-text-soft);
        cursor: pointer;
      }

      .port-monitor-search-clear:hover {
        background: #edf1f5;
        color: var(--color-text);
      }
    }

    .port-monitor-protocols {
      display: inline-flex;
      flex: none;
      align-items: center;
      padding: 2px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: #ffffff;

      .port-monitor-protocol-button {
        display: inline-flex;
        height: 27px;
        align-items: center;
        gap: 5px;
        padding: 0 9px;
        border: 0;
        border-radius: 5px;
        background: transparent;
        color: var(--color-text-muted);
        cursor: pointer;
        font-size: 0.72rem;
        font-weight: 700;

        .port-monitor-protocol-count {
          color: var(--color-text-soft);
          font-size: 0.68rem;
        }
      }

      .port-monitor-protocol-button.active {
        background: var(--color-primary-soft);
        color: var(--color-primary);

        .port-monitor-protocol-count {
          color: var(--color-primary);
        }
      }
    }

    .port-monitor-updated-at {
      display: inline-flex;
      margin-left: auto;
      align-items: center;
      gap: 5px;
      padding-right: 7px;
      color: var(--color-text-soft);
      font-size: 0.71rem;
      white-space: nowrap;
    }
  }

  .port-monitor-table-shell {
    display: flex;
    min-height: 0;
    flex: 1;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;

    .port-monitor-state {
      display: flex;
      min-height: 220px;
      flex: 1;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 8px;
      padding: 28px;
      color: var(--color-text-soft);

      .port-monitor-state-title {
        color: var(--color-text);
        font-size: 0.88rem;
      }

      .port-monitor-state-text {
        max-width: 560px;
        color: var(--color-text-muted);
        font-size: 0.78rem;
        line-height: 1.55;
        text-align: center;
      }

      .port-monitor-retry-button {
        height: 31px;
        margin-top: 4px;
        padding: 0 12px;
        border: 1px solid var(--color-primary);
        border-radius: 6px;
        background: var(--color-primary);
        color: #ffffff;
        cursor: pointer;
        font-size: 0.76rem;
        font-weight: 700;
      }
    }

    .port-monitor-state.error {
      color: var(--color-danger);
    }

    .port-monitor-table-scroll {
      min-width: 0;
      flex: 1;
      overflow: auto;

      .port-monitor-table {
        width: 100%;
        min-width: 930px;
        border-collapse: collapse;
        table-layout: fixed;

        .port-monitor-program-column {
          width: auto;
        }

        .port-monitor-port-column {
          width: 16%;
        }

        .port-monitor-protocol-column {
          width: 9%;
        }

        .port-monitor-pid-column {
          width: 9%;
        }

        .port-monitor-path-column {
          width: 240px;
        }

        .port-monitor-action-column {
          width: 62px;
        }

        .port-monitor-table-head {
          position: sticky;
          top: 0;
          z-index: 1;
          background: #f7f9fb;

          .port-monitor-table-heading {
            height: 35px;
            padding: 0 12px;
            border-bottom: 1px solid var(--color-line);
            color: var(--color-text-soft);
            font-size: 0.69rem;
            font-weight: 700;
            text-align: left;
            white-space: nowrap;
          }

          .port-monitor-table-heading.action {
            text-align: center;
          }
        }

        .port-monitor-table-body {
          .port-monitor-table-row {
            transition: background-color 0.16s ease;

            .port-monitor-table-cell {
              height: 58px;
              padding: 8px 12px;
              border-bottom: 1px solid #edf1f4;
              color: var(--color-text-muted);
              font-size: 0.76rem;
              vertical-align: middle;

              .port-monitor-process {
                display: flex;
                min-width: 0;
                align-items: center;
                gap: 9px;

                .port-monitor-process-icon {
                  display: inline-flex;
                  width: 30px;
                  height: 30px;
                  flex: 0 0 30px;
                  align-items: center;
                  justify-content: center;
                  border-radius: 6px;
                  background: #eaf2f8;
                  color: #35688f;
                }

                .port-monitor-process-icon.protected {
                  background: #f0f2f4;
                  color: #718092;
                }

                .port-monitor-process-main {
                  display: flex;
                  min-width: 0;
                  flex-direction: column;
                  gap: 3px;

                  .port-monitor-process-name {
                    overflow: hidden;
                    color: var(--color-text);
                    font-size: 0.79rem;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                  }

                  .port-monitor-service-name,
                  .port-monitor-protected-reason {
                    overflow: hidden;
                    color: var(--color-text-soft);
                    font-size: 0.68rem;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                  }
                }
              }

              .port-monitor-endpoint {
                display: flex;
                min-width: 0;
                flex-direction: column;
                gap: 3px;

                .port-monitor-port {
                  color: var(--color-text);
                  font-family: Consolas, "SFMono-Regular", monospace;
                  font-size: 0.83rem;
                }

                .port-monitor-address {
                  overflow: hidden;
                  color: var(--color-text-soft);
                  font-size: 0.68rem;
                  text-overflow: ellipsis;
                  white-space: nowrap;
                }
              }

              .port-monitor-protocol {
                display: inline-flex;
                min-width: 42px;
                height: 22px;
                align-items: center;
                justify-content: center;
                border: 1px solid #c9dbe8;
                border-radius: 5px;
                background: #eef6fb;
                color: #2d668f;
                font-size: 0.67rem;
                font-weight: 800;
              }

              .port-monitor-protocol.udp {
                border-color: #d8d3aa;
                background: #faf8e8;
                color: #776b21;
              }

              .port-monitor-pid {
                font-family: Consolas, "SFMono-Regular", monospace;
                color: var(--color-text);
                font-size: 0.75rem;
              }

              .port-monitor-path {
                display: block;
                width: 100%;
                max-width: 216px;
                overflow: hidden;
                color: var(--color-text-muted);
                font-family: Consolas, "SFMono-Regular", monospace;
                font-size: 0.69rem;
                text-overflow: ellipsis;
                white-space: nowrap;
              }

              .port-monitor-stop-button {
                display: inline-flex;
                width: 30px;
                height: 30px;
                align-items: center;
                justify-content: center;
                border: 1px solid #e7c8c5;
                border-radius: 6px;
                background: #fff8f7;
                color: var(--color-danger);
                cursor: pointer;
              }

              .port-monitor-stop-button:hover:not(:disabled) {
                border-color: #d89e98;
                background: var(--color-danger-soft);
              }

              .port-monitor-stop-button:disabled {
                border-color: var(--color-line);
                background: #f4f6f8;
                color: #a7b1bc;
                cursor: not-allowed;
              }
            }

            .port-monitor-table-cell.action {
              text-align: center;
            }
          }

          .port-monitor-table-row:hover {
            background: #fafcfd;
          }

          .port-monitor-table-row:last-child {
            .port-monitor-table-cell {
              border-bottom: 0;
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
