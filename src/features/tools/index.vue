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
          <strong>{{ activeToolMeta?.label || "工具" }}</strong>
          <span>{{ activeToolMeta?.summary || "" }}</span>
        </div>
      </header>

      <GitToolView
        v-if="activeTool === 'git'"
        :repos="repos"
        @add-repo="$emit('add-repo')"
      />
    </section>
  </section>
</template>

<script setup>
import { computed, defineAsyncComponent, ref } from "vue"
import { ArrowLeft, GitBranchIcon } from "lucide-vue-next"

const GitToolView = defineAsyncComponent(
  () => import("@/features/gitTool/index.vue")
)

const props = defineProps({
  repos: {
    type: Array,
    default: () => []
  }
})

defineEmits(["add-repo"])

const activeTool = ref("")

const toolItems = computed(() => [
  {
    id: "git",
    label: "Git 管理",
    summary: "管理项目的本地分支归档、提交检查和 stash 归档。",
    meta: `${props.repos.length} 个项目`,
    icon: GitBranchIcon
  }
])

const activeToolMeta = computed(
  () => toolItems.value.find(tool => tool.id === activeTool.value) || null
)

function openTool(toolId) {
  activeTool.value = toolId
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
  gap: 12px;
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
  letter-spacing: 0.12em;
  text-transform: uppercase;
}

.tools-view-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  overflow: auto;
}

.tools-view-tool {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 70px;
  padding: 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: #ffffff;
  color: var(--color-text);
  cursor: pointer;
  text-align: left;
}

.tools-view-tool:hover {
  border-color: #b8c9d8;
  background: #f8fbff;
}

.tools-view-tool-icon {
  display: inline-flex;
  width: 36px;
  height: 36px;
  flex: 0 0 36px;
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
  color: var(--color-text-soft);
  font-size: 0.78rem;
  font-weight: 700;
}

.tools-view-detail-page {
  gap: 10px;
}

.tools-view-detail-head {
  display: flex;
  flex: none;
  align-items: center;
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
  flex-direction: column;
  gap: 2px;
}

.tools-view-detail-title strong {
  color: var(--color-text);
  font-size: 0.94rem;
}

.tools-view-detail-title span {
  overflow: hidden;
  color: var(--color-text-muted);
  font-size: 0.76rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
