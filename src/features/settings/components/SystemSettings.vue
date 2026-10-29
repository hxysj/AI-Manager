<template>
  <section class="system-settings" aria-label="系统设置">
    <template v-for="item in settingItems" :key="item.key">
      <article
        v-if="item.kind !== 'tools' || draft.quickSwitchVisible"
        class="system-settings-card"
      >
        <div class="system-settings-card-row">
          <div class="system-settings-copy">
            <div class="system-settings-icon">
              <component :is="item.icon" :size="24" />
            </div>
            <div class="system-settings-text">
              <h3 class="system-settings-title">{{ item.title }}</h3>
              <p class="system-settings-description">{{ item.description }}</p>
              <div
                v-if="item.key === 'logEnabled'"
                class="system-settings-log-storage"
              >
                <span aria-live="polite">
                  当前占用：{{ logStorageText }}
                  <template v-if="logStats"
                    >（{{ logStats.fileCount }} 个日期文件）</template
                  >
                </span>
                <button
                  class="system-settings-log-refresh"
                  type="button"
                  aria-label="刷新日志存储占用"
                  @click="loadLogStats"
                >
                  <RefreshCw :size="13" />
                </button>
              </div>
            </div>
          </div>
          <div v-if="item.kind !== 'tools'" class="system-settings-controls">
            <button
              v-if="item.kind === 'switch'"
              class="system-settings-switch"
              :class="{ 'system-settings-switch--active': draft[item.key] }"
              type="button"
              role="switch"
              :name="item.name"
              :aria-label="item.title"
              :aria-checked="draft[item.key]"
              :disabled="pending"
              @click="saveSetting(item.key, !draft[item.key])"
            >
              <span class="system-settings-switch-thumb"></span>
            </button>
            <div
              v-else-if="item.kind === 'choices'"
              class="system-settings-choices"
              role="radiogroup"
              :aria-label="item.title"
            >
              <button
                v-for="choice in item.choices"
                :key="choice.value"
                class="system-settings-choice"
                :class="{
                  'system-settings-choice--active':
                    draft[item.key] === choice.value
                }"
                type="button"
                role="radio"
                :name="item.name"
                :value="choice.value"
                :aria-checked="draft[item.key] === choice.value"
                :disabled="pending"
                @click="saveSetting(item.key, choice.value)"
              >
                {{ choice.label }}
              </button>
            </div>
            <button
              v-else
              class="system-settings-danger-button"
              type="button"
              :disabled="pending"
              @click="$emit(item.key)"
            >
              <component :is="item.icon" :size="16" />
              {{ item.title }}
            </button>
          </div>
        </div>
        <div v-if="item.kind === 'tools'" class="system-settings-tools">
          <label
            v-for="tool in floatingToolItems"
            :key="tool.id"
            class="system-settings-tool"
          >
            <input
              class="system-settings-tool-checkbox"
              type="checkbox"
              :checked="draft.quickSwitchTools[tool.id] !== false"
              :disabled="pending"
              @change="
                saveSetting('quickSwitchTools', $event.target.checked, tool.id)
              "
            />
            <component :is="tool.icon" :size="16" />
            <span class="system-settings-tool-label">{{ tool.label }}</span>
          </label>
        </div>
      </article>
    </template>
  </section>
</template>

<script setup>
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref
} from "vue"
import {
  FileText,
  Monitor,
  PanelTopClose,
  Power,
  RefreshCw,
  Trash2,
  Wrench
} from "lucide-vue-next"
import { getToolItems } from "@/features/tools/constant"
import { appApi } from "@/api"

const props = defineProps({
  settings: { type: Object, default: () => ({}) },
  cliTargets: { type: Array, default: () => [] },
  pending: { type: Boolean, default: false },
  saveSettings: { type: Function, required: true }
})

defineEmits(["quit-app", "uninstall-without-trace"])

const settingItems = computed(() => [
  {
    key: "logEnabled",
    name: "log-enabled",
    kind: "switch",
    icon: FileText,
    title: "日志记录",
    description:
      "按日期记录调试信息，仅保留最近 7 天，过期自动彻底删除。关闭后停止新增日志。"
  },
  {
    key: "autoLaunchEnabled",
    name: "auto-launch-enabled",
    kind: "switch",
    icon: Power,
    title: "开机自动启动",
    description: "开启后登录系统时自动启动软件，可随时关闭。"
  },
  {
    key: "closeAction",
    name: "close-action",
    kind: "choices",
    icon: PanelTopClose,
    title: "关闭按钮行为",
    description: "点击窗口关闭按钮时，选择询问、最小化到托盘或直接关闭软件。",
    choices: [
      { value: "ask", label: "每次询问" },
      { value: "minimize", label: "最小化到托盘" },
      { value: "quit", label: "直接关闭" }
    ]
  },
  {
    key: "quickSwitchVisible",
    name: "quick-switch-visible",
    kind: "switch",
    icon: Monitor,
    title: "悬浮快速切换窗",
    description: "主界面最小化时显示中心节点，鼠标悬浮后展开四周的工具入口。"
  },
  {
    key: "quickSwitchTools",
    kind: "tools",
    icon: Wrench,
    title: "悬浮窗工具",
    description:
      "选择悬浮时显示的工具，默认全部显示。取消全部后只保留中心节点。"
  },
  {
    key: "quit-app",
    kind: "action",
    icon: Power,
    title: "关闭程序",
    description: "立即退出桌面端，不最小化到托盘。"
  },
  {
    key: "uninstall-without-trace",
    kind: "action",
    icon: Trash2,
    title: "无痕卸载",
    description: "移除本地应用数据并启动系统卸载流程。"
  }
])

const floatingToolItems = computed(() =>
  getToolItems({ cliTargets: props.cliTargets })
)
// 临时值只覆盖正在保存的字段，其他设置始终读取最新配置。
const overrides = reactive({ quickSwitchTools: {} })
const requests = new Map()
const draft = computed(() => ({
  logEnabled: true,
  autoLaunchEnabled: false,
  closeAction: "ask",
  quickSwitchVisible: true,
  ...props.settings,
  ...overrides,
  // 缺少配置的工具默认显示，旧配置和以后新增的工具都无需迁移。
  quickSwitchTools: {
    ...props.settings.quickSwitchTools,
    ...overrides.quickSwitchTools
  }
}))

const logStats = ref(null)
const logStatsError = ref(false)
let logStatsTimer = null
const logStorageText = computed(() => {
  if (logStatsError.value) return "读取失败，请刷新"
  if (!logStats.value) return "读取中…"
  const bytes = Number(logStats.value.bytes || 0)
  if (bytes < 1024) return `${bytes} B`
  const units = ["KB", "MB", "GB"]
  const index = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)) - 1,
    units.length - 1
  )
  return `${(bytes / 1024 ** (index + 1)).toFixed(2)} ${units[index]}`
})

// 统计读取不写日志；设置页打开时更新，关闭页面后停止轮询。
async function loadLogStats() {
  try {
    logStats.value = await appApi.getAppLogStats()
    logStatsError.value = false
  } catch {
    logStatsError.value = true
  }
}

onMounted(() => {
  void loadLogStats()
  logStatsTimer = window.setInterval(loadLogStats, 10_000)
})
onBeforeUnmount(() => window.clearInterval(logStatsTimer))

async function saveSetting(key, value, toolId = "") {
  const current = toolId
    ? draft.value.quickSwitchTools[toolId] !== false
    : draft.value[key]
  if (current === value) return true
  const values = toolId ? overrides.quickSwitchTools : overrides
  const field = toolId || key
  const requestKey = toolId ? `${key}.${toolId}` : key
  const request = Symbol()
  values[field] = value
  requests.set(requestKey, request)
  const success = await props.saveSettings({
    system: toolId ? { [key]: { [toolId]: value } } : { [key]: value }
  })
  await nextTick()
  // 仅清除当前请求的临时值，较早的响应不能覆盖后一次操作；失败时显示已保存值。
  if (requests.get(requestKey) === request) {
    delete values[field]
    requests.delete(requestKey)
  }
  if (key === "logEnabled") await loadLogStats()
  return success
}
</script>

<style scoped lang="less">
.system-settings {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);

  .system-settings-card {
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 22px 28px;
    border: 1px solid var(--color-line);
    border-radius: 16px;
    background: var(--color-panel);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);

    .system-settings-card-row {
      display: flex;
      align-items: center;
      gap: 24px;

      .system-settings-copy {
        display: flex;
        min-width: 0;
        flex: 1;
        align-items: center;
        gap: 18px;

        .system-settings-icon {
          display: flex;
          width: 52px;
          height: 52px;
          flex: none;
          align-items: center;
          justify-content: center;
          border-radius: 14px;
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }

        .system-settings-text {
          display: flex;
          min-width: 0;
          flex-direction: column;
          gap: 4px;

          .system-settings-title {
            margin: 0;
            color: var(--color-text);
            font-size: var(--font-size-base);
            line-height: 1.4;
          }

          .system-settings-description {
            margin: 0;
            color: var(--color-text-muted);
            font-size: 13px;
            line-height: 1.5;
          }

          .system-settings-log-storage {
            display: flex;
            align-items: center;
            gap: 8px;
            color: var(--color-text-muted);
            font-size: 12px;

            .system-settings-log-refresh {
              display: flex;
              align-items: center;
              justify-content: center;
              padding: 4px;
              border: 0;
              border-radius: 4px;
              background: transparent;
              color: var(--color-primary);
              cursor: pointer;
            }
          }
        }
      }

      .system-settings-controls {
        display: flex;
        min-height: 52px;
        flex: none;
        align-items: center;
        justify-content: flex-end;
        padding-left: 24px;
        border-left: 1px solid var(--color-line);

        .system-settings-switch {
          position: relative;
          width: 44px;
          height: 24px;
          padding: 2px;
          border: none;
          border-radius: 12px;
          background: #cbd5e1;
          cursor: pointer;
          transition: background 0.2s ease;

          .system-settings-switch-thumb {
            display: block;
            width: 20px;
            height: 20px;
            border-radius: 50%;
            background: #ffffff;
            box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
            transition: transform 0.2s ease;
          }

          &.system-settings-switch--active {
            background: var(--color-primary);

            .system-settings-switch-thumb {
              transform: translateX(20px);
            }
          }
        }

        .system-settings-choices {
          display: flex;
          flex-wrap: wrap;
          justify-content: flex-end;
          gap: 4px;
          padding: 4px;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel-soft);

          .system-settings-choice {
            padding: 6px 10px;
            border: none;
            border-radius: 6px;
            background: transparent;
            color: var(--color-text-muted);
            font-size: 13px;
            cursor: pointer;

            &.system-settings-choice--active {
              background: var(--color-primary-soft);
              color: var(--color-primary);
            }
          }
        }

        .system-settings-danger-button {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          padding: 7px 12px;
          border: 1px solid var(--color-danger);
          border-radius: 8px;
          background: var(--color-danger-soft);
          color: var(--color-danger);
          font-size: 13px;
          cursor: pointer;
        }
      }
    }

    .system-settings-tools {
      display: flex;
      flex-wrap: wrap;
      gap: 10px;

      .system-settings-tool {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 9px 12px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;

        &:has(:checked) {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }

        .system-settings-tool-checkbox {
          margin: 0;
          accent-color: var(--color-primary);
        }

        .system-settings-tool-label {
          font-size: 13px;
          line-height: 1.4;
        }
      }
    }
  }

  :disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
}
</style>
