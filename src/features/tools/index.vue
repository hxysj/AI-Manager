<template>
  <section :class="['tools-view', `tools-view--${themeMode}`]">
    <section v-if="!activeTool" class="tools-view-list-page">
      <header class="tools-view-list-head">
        <h1 class="tools-view-title">工具中心</h1>
      </header>

      <div class="tools-view-list">
        <button
          v-for="tool in toolItems"
          :key="tool.id"
          class="tools-view-tool"
          :class="`tools-view-tool--${tool.theme}`"
          type="button"
          @click="openTool(tool.id)"
        >
          <span class="tools-view-tool-icon">
            <component :is="tool.icon" :size="24" :stroke-width="2.1" />
          </span>
          <span class="tools-view-tool-main">
            <span class="tools-view-tool-head">
              <span data-emphasis class="tools-view-tool-name">{{
                tool.label
              }}</span>
              <span class="tools-view-tool-meta">{{ tool.meta }}</span>
            </span>
            <span class="tools-view-tool-desc">{{ tool.summary }}</span>
          </span>
        </button>
      </div>
    </section>

    <section v-else class="tools-view-detail-page">
      <header class="tools-view-detail-head">
        <button class="tools-view-back" type="button" @click="closeTool">
          <ArrowLeft :size="15" />
          工具列表
        </button>
        <div v-if="activeTool === 'git'" class="tools-view-git-badge">
          <GitBranchIcon :size="20" class="tools-view-git-badge-icon" />
        </div>
        <div
          v-else-if="activeTool === 'image-workbench'"
          class="tools-view-image-badge"
        >
          <ImageIcon :size="20" class="tools-view-image-badge-icon" />
        </div>
        <div class="tools-view-detail-title">
          <span data-emphasis class="tools-view-detail-name">{{
            activeToolMeta?.label || "工具"
          }}</span>
          <span class="tools-view-detail-summary">{{
            activeToolMeta?.summary || ""
          }}</span>
        </div>
        <div v-if="activeTool === 'git'" class="tools-view-git-status">
          <template v-for="(item, index) in gitToolStatus" :key="item.label">
            <div class="tools-view-git-status-item">
              <div class="tools-view-git-status-item-head">
                <component
                  :is="getGitStatusIcon(item.label)"
                  :size="14"
                  class="tools-view-git-status-icon"
                />
                <span class="tools-view-git-status-label">{{
                  item.label
                }}</span>
              </div>
              <span data-emphasis class="tools-view-git-status-value">{{
                item.value
              }}</span>
            </div>
            <div
              v-if="index < gitToolStatus.length - 1"
              class="tools-view-git-status-divider"
            />
          </template>
        </div>
        <div
          v-if="activeTool === 'image-workbench'"
          class="image-workbench-header-actions"
        >
          <div
            class="image-generation-modes"
            role="group"
            aria-label="生图调用模式"
          >
            <button
              v-for="mode in imageGenerationModes"
              :key="mode.value"
              class="image-generation-mode"
              :class="{ active: imageGenerationMode === mode.value }"
              type="button"
              :aria-pressed="imageGenerationMode === mode.value"
              @click="imageGenerationMode = mode.value"
            >
              {{ mode.label }}
            </button>
          </div>
          <button
            class="tools-view-icon-btn"
            type="button"
            title="刷新工作台"
            aria-label="刷新工作台"
            @click="imageWorkbenchRef?.refreshAll?.()"
          >
            <RefreshCw :size="16" />
          </button>
        </div>
        <button
          v-if="activeTool !== 'git'"
          class="tools-view-theme"
          type="button"
          :title="themeMode === 'dark' ? '切换为亮色模式' : '切换为暗色模式'"
          :aria-label="
            themeMode === 'dark' ? '切换为亮色模式' : '切换为暗色模式'
          "
          @click="$emit('toggle-theme')"
        >
          <Sun v-if="themeMode === 'dark'" :size="17" :stroke-width="1.9" />
          <Moon v-else :size="17" :stroke-width="1.9" />
        </button>
      </header>

      <GitToolView
        v-if="activeTool === 'git'"
        :repos="repos"
        @add-repo="$emit('add-repo')"
        @status-change="gitToolStatus = $event"
      />
      <LanShareView v-else-if="activeTool === 'lan-share'" />
      <CodexPetManager v-else-if="activeTool === 'codex-pets'" />
      <PortMonitor v-else-if="activeTool === 'port-monitor'" />
      <StringDiff v-else-if="activeTool === 'string-diff'" />
      <ImageLinkExtractor v-else-if="activeTool === 'image-link-extractor'" />
      <ImageWorkbench
        v-else-if="activeTool === 'image-workbench'"
        ref="imageWorkbenchRef"
        v-model:generation-mode="imageGenerationMode"
      />
      <JsonAgentTool
        v-else-if="activeTool === 'json-agent'"
        :providers="providers"
        :runtime-models="runtimeModels"
        :runtime-profiles="runtimeProfiles"
      />
    </section>
  </section>
</template>

<script setup>
import {
  computed,
  defineAsyncComponent,
  onBeforeUnmount,
  ref,
  watch
} from "vue"
import {
  Archive,
  ArrowLeft,
  Braces,
  Database,
  FileDiff,
  GitBranchIcon,
  Image as ImageIcon,
  Images,
  Moon,
  Network,
  PawPrint,
  RefreshCw,
  Share2,
  Sun
} from "lucide-vue-next"
import GitToolView from "@/features/gitTool/index.vue"
import LanShareView from "@/features/lanShare/index.vue"
import CodexPetManager from "@/features/tools/components/CodexPetManager.vue"
import ImageLinkExtractor from "@/features/tools/components/ImageLinkExtractor.vue"
import PortMonitor from "@/features/tools/components/PortMonitor.vue"
import StringDiff from "@/features/tools/components/StringDiff.vue"

const JsonAgentTool = defineAsyncComponent(
  () => import("@/features/tools/components/JsonAgentTool.vue")
)

// 图片工作台按需加载，进入工具后再读取任务和官方账号。
const ImageWorkbench = defineAsyncComponent(
  () => import("@/features/tools/components/ImageWorkbench.vue")
)

const props = defineProps({
  cliTargets: {
    type: Array,
    default: () => []
  },
  repos: {
    type: Array,
    default: () => []
  },
  providers: {
    type: Array,
    default: () => []
  },
  runtimeModels: {
    type: Array,
    default: () => []
  },
  runtimeProfiles: {
    type: Array,
    default: () => []
  },
  themeMode: {
    type: String,
    default: "light"
  }
})

const emit = defineEmits(["add-repo", "detail-change", "toggle-theme"])

const activeTool = ref("")
const gitToolStatus = ref([])
const imageWorkbenchRef = ref(null)

function getGitStatusIcon(label) {
  if (label === "当前分支" || label === "本地分支") return GitBranchIcon
  if (label === "Stash") return Database
  if (label === "归档") return Archive
  return GitBranchIcon
}
// 头部和工作台共享模式，历史任务回填时也同步切换。
const imageGenerationMode = ref("web")
const imageGenerationModes = [
  { value: "web", label: "Web（网页额度）" },
  { value: "codex", label: "Codex（API额度）" }
]

const codexInstalled = computed(() =>
  props.cliTargets.some(
    (target) => target.id === "codex" && target.installed === true
  )
)

const toolItems = computed(() => {
  const items = [
    {
      id: "image-workbench",
      label: "图片工作台",
      summary: "使用官方账号通过 Web 或 Codex 生成、编辑图片，管理与导出任务。",
      meta: "文生图 / 图片编辑",
      icon: Images,
      theme: "sky"
    },
    {
      id: "json-agent",
      label: "JSON 智能解析",
      summary: "格式化 JSON，并通过 Codex Agent 按指令修复异常内容。",
      meta: "JSON / Agent",
      icon: Braces,
      theme: "purple"
    },
    {
      id: "string-diff",
      label: "差异对比",
      summary: "对比两个字符串或 JSON 内容，定位路径、行和字符差异。",
      meta: "文本 / JSON",
      icon: FileDiff,
      theme: "green"
    },
    {
      id: "image-link-extractor",
      label: "图片链接提取",
      summary: "从文本、Markdown、HTML 或 JSON 中提取图片并批量导出。",
      meta: "图片 / 导出",
      icon: ImageIcon,
      theme: "orange"
    },
    {
      // 端口监测直接使用桌面端系统权限，不依赖浏览器工具服务。
      id: "port-monitor",
      label: "端口监测",
      summary: "查看本机监听端口、所属程序和系统服务，并按需关闭进程。",
      meta: "本机进程",
      icon: Network,
      theme: "blue"
    },
    {
      id: "git",
      label: "Git 管理",
      summary: "管理项目的本地分支归档、提交检查和 stash 归档。",
      meta: `${props.repos.length} 个项目`,
      icon: GitBranchIcon,
      theme: "rose"
    },
    {
      id: "lan-share",
      label: "设备快传",
      summary: "客户端直接连接，聊天中发送文件与图片；网页仅作为访客入口。",
      meta: "自动发现",
      icon: Share2,
      theme: "violet"
    }
  ]

  // 仅在应用状态确认本机已安装 Codex 后提供宠物管理入口。
  if (codexInstalled.value) {
    items.push({
      id: "codex-pets",
      label: "Codex 宠物",
      summary: "管理 Codex 宠物的名称、启用状态和本地文件。",
      meta: "本机 Codex",
      icon: PawPrint,
      theme: "sky"
    })
  }

  return items
})

const activeToolMeta = computed(
  () => toolItems.value.find((tool) => tool.id === activeTool.value) || null
)

function openTool(toolId) {
  if (toolId === "image-workbench") imageGenerationMode.value = "web"
  activeTool.value = toolId
}

function closeTool() {
  activeTool.value = ""
}

// 工具详情独占工作区，退出详情或组件卸载时恢复全局侧栏。
watch(activeTool, (toolId) => emit("detail-change", Boolean(toolId)))
onBeforeUnmount(() => emit("detail-change", false))
</script>

<style scoped lang="less">
.tools-view {
  display: flex;
  min-height: 0;
  height: 100%;
  flex-direction: column;
  overflow: hidden;
}

.tools-view-list-page,
.tools-view-detail-page {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
}

.tools-view-list-page {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  padding: 4px 4px 16px 2px;
}

.tools-view-list-head {
  display: flex;
  align-items: center;
  margin-bottom: 24px;
}

.tools-view-title {
  margin: 0;
  color: var(--color-text);
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.01em;
  line-height: 1.2;
}

.tools-view-list {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px 20px;
  overflow-y: auto;
  padding-right: 4px;
  padding-bottom: 8px;
}

.tools-view-tool {
  position: relative;
  display: flex;
  align-items: flex-start;
  gap: 18px;
  min-height: 112px;
  padding: 22px 24px;
  border: 1px solid var(--color-line);
  border-radius: 14px;
  background: var(--color-panel);
  color: var(--color-text);
  cursor: pointer;
  text-align: left;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);
  transition:
    border-color 0.2s ease,
    background-color 0.2s ease,
    box-shadow 0.2s ease,
    transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);

  &:hover {
    border-color: var(--color-line-strong);
    background: var(--color-panel);
    box-shadow:
      0 10px 24px -4px rgba(34, 56, 83, 0.08),
      0 4px 6px -2px rgba(34, 56, 83, 0.03);
    transform: translateY(-2px);
  }

  &:active {
    transform: translateY(0);
    box-shadow: 0 4px 12px rgba(34, 56, 83, 0.05);
  }

  // Theme color variants (light mode)
  &--sky {
    --tool-color: #0284c7;
    --tool-bg: #e8f4fd;
  }

  &--purple {
    --tool-color: #4f46e5;
    --tool-bg: #eeedfd;
  }

  &--green {
    --tool-color: #0d9468;
    --tool-bg: #e6f8f1;
  }

  &--orange {
    --tool-color: #e66a15;
    --tool-bg: #fff0e5;
  }

  &--blue {
    --tool-color: #1d5bd8;
    --tool-bg: #e6f0fc;
  }

  &--rose {
    --tool-color: #dc2626;
    --tool-bg: #feebee;
  }

  &--violet {
    --tool-color: #6d28d9;
    --tool-bg: #eeeafd;
  }
}

:global(:root[data-theme="dark"]),
.tools-view--dark {
  .tools-view-tool {
    background: var(--color-panel);
    border-color: var(--color-line);

    &:hover {
      border-color: var(--color-line-strong);
      background: var(--color-panel-soft);
      box-shadow: 0 12px 28px rgba(0, 0, 0, 0.45);
    }

    &--sky {
      --tool-color: #38bdf8;
      --tool-bg: rgba(56, 189, 248, 0.16);
    }

    &--purple {
      --tool-color: #818cf8;
      --tool-bg: rgba(99, 102, 241, 0.16);
    }

    &--green {
      --tool-color: #34d399;
      --tool-bg: rgba(52, 211, 153, 0.16);
    }

    &--orange {
      --tool-color: #fb923c;
      --tool-bg: rgba(251, 146, 60, 0.16);
    }

    &--blue {
      --tool-color: #60a5fa;
      --tool-bg: rgba(96, 165, 250, 0.16);
    }

    &--rose {
      --tool-color: #fb7185;
      --tool-bg: rgba(251, 113, 133, 0.16);
    }

    &--violet {
      --tool-color: #a78bfa;
      --tool-bg: rgba(167, 139, 250, 0.16);
    }
  }

  .tools-view-git-status {
    background: var(--color-panel);
    border-color: var(--color-line);

    .tools-view-git-status-icon {
      color: #60a5fa;
    }

    .tools-view-git-status-value {
      color: #60a5fa;
    }
  }

  .tools-view-image-badge {
    background: #3b82f6;
  }

  .image-generation-modes {
    background: var(--color-panel);
    border-color: var(--color-line);

    .image-generation-mode {
      color: var(--color-text-muted);

      &.active {
        color: #ffffff;
        background: #2563eb;
      }

      &:hover:not(.active) {
        color: var(--color-text);
      }
    }
  }
}

.tools-view-tool-icon {
  display: inline-flex;
  width: 52px;
  height: 52px;
  flex: 0 0 52px;
  align-items: center;
  justify-content: center;
  border-radius: 12px;
  background: var(--tool-bg);
  color: var(--tool-color);
  margin-top: 1px;
  transition: transform 0.2s ease;
}

.tools-view-tool:hover .tools-view-tool-icon {
  transform: scale(1.05);
}

.tools-view-tool-main {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  gap: 8px;
}

.tools-view-tool-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.tools-view-tool-name {
  color: var(--color-text);
  font-size: 17px;
  font-weight: 700;
  line-height: 1.3;
}

.tools-view-tool-meta {
  flex: none;
  display: inline-flex;
  align-items: center;
  padding: 3px 12px;
  border-radius: 9999px;
  background: var(--tool-bg);
  color: var(--tool-color);
  font-size: 12px;
  font-weight: 500;
  line-height: 1.4;
  letter-spacing: 0.01em;
}

.tools-view-tool-desc {
  margin: 0;
  color: var(--color-text-muted);
  font-size: 13.5px;
  line-height: 1.55;
  letter-spacing: 0.01em;
}

@media (max-width: 860px) {
  .tools-view-list {
    grid-template-columns: 1fr;
  }
}

.tools-view-detail-page {
  gap: 10px;

  .tools-view-detail-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--color-line);

    .image-workbench-header-actions {
      display: flex;
      align-items: center;
      gap: 8px;

      .tools-view-icon-btn {
        display: grid;
        width: 34px;
        height: 34px;
        flex: 0 0 34px;
        place-items: center;
        border: 1px solid var(--color-line);
        border-radius: 50%;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.2s ease;

        &:hover {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }
      }
    }

    .image-generation-modes {
      display: flex;
      flex: none;
      align-items: center;
      gap: 4px;
      padding: 3px;
      border: 1px solid #bfdbfe;
      border-radius: 9999px;
      background: #ffffff;

      .image-generation-mode {
        height: 28px;
        padding: 0 16px;
        border: 0;
        border-radius: 9999px;
        color: #4b5563;
        background: transparent;
        cursor: pointer;
        white-space: nowrap;
        font-size: 13px;
        font-weight: 500;
        transition: all 0.2s ease;

        &.active {
          color: #ffffff;
          background: #2563eb;
          box-shadow: 0 1px 3px rgba(37, 99, 235, 0.25);
        }

        &:hover:not(.active) {
          color: #1e293b;
        }
      }
    }
  }

  .tools-view-back {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-primary);
    cursor: pointer;
    font-size: var(--font-size-base);
  }

  .tools-view-back:hover {
    border-color: var(--color-line-strong);
    background: var(--color-panel-soft);
  }

  .tools-view-detail-title {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 2px;
  }

  .tools-view-detail-name {
    color: var(--color-text);
    font-size: var(--font-size-lg);
  }

  .tools-view-detail-summary {
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: var(--font-size-base);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tools-view-theme {
    display: grid;
    width: 34px;
    height: 34px;
    flex: 0 0 34px;
    padding: 0;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    line-height: 1;
  }

  .tools-view-theme:hover {
    border-color: var(--color-line-strong);
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  .tools-view-theme :deep(svg) {
    display: block;
  }

  .tools-view-git-badge {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    flex: 0 0 36px;
    background: #2563eb;
    border-radius: 9px;
    transform: rotate(45deg);
    margin: 0 8px 0 4px;

    .tools-view-git-badge-icon {
      transform: rotate(-45deg);
      color: #ffffff;
      stroke-width: 2.2;
    }
  }

  .tools-view-image-badge {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    flex: 0 0 38px;
    background: #2563eb;
    border-radius: 10px;
    margin: 0 6px 0 2px;

    .tools-view-image-badge-icon {
      color: #ffffff;
      stroke-width: 2.1;
    }
  }

  .tools-view-git-status {
    display: flex;
    align-items: center;
    flex: none;
    padding: 5px 8px;
    border: 1px solid var(--color-line);
    border-radius: 9px;
    background: var(--color-panel);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);

    .tools-view-git-status-item {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 0 14px;
      gap: 3px;

      .tools-view-git-status-item-head {
        display: flex;
        align-items: center;
        gap: 5px;
      }

      .tools-view-git-status-icon {
        color: #2563eb;
      }

      .tools-view-git-status-label {
        overflow: hidden;
        color: var(--color-text-muted);
        font-size: 12px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .tools-view-git-status-value {
        color: #2563eb;
        font-size: 16px;
        font-weight: 700;
        line-height: 1.2;
      }
    }

    .tools-view-git-status-divider {
      width: 1px;
      height: 26px;
      background: var(--color-line);
      flex-shrink: 0;
    }
  }
}
</style>
