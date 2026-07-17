<template>
  <section class="tools-view">
    <section v-if="!activeTool" class="tools-view-list-page">
      <header class="tools-view-list-head">
        <div>
          <p class="tools-view-mark">Tools</p>
          <h1>其他工具</h1>
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
        <button class="tools-view-back" type="button" @click="activeTool = ''">
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
      <section v-else-if="activeTool === 'utility'" class="tools-view-utility">
        <div class="tools-view-utility-card">
          <span class="tools-view-utility-icon">
            <Hammer :size="28" />
          </span>
          <div class="tools-view-utility-main">
            <p class="tools-view-mark">Utility Toolbox</p>
            <h2 class="tools-view-utility-title">实用工具大全</h2>
            <p class="tools-view-utility-desc">
              启用后会启动本机浏览器工具服务，并在浏览器打开与桌面端风格一致的工具面板。
            </p>
            <div class="tools-view-utility-actions">
              <button
                class="tools-view-enable"
                type="button"
                :disabled="utilityPending"
                @click="enableUtilityToolbox"
              >
                <Power :size="16" />
                {{ utilityPending ? "正在启用" : "启用" }}
              </button>
              <span class="tools-view-utility-status">按需启动本机服务</span>
            </div>
          </div>
        </div>

        <div class="tools-view-utility-grid">
          <article class="tools-view-utility-tool">
            <strong>差异对比工具</strong>
            <span>对比两个字符串或 JSON 内容的差异，支持仅查看变化项。</span>
          </article>
          <article class="tools-view-utility-tool">
            <strong>图片链接提取</strong>
            <span>从文本、Markdown、HTML 或 JSON 中提取图片链接并预览。</span>
          </article>
        </div>

        <div class="tools-view-utility-info">
          <strong class="tools-view-utility-info-title">当前工具</strong>
          <span class="tools-view-utility-info-desc">
            已内置差异对比工具和图片链接提取工具，打开后可在浏览器面板左侧切换。
          </span>
          <small v-if="utilityUrl" class="tools-view-utility-info-url">
            服务地址：{{ utilityUrl }}
          </small>
        </div>
      </section>
    </section>
  </section>
</template>

<script setup>
import { computed, ref } from "vue"
import {
  ArrowLeft,
  GitBranchIcon,
  Hammer,
  PawPrint,
  Power,
  Share2,
  Wrench
} from "lucide-vue-next"
import GitToolView from "@/features/gitTool/index.vue"
import LanShareView from "@/features/lanShare/index.vue"
import CodexPetManager from "@/features/tools/components/CodexPetManager.vue"
import { toolboxApi } from "@/api"
import { createMessage } from "@/utils/message"

const props = defineProps({
  cliTargets: {
    type: Array,
    default: () => []
  },
  repos: {
    type: Array,
    default: () => []
  }
})

defineEmits(["add-repo"])

const activeTool = ref("")
const gitToolStatus = ref([])
const utilityPending = ref(false)
const utilityUrl = ref("")

const codexInstalled = computed(() =>
  props.cliTargets.some(
    (target) => target.id === "codex" && target.installed === true
  )
)

const toolItems = computed(() => {
  const items = [
    {
      id: "utility",
      label: "实用工具",
      summary: "启动浏览器工具大全面板，集中使用轻量辅助工具。",
      meta: "2 个工具",
      icon: Wrench
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

async function enableUtilityToolbox() {
  utilityPending.value = true

  try {
    const result = await toolboxApi.openToolbox()
    utilityUrl.value = result.url || ""
    createMessage.success("实用工具大全已在浏览器打开。")
  } catch (error) {
    createMessage.error(error.message || String(error))
  } finally {
    utilityPending.value = false
  }
}
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

.tools-view-utility {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 12px;
  overflow: auto;
}

.tools-view-utility-card {
  display: flex;
  align-items: flex-start;
  gap: 18px;
  padding: 20px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background:
    linear-gradient(135deg, rgba(237, 242, 248, 0.72), transparent 42%),
    #ffffff;
  box-shadow: var(--shadow-panel);
}

.tools-view-utility-icon {
  display: grid;
  width: 56px;
  height: 56px;
  flex: 0 0 56px;
  place-items: center;
  border: 1px solid #c9d9e7;
  border-radius: 8px;
  background: #eef5fb;
  color: #2f5f91;
}

.tools-view-utility-main {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  align-items: flex-start;
  gap: 10px;
}

.tools-view-utility-title {
  margin: 0;
  color: var(--color-text);
  font-size: 1.18rem;
  line-height: 1.25;
}

.tools-view-utility-desc {
  margin: 0;
  color: var(--color-text-muted);
  font-size: 0.86rem;
  line-height: 1.7;
}

.tools-view-utility-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.tools-view-enable {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  height: 34px;
  padding: 0 13px;
  border: 1px solid var(--color-primary);
  border-radius: 7px;
  background: var(--color-primary);
  color: #ffffff;
  cursor: pointer;
  font-size: 0.82rem;
  font-weight: 700;
}

.tools-view-enable:disabled {
  cursor: not-allowed;
  opacity: 0.58;
}

.tools-view-utility-status {
  color: var(--color-text-soft);
  font-size: 0.78rem;
  font-weight: 700;
}

.tools-view-utility-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.tools-view-utility-tool {
  display: flex;
  min-height: 86px;
  flex-direction: column;
  justify-content: center;
  gap: 7px;
  padding: 14px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #ffffff;
}

.tools-view-utility-tool strong {
  color: var(--color-text);
  font-size: 0.9rem;
}

.tools-view-utility-tool span {
  color: var(--color-text-muted);
  font-size: 0.8rem;
  line-height: 1.55;
}

.tools-view-utility-info {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 14px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel-soft);
}

.tools-view-utility-info-title {
  color: var(--color-text);
  font-size: 0.86rem;
}

.tools-view-utility-info-desc,
.tools-view-utility-info-url {
  color: var(--color-text-muted);
  font-size: 0.8rem;
  line-height: 1.5;
}
</style>
