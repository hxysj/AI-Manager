<template>
  <div class="note-body-editor">
    <div class="body-toolbar" aria-label="正文工具栏">
      <select
        class="body-format"
        aria-label="段落格式"
        @change="formatParagraph"
      >
        <option value="">正文</option>
        <option value="## ">标题</option>
        <option value="### ">小标题</option>
      </select>
      <span class="body-divider"></span>
      <button
        v-for="item in formats"
        :key="item.label"
        class="body-tool"
        type="button"
        :aria-label="item.label"
        :title="item.label"
        @click="insert(item.before, item.after)"
      >
        <component :is="item.icon" :size="15" />
      </button>
      <span class="body-divider"></span>
      <button
        class="body-tool body-preview"
        type="button"
        :aria-pressed="preview"
        @click="preview = !preview"
      >
        <Eye :size="15" />{{ preview ? "编辑" : "预览" }}
      </button>
    </div>
    <MarkdownContent
      v-if="preview"
      class="body-rendered"
      :text="modelValue || '正文尚未填写。'"
    />
    <textarea
      v-else
      ref="input"
      class="body-input"
      :value="modelValue"
      :placeholder="placeholder"
      :maxlength="100000"
      aria-label="便签正文"
      @input="$emit('update:modelValue', $event.target.value)"
    ></textarea>
    <div class="body-count">Markdown · 共 {{ modelValue.length }} 字</div>
  </div>
</template>

<script setup>
import { nextTick, ref } from "vue"
import {
  Bold,
  Italic,
  Strikethrough,
  List,
  ListOrdered,
  ListTodo,
  Link,
  Code,
  Quote,
  Eye
} from "lucide-vue-next"
import MarkdownContent from "@/components/MarkdownContent.vue"
defineProps({
  modelValue: { type: String, default: "" },
  placeholder: { type: String, default: "记录你的想法、任务或工作进展…" }
})
const emit = defineEmits(["update:modelValue"])
const input = ref(null)
const preview = ref(false)
const formats = [
  { label: "加粗", icon: Bold, before: "**", after: "**" },
  { label: "斜体", icon: Italic, before: "*", after: "*" },
  { label: "删除线", icon: Strikethrough, before: "~~", after: "~~" },
  { label: "无序列表", icon: List, before: "\n- ", after: "" },
  { label: "有序列表", icon: ListOrdered, before: "\n1. ", after: "" },
  { label: "待办清单", icon: ListTodo, before: "\n- [ ] ", after: "" },
  { label: "插入链接", icon: Link, before: "[", after: "](https://)" },
  { label: "代码", icon: Code, before: "`", after: "`" },
  { label: "引用", icon: Quote, before: "\n> ", after: "" }
]
function formatParagraph(event) {
  insert(event.target.value, "")
  event.target.value = ""
}
// 使用文本选区插入 Markdown，保留原始正文与现有存储格式。
async function insert(before, after) {
  preview.value = false
  await nextTick()
  const element = input.value
  const start = element.selectionStart
  const end = element.selectionEnd
  emit(
    "update:modelValue",
    element.value.slice(0, start) +
      before +
      element.value.slice(start, end) +
      after +
      element.value.slice(end)
  )
  await nextTick()
  element.focus()
  element.setSelectionRange(start + before.length, end + before.length)
}
</script>

<style scoped lang="less">
.note-body-editor {
  display: flex;
  flex-direction: column;
  min-height: 220px;
  border: 1px solid var(--color-line);
  border-radius: 11px;
  background: var(--color-panel);
  overflow: hidden;
  .body-toolbar {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
    .body-tool,
    .body-format {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 5px;
      min-width: 27px;
      height: 27px;
      border: 0;
      border-radius: 5px;
      color: var(--color-text);
      background: transparent;
      font: inherit;
      font-size: 12px;
      cursor: pointer;
      &:hover {
        background: var(--color-primary-soft);
      }
    }
    .body-preview {
      margin-left: auto;
    }
    .body-divider {
      height: 15px;
      border-left: 1px solid var(--color-line);
      margin: 0 3px;
    }
  }
  .body-input {
    flex: 1;
    width: 100%;
    min-height: 180px;
    padding: 15px 17px;
    border: 0;
    outline: 0;
    resize: vertical;
    color: var(--color-text);
    background: transparent;
    font: inherit;
    font-size: 13px;
    line-height: 1.9;
    box-sizing: border-box;
  }
  .body-rendered {
    flex: 1;
    padding: 15px 17px;
    min-height: 175px;
    overflow: auto;
  }
  .body-count {
    padding: 0 13px 9px;
    color: var(--color-text-soft);
    font-size: 11px;
    text-align: right;
  }
}
</style>
