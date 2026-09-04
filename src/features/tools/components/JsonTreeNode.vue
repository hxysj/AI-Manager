<template>
  <div class="json-tree-node" :class="{ 'json-tree-node-root': isRoot }">
    <div
      class="json-tree-row"
      :class="{ 'json-tree-row-match': isMatch }"
      :style="{ paddingLeft: `${depth * 18}px` }"
    >
      <button
        v-if="isContainer"
        class="json-tree-toggle"
        type="button"
        :title="isExpanded ? '收起' : '展开'"
        :aria-label="isExpanded ? '收起节点' : '展开节点'"
        @click="toggleNode"
      >
        <ChevronDown v-if="isExpanded" :size="14" />
        <ChevronRight v-else :size="14" />
      </button>
      <span v-else class="json-tree-toggle-spacer"></span>

      <template v-if="isRoot && editing !== 'value' && !isContainer">
        <span class="json-tree-type-label">{{ rootLabel }}</span>
      </template>
      <template v-else-if="!isRoot">
        <input
          v-if="editing === 'key'"
          ref="keyInput"
          v-model="keyDraft"
          class="json-tree-edit-input json-tree-key-input"
          type="text"
          @keydown.enter.prevent="saveKey"
          @keydown.esc.prevent="cancelEdit"
          @blur="saveKey"
        />
        <span
          v-else
          class="json-tree-key"
          :title="`编辑键名：${nodeKey}`"
          @dblclick="startKeyEdit"
          >{{ nodeKey }}</span
        >
        <span class="json-tree-colon">:</span>
      </template>

      <template v-if="isContainer && editing === 'value'">
        <input
          ref="valueInput"
          v-model="valueDraft"
          class="json-tree-edit-input json-tree-value-input"
          type="text"
          @keydown.enter.prevent="saveValue"
          @keydown.esc.prevent="cancelEdit"
          @blur="saveValue"
        />
        <button
          class="json-tree-inline-control"
          type="button"
          title="保存值"
          @mousedown.prevent
          @click="saveValue"
        >
          <Check :size="13" />
        </button>
        <button
          class="json-tree-inline-control"
          type="button"
          title="取消编辑"
          @mousedown.prevent
          @click="cancelEdit"
        >
          <X :size="13" />
        </button>
      </template>
      <template v-else-if="isContainer">
        <span class="json-tree-container-label">{{ containerLabel }}</span>
        <span v-if="!isExpanded && !isRoot && !isLast" class="json-tree-comma"
          >,</span
        >
      </template>
      <template v-else-if="editing === 'value'">
        <input
          ref="valueInput"
          v-model="valueDraft"
          class="json-tree-edit-input json-tree-value-input"
          type="text"
          @keydown.enter.prevent="saveValue"
          @keydown.esc.prevent="cancelEdit"
          @blur="saveValue"
        />
        <button
          class="json-tree-inline-control"
          type="button"
          title="保存值"
          @mousedown.prevent
          @click="saveValue"
        >
          <Check :size="13" />
        </button>
        <button
          class="json-tree-inline-control"
          type="button"
          title="取消编辑"
          @mousedown.prevent
          @click="cancelEdit"
        >
          <X :size="13" />
        </button>
      </template>
      <span
        v-else
        class="json-tree-value"
        :class="`json-tree-value-${valueType}`"
        :title="`编辑值：${valueText}`"
        @dblclick="startValueEdit"
        >{{ valueText }}</span
      >
      <span v-if="isContainer && isExpanded" class="json-tree-brace">{{
        openingBrace
      }}</span>
      <span v-if="!isContainer && !isRoot && !isLast" class="json-tree-comma"
        >,</span
      >

      <span class="json-tree-row-actions">
        <button
          v-if="!isRoot && editing !== 'key'"
          class="json-tree-inline-control"
          type="button"
          title="编辑键名"
          @click="startKeyEdit"
        >
          <Pencil :size="12" />
        </button>
        <button
          v-if="editing !== 'value'"
          class="json-tree-inline-control"
          type="button"
          title="编辑值"
          @click="startValueEdit"
        >
          <Pencil :size="12" />
        </button>
        <button
          class="json-tree-inline-control"
          type="button"
          title="复制节点"
          @click="emitCopy"
        >
          <Copy :size="12" />
        </button>
      </span>
    </div>

    <div v-if="isContainer && isExpanded" class="json-tree-children">
      <JsonTreeNode
        v-for="entry in entries"
        :key="entry.pathKey"
        :node-key="entry.key"
        :value="entry.value"
        :path="entry.path"
        :depth="depth + 1"
        :is-last="entry.isLast"
        :is-array-item="entry.isArrayItem"
        :expanded-paths="expandedPaths"
        :search-query="searchQuery"
        :match-paths="matchPaths"
        @toggle="emit('toggle', $event)"
        @update-value="emit('update-value', $event)"
        @rename-key="emit('rename-key', $event)"
        @copy="emit('copy', $event)"
      />
      <div
        class="json-tree-closing"
        :style="{ paddingLeft: `${(depth + 1) * 18}px` }"
      >
        {{ closingBrace
        }}<span v-if="!isRoot && !isLast" class="json-tree-comma">,</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, nextTick, ref } from "vue"
import {
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Pencil,
  X
} from "lucide-vue-next"

const props = defineProps({
  nodeKey: {
    type: [String, Number],
    default: null
  },
  value: {
    required: true
  },
  path: {
    type: Array,
    default: () => []
  },
  depth: {
    type: Number,
    default: 0
  },
  expandedPaths: {
    type: Object,
    required: true
  },
  searchQuery: {
    type: String,
    default: ""
  },
  matchPaths: {
    type: Object,
    required: true
  },
  isRoot: {
    type: Boolean,
    default: false
  },
  isLast: {
    type: Boolean,
    default: false
  },
  isArrayItem: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits(["toggle", "update-value", "rename-key", "copy"])
const keyInput = ref(null)
const valueInput = ref(null)
const editing = ref("")
const keyDraft = ref("")
const valueDraft = ref("")

const isContainer = computed(
  () => props.value !== null && typeof props.value === "object"
)
const isArray = computed(() => Array.isArray(props.value))
const isExpanded = computed(() => props.expandedPaths.has(pathKey.value))
const pathKey = computed(() => JSON.stringify(props.path))
const isMatch = computed(() => props.matchPaths.has(pathKey.value))
const valueType = computed(() => {
  if (props.value === null) {
    return "null"
  }

  return typeof props.value
})
const valueText = computed(() => {
  if (props.value === null) {
    return "null"
  }

  if (typeof props.value === "string") {
    return JSON.stringify(props.value)
  }

  return String(props.value)
})
const entries = computed(() =>
  isArray.value
    ? props.value.map((value, key) => ({
        key,
        value,
        path: [...props.path, key],
        pathKey: JSON.stringify([...props.path, key]),
        isLast: key === props.value.length - 1,
        isArrayItem: true
      }))
    : Object.keys(props.value).map((key, index, keys) => ({
        key,
        value: props.value[key],
        path: [...props.path, key],
        pathKey: JSON.stringify([...props.path, key]),
        isLast: index === keys.length - 1,
        isArrayItem: false
      }))
)
const rootLabel = computed(() => {
  if (!isContainer.value) {
    return "Value"
  }

  return isArray.value ? "Array" : "Object"
})
const containerLabel = computed(() => {
  if (isArray.value) {
    return isExpanded.value ? "Array" : `Array[${props.value.length}]`
  }

  return isExpanded.value ? "Object" : "Object{...}"
})
const openingBrace = computed(() => (isArray.value ? "[" : "{"))
const closingBrace = computed(() => (isArray.value ? "]" : "}"))

function toggleNode() {
  emit("toggle", props.path)
}

async function startKeyEdit() {
  if (props.isRoot || props.isArrayItem) {
    return
  }

  editing.value = "key"
  keyDraft.value = String(props.nodeKey)
  await nextTick()
  keyInput.value?.focus()
  keyInput.value?.select()
}

async function startValueEdit() {
  editing.value = "value"
  valueDraft.value =
    typeof props.value === "string" ? props.value : JSON.stringify(props.value)
  await nextTick()
  valueInput.value?.focus()
  valueInput.value?.select()
}

function cancelEdit() {
  editing.value = ""
}

function saveKey() {
  if (editing.value !== "key") {
    return
  }

  const nextKey = keyDraft.value.trim()

  if (nextKey && nextKey !== String(props.nodeKey)) {
    emit("rename-key", {
      path: props.path,
      nextKey
    })
  }

  editing.value = ""
}

function saveValue() {
  if (editing.value !== "value") {
    return
  }

  // 字符串保留原文，其它 JSON 类型按类型解析后再提交。
  const draft = valueDraft.value
  let nextValue = draft

  try {
    if (typeof props.value !== "string") {
      nextValue = JSON.parse(draft)
    }
  } catch {
    return
  }

  emit("update-value", {
    path: props.path,
    value: nextValue
  })
  editing.value = ""
}

function emitCopy() {
  emit("copy", {
    path: props.path,
    value: props.value,
    nodeKey: props.nodeKey
  })
}
</script>

<style scoped lang="less">
.json-tree-node {
  .json-tree-row {
    position: relative;
    display: flex;
    min-height: 29px;
    align-items: center;
    gap: 4px;
    padding-top: 2px;
    padding-right: 7px;
    padding-bottom: 2px;
    border-left: 2px solid transparent;
    color: var(--color-text);
    font-family: Consolas, "Courier New", monospace;
    font-size: 0.78rem;
    line-height: 1.45;
    white-space: nowrap;
  }

  .json-tree-row:hover {
    background: var(--color-primary-soft);

    .json-tree-row-actions {
      opacity: 1;
    }
  }

  .json-tree-row-match {
    border-left-color: var(--color-search-match-line);
    background: var(--color-search-match);
  }

  .json-tree-toggle,
  .json-tree-inline-control {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .json-tree-toggle {
    width: 17px;
    height: 22px;
    flex: 0 0 17px;
    padding: 0;
  }

  .json-tree-toggle:hover,
  .json-tree-inline-control:hover {
    color: var(--color-primary);
  }

  .json-tree-toggle-spacer {
    width: 17px;
    flex: 0 0 17px;
  }

  .json-tree-key {
    color: var(--color-json-key);
    cursor: text;
    font-weight: 700;
  }

  .json-tree-colon {
    color: var(--color-text-soft);
  }

  .json-tree-type-label,
  .json-tree-container-label {
    color: var(--color-primary);
    font-weight: 700;
  }

  .json-tree-value {
    overflow: hidden;
    color: var(--color-json-string);
    text-overflow: ellipsis;
    cursor: text;
  }

  .json-tree-value-number {
    color: var(--color-json-number);
  }

  .json-tree-value-boolean {
    color: var(--color-json-boolean);
  }

  .json-tree-value-null {
    color: var(--color-json-null);
  }

  .json-tree-brace,
  .json-tree-closing,
  .json-tree-comma {
    color: var(--color-text-muted);
  }

  .json-tree-edit-input {
    min-width: 54px;
    height: 24px;
    border: 1px solid var(--color-primary);
    border-radius: 4px;
    outline: 0;
    padding: 1px 5px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
  }

  .json-tree-key-input {
    width: 150px;
  }

  .json-tree-value-input {
    min-width: 180px;
    flex: 1;
  }

  .json-tree-inline-control {
    width: 23px;
    height: 23px;
    flex: 0 0 23px;
    padding: 0;
    border-radius: 4px;
  }

  .json-tree-row-actions {
    display: inline-flex;
    margin-left: auto;
    align-items: center;
    gap: 1px;
    opacity: 0;
  }

  .json-tree-closing {
    min-height: 24px;
    padding-top: 2px;
    color: var(--color-text-muted);
    font-family: Consolas, "Courier New", monospace;
    font-size: 0.78rem;
  }

  .json-tree-children {
    border-left: 1px solid var(--color-line);
  }
}
</style>
