<template>
  <section class="tools-view">
    <section v-if="!activeTool" class="tools-view-list-page">
      <header class="tools-view-list-head">
        <div>
          <p class="tools-view-mark">Tools</p>
          <h1>工具中心</h1>
        </div>
        <span>{{ toolItems.length }} 个工具</span>
      </header>

      <div class="tools-view-list">
        <button
          v-for="tool in toolItems"
          :key="tool.id"
          class="tools-view-tool"
          type="button"
          @click="openTool(tool.id)"
        >
          <span class="tools-view-tool-icon">
            <component :is="tool.icon" :size="20" />
          </span>
          <span class="tools-view-tool-main">
            <strong class="tools-view-tool-name">{{ tool.label }}</strong>
            <span class="tools-view-tool-desc">{{ tool.summary }}</span>
          </span>
          <span class="tools-view-tool-meta">{{ tool.meta }}</span>
        </button>
      </div>
    </section>

    <section v-else class="tools-view-detail-page">
      <header class="tools-view-detail-head">
        <button class="tools-view-back" type="button" @click="closeTool">
          <ArrowLeft :size="15" />
          工具列表
        </button>
        <div class="tools-view-detail-title">
          <strong class="tools-view-detail-name">{{
            activeToolMeta?.label || "工具"
          }}</strong>
          <span class="tools-view-detail-summary">{{
            activeToolMeta?.summary || ""
          }}</span>
        </div>
        <div v-if="activeTool === 'git'" class="tools-view-git-status">
          <div
            v-for="item in gitToolStatus"
            :key="item.label"
            class="tools-view-git-status-item"
          >
            <span class="tools-view-git-status-label">{{ item.label }}</span>
            <strong class="tools-view-git-status-value">{{
              item.value
            }}</strong>
          </div>
        </div>
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
  ArrowLeft,
  Braces,
  FileDiff,
  GitBranchIcon,
  Images,
  Network,
  PawPrint,
  Share2
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
  }
})

const emit = defineEmits(["add-repo", "detail-change"])

const activeTool = ref("")
const gitToolStatus = ref([])

const codexInstalled = computed(() =>
  props.cliTargets.some(
    (target) => target.id === "codex" && target.installed === true
  )
)

const toolItems = computed(() => {
  const items = [
    {
      id: "json-agent",
      label: "JSON 智能解析",
      summary: "格式化 JSON，并通过 Codex Agent 按指令修复异常内容。",
      meta: "JSON / Agent",
      icon: Braces
    },
    {
      id: "string-diff",
      label: "差异对比",
      summary: "对比两个字符串或 JSON 内容，定位路径、行和字符差异。",
      meta: "文本 / JSON",
      icon: FileDiff
    },
    {
      id: "image-link-extractor",
      label: "图片链接提取",
      summary: "从文本、Markdown、HTML 或 JSON 中提取图片并批量导出。",
      meta: "图片 / 导出",
      icon: Images
    },
    {
      // 端口监测直接使用桌面端系统权限，不依赖浏览器工具服务。
      id: "port-monitor",
      label: "端口监测",
      summary: "查看本机监听端口、所属程序和系统服务，并按需关闭进程。",
      meta: "本机进程",
      icon: Network
    },
    {
      id: "git",
      label: "Git 管理",
      summary: "管理项目的本地分支归档、提交检查和 stash 归档。",
      meta: `${props.repos.length} 个项目`,
      icon: GitBranchIcon
    },
    {
      id: "lan-share",
      label: "设备快传",
      summary: "启动局域网服务，通过二维码向同网段设备共享文件并实时通信。",
      meta: "手动启动",
      icon: Share2
    }
  ]

  // 仅在应用状态确认本机已安装 Codex 后提供宠物管理入口。
  if (codexInstalled.value) {
    items.push({
      id: "codex-pets",
      label: "Codex 宠物",
      summary: "管理 Codex 宠物的名称、启用状态和本地文件。",
      meta: "本机 Codex",
      icon: PawPrint
    })
  }

  return items
})

const activeToolMeta = computed(
  () => toolItems.value.find((tool) => tool.id === activeTool.value) || null
)

function openTool(toolId) {
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
  gap: 14px;
}

.tools-view-list-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
}

.tools-view-list-head h1 {
  margin: 0;
  color: var(--color-text);
  font-size: 1.34rem;
  line-height: 1.2;
}

.tools-view-list-head span {
  color: var(--color-text-muted);
  font-size: 0.82rem;
  font-weight: 700;
}

.tools-view-mark {
  margin: 0 0 5px;
  color: var(--color-text-soft);
  font-size: 0.7rem;
  font-weight: 700;
  letter-spacing: 0;
  text-transform: uppercase;
}

.tools-view-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  overflow: auto;
  padding-right: 2px;
}

.tools-view-tool {
  position: relative;
  display: flex;
  align-items: center;
  gap: 14px;
  min-height: 76px;
  padding: 13px 14px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #ffffff;
  color: var(--color-text);
  cursor: pointer;
  text-align: left;
  transition:
    border-color 0.18s ease,
    background-color 0.18s ease,
    box-shadow 0.18s ease,
    transform 0.18s ease;
}

.tools-view-tool:hover {
  border-color: #b8c9d8;
  background: #f8fbff;
  box-shadow: 0 10px 26px rgba(34, 56, 83, 0.08);
  transform: translateY(-1px);
}

.tools-view-tool-icon {
  display: inline-flex;
  width: 40px;
  height: 40px;
  flex: 0 0 40px;
  align-items: center;
  justify-content: center;
  border-radius: 7px;
  background: #e8f1fa;
  color: #2f5f91;
}

.tools-view-tool-main {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  gap: 4px;
}

.tools-view-tool-name {
  color: var(--color-text);
  font-size: 0.92rem;
}

.tools-view-tool-desc {
  color: var(--color-text-muted);
  font-size: 0.8rem;
}

.tools-view-tool-meta {
  flex: none;
  padding: 4px 8px;
  border: 1px solid #d6e2ec;
  border-radius: 999px;
  background: #f7fafc;
  color: var(--color-text-soft);
  font-size: 0.78rem;
  font-weight: 700;
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
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 700;
  }

  .tools-view-back:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
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
    font-size: 0.94rem;
  }

  .tools-view-detail-summary {
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.76rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tools-view-git-status {
    display: grid;
    width: 318px;
    flex: none;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 6px 8px;
    padding: 7px 8px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #f8fafc;

    .tools-view-git-status-item {
      display: flex;
      min-width: 0;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      min-height: 22px;
      padding: 0 2px;

      .tools-view-git-status-label {
        overflow: hidden;
        color: var(--color-text-soft);
        font-size: 0.72rem;
        font-weight: 700;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .tools-view-git-status-value {
        min-width: 0;
        overflow: hidden;
        color: var(--color-primary);
        font-size: 0.84rem;
        text-align: right;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }
  }
}
</style>
