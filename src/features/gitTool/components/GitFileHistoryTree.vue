<template>
  <aside class="git-history-tree">
    <div class="git-history-tree-heading">
      <span>当前工作树</span
      ><span class="git-history-tree-count">{{ files.length }} 个文件</span>
      <button
        class="git-history-tree-icon-button"
        type="button"
        title="刷新文件树&#10;命令：git ls-files --cached --others --exclude-standard"
        :disabled="loading || busy"
        @click="$emit('refresh')"
      >
        <RefreshCw :size="14" />
      </button>
    </div>
    <div class="git-history-tree-search">
      <Search :size="14" />
      <input
        v-model="query"
        class="git-history-tree-search-input"
        aria-label="快速定位文件"
        placeholder="搜索文件名或路径，不过滤文件树"
        @keydown.enter.prevent="locate($event.shiftKey ? -1 : 1)"
      />
      <span v-if="query" class="git-history-tree-match-count"
        >{{ matches.length ? matchIndex + 1 : 0 }}/{{ matches.length }}</span
      >
      <button
        class="git-history-tree-icon-button"
        type="button"
        aria-label="上一个匹配文件"
        title="上一个匹配文件（Shift+Enter）"
        :disabled="!matches.length"
        @click="locate(-1)"
      >
        <ChevronUp :size="13" />
      </button>
      <button
        class="git-history-tree-icon-button"
        type="button"
        aria-label="下一个匹配文件"
        title="下一个匹配文件（Enter）"
        :disabled="!matches.length"
        @click="locate(1)"
      >
        <ChevronDown :size="13" />
      </button>
    </div>
    <div v-if="error" class="git-history-tree-notice" role="alert">
      {{ error }}
    </div>
    <div v-else-if="loading" class="git-history-tree-notice" role="status">
      正在读取文件树…
    </div>
    <div v-else-if="!files.length" class="git-history-tree-notice">
      当前工作树没有可显示的文件
    </div>
    <div
      v-else
      ref="viewport"
      class="git-history-tree-viewport"
      role="tree"
      aria-label="当前工作树文件"
      @scroll="scrollTop = $event.target.scrollTop"
    >
      <div
        class="git-history-tree-content"
        :style="{ height: `${visibleNodes.length * rowHeight}px` }"
      >
        <button
          v-for="(node, index) in windowNodes"
          :key="node.path"
          :class="[
            'git-history-tree-row',
            {
              'git-history-tree-row-selected':
                !node.folder && node.path === selectedPath,
              'git-history-tree-row-located':
                !node.folder && node.path === locatedPath
            }
          ]"
          :style="{
            top: `${(windowStart + index) * rowHeight}px`,
            paddingLeft: `${10 + node.depth * 16}px`
          }"
          type="button"
          role="treeitem"
          :aria-level="node.depth + 1"
          :aria-expanded="node.folder ? expanded.has(node.path) : undefined"
          :aria-selected="!node.folder && node.path === selectedPath"
          :title="node.path"
          :disabled="busy"
          @click="selectNode(node)"
        >
          <component
            :is="expanded.has(node.path) ? ChevronDown : ChevronRight"
            v-if="node.folder"
            :size="12"
          />
          <span v-else class="git-history-tree-chevron-space"></span>
          <component
            :is="
              node.folder
                ? expanded.has(node.path)
                  ? FolderOpen
                  : Folder
                : FileText
            "
            :size="14"
          />
          <span class="git-history-tree-name">{{ node.name }}</span>
        </button>
      </div>
    </div>
    <div class="git-history-tree-footer">
      {{
        query && !matches.length
          ? "没有匹配文件，文件树保持完整"
          : "搜索仅定位；点击文件查看提交历史"
      }}
    </div>
  </aside>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue"
import {
  ChevronDown,
  ChevronRight,
  ChevronUp,
  FileText,
  Folder,
  FolderOpen,
  RefreshCw,
  Search
} from "lucide-vue-next"

const props = defineProps({
  files: { type: Array, default: () => [] },
  selectedPath: { type: String, default: "" },
  loading: { type: Boolean, default: false },
  error: { type: String, default: "" },
  busy: { type: Boolean, default: false }
})
const emit = defineEmits(["select-file", "refresh"])
const query = ref("")
const matchIndex = ref(-1)
const locatedPath = ref("")
const expanded = ref(new Set())
const viewport = ref(null)
const scrollTop = ref(0)
const height = ref(400)
const rowHeight = 28
let observer

// 目录索引只随文件列表重建，搜索不参与树结构的生成或过滤。
const tree = computed(() => {
  const root = { children: [] }
  const directories = new Map([["", root]])
  for (const path of props.files) {
    const parts = path.split("/")
    let parent = root
    let current = ""
    parts.forEach((name, index) => {
      current = current ? `${current}/${name}` : name
      const folder = index < parts.length - 1
      let node = folder ? directories.get(current) : null
      if (!node) {
        node = { name, path: current, folder, children: [] }
        parent.children.push(node)
        if (folder) directories.set(current, node)
      }
      parent = node
    })
  }
  const sort = (node) => {
    node.children.sort(
      (left, right) =>
        Number(right.folder) - Number(left.folder) ||
        left.name.localeCompare(right.name, "zh-CN")
    )
    node.children.filter((child) => child.folder).forEach(sort)
  }
  sort(root)
  return root.children
})
const visibleNodes = computed(() => {
  const nodes = []
  const visit = (children, depth) => {
    for (const node of children) {
      nodes.push({ ...node, depth })
      if (node.folder && expanded.value.has(node.path))
        visit(node.children, depth + 1)
    }
  }
  visit(tree.value, 0)
  return nodes
})
const windowStart = computed(() =>
  // 刷新或折叠后行数可能减少，旧滚动位置不能让可见区域变成空白。
  Math.max(
    0,
    Math.min(
      Math.floor(scrollTop.value / rowHeight),
      visibleNodes.value.length - Math.ceil(height.value / rowHeight)
    ) - 6
  )
)
const windowNodes = computed(() =>
  visibleNodes.value.slice(
    windowStart.value,
    windowStart.value + Math.ceil(height.value / rowHeight) + 12
  )
)
const matches = computed(() => {
  const text = query.value.trim().replace(/\\/g, "/").toLocaleLowerCase()
  return text
    ? props.files.filter((path) => path.toLocaleLowerCase().includes(text))
    : []
})

async function locate(direction) {
  if (!matches.value.length) {
    matchIndex.value = -1
    locatedPath.value = ""
    return
  }
  matchIndex.value =
    (matchIndex.value + direction + matches.value.length) % matches.value.length
  const path = matches.value[matchIndex.value]
  locatedPath.value = path
  const folders = new Set(expanded.value)
  const parts = path.split("/")
  parts.pop()
  for (let index = 1; index <= parts.length; index++)
    folders.add(parts.slice(0, index).join("/"))
  expanded.value = folders
  await nextTick()
  if (locatedPath.value !== path) return
  const index = visibleNodes.value.findIndex((node) => node.path === path)
  scrollTop.value = Math.max(0, index * rowHeight - height.value / 2)
  if (viewport.value) viewport.value.scrollTop = scrollTop.value
}

function selectNode(node) {
  if (!node.folder) {
    emit("select-file", node.path)
    return
  }
  const folders = new Set(expanded.value)
  if (folders.has(node.path)) folders.delete(node.path)
  else folders.add(node.path)
  expanded.value = folders
}

watch([query, () => props.files], () => {
  matchIndex.value = -1
  locate(1)
})

// 文件树只渲染视口附近的行，展开大型目录时也不累积大量 DOM。
watch(viewport, (element) => {
  observer?.disconnect()
  if (!element) return
  observer = new ResizeObserver(() => {
    height.value = element.clientHeight || 400
  })
  observer.observe(element)
})
onBeforeUnmount(() => observer?.disconnect())
</script>

<style scoped lang="less">
.git-history-tree {
  display: flex;
  width: 310px;
  min-height: 0;
  flex: none;
  flex-direction: column;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  overflow: hidden;

  .git-history-tree-icon-button {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    padding: 4px;
    border: 0;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    &:hover {
      color: var(--color-primary);
      background: var(--color-primary-soft);
    }
    &:disabled {
      opacity: 0.4;
      cursor: default;
    }
  }

  .git-history-tree-heading {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    border-bottom: 1px solid var(--color-line);
    .git-history-tree-count {
      flex: 1;
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
    }
  }

  .git-history-tree-search {
    display: flex;
    align-items: center;
    gap: 3px;
    margin: 10px;
    padding: 0 5px;
    border: 1px solid var(--color-line);
    border-radius: 5px;
    color: var(--color-text-muted);
    &:focus-within {
      border-color: var(--color-primary);
    }
    .git-history-tree-search-input {
      width: 0;
      height: 30px;
      flex: 1;
      border: 0;
      outline: 0;
      box-shadow: none;
      background: transparent;
      color: var(--color-text);
      font: inherit;
      font-size: var(--font-size-xs);
    }
    .git-history-tree-match-count {
      flex: none;
      font-size: var(--font-size-xs);
    }
  }

  .git-history-tree-notice {
    padding: 12px;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .git-history-tree-viewport {
    min-height: 0;
    flex: 1;
    overflow: auto;
    .git-history-tree-content {
      position: relative;
      .git-history-tree-row {
        position: absolute;
        display: flex;
        width: 100%;
        height: 28px;
        align-items: center;
        gap: 5px;
        padding: 0 10px;
        border: 0;
        background: transparent;
        color: var(--color-text);
        font: inherit;
        font-size: var(--font-size-sm);
        text-align: left;
        cursor: pointer;
        &:hover {
          background: var(--color-panel-soft);
        }
        &.git-history-tree-row-selected {
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }
        &.git-history-tree-row-located {
          box-shadow: inset 3px 0 var(--color-warning);
          background: var(--color-warning-soft);
        }
        .git-history-tree-chevron-space {
          width: 12px;
          flex: none;
        }
        .git-history-tree-name {
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }
    }
  }
  .git-history-tree-footer {
    padding: 10px;
    border-top: 1px solid var(--color-line);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }
}
</style>
