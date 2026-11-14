<template>
  <div class="markdown-content" @click="openLink" v-html="html"></div>
</template>

<script setup>
import { computed } from "vue"
import { micromark } from "micromark"
import { gfm, gfmHtml } from "micromark-extension-gfm"
import { systemApi } from "@/api"
import { createMessage } from "@/utils/message"

const props = defineProps({ text: { type: String, default: "" } })
// 保持解析器默认的 HTML 转义和危险协议过滤，流式文本也走同一渲染路径。
const html = computed(() =>
  micromark(props.text, {
    extensions: [gfm()],
    htmlExtensions: [gfmHtml()]
  })
)
async function openLink(event) {
  const link = event.target.closest?.("a")
  if (!link) return
  event.preventDefault()
  const url = link.getAttribute("href")
  if (!/^https?:\/\//i.test(url || "")) return
  try {
    await systemApi.openExternal({ url })
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}
</script>

<style scoped lang="less">
.markdown-content {
  min-width: 0;
  overflow-wrap: anywhere;
  white-space: normal;
  line-height: 1.75;
  :deep(> :first-child) {
    margin-top: 0;
  }
  :deep(> :last-child) {
    margin-bottom: 0;
  }
  :deep(p) {
    margin: 0.65em 0;
    white-space: pre-wrap;
  }
  :deep(h1),
  :deep(h2),
  :deep(h3),
  :deep(h4) {
    margin: 1em 0 0.5em;
    line-height: 1.4;
  }
  :deep(h1) {
    font-size: 1.5em;
  }
  :deep(h2) {
    font-size: 1.3em;
  }
  :deep(h3),
  :deep(h4) {
    font-size: 1.1em;
  }
  :deep(ul),
  :deep(ol) {
    margin: 0.6em 0;
    padding-left: 1.6em;
  }
  :deep(li) {
    margin: 0.25em 0;
  }
  :deep(blockquote) {
    margin: 0.8em 0;
    padding: 0.1em 1em;
    border-left: 3px solid var(--color-primary);
    color: var(--color-text-muted);
    background: var(--color-panel-soft);
  }
  :deep(code) {
    padding: 0.1em 0.3em;
    border-radius: 4px;
    background: var(--color-panel-soft);
    font-size: 0.9em;
  }
  :deep(pre) {
    max-width: 100%;
    overflow: auto;
    margin: 0.8em 0;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
    white-space: pre;
  }
  :deep(pre code) {
    padding: 0;
    background: transparent;
  }
  :deep(table) {
    display: block;
    max-width: 100%;
    overflow: auto;
    border-collapse: collapse;
    font-size: 0.9em;
  }
  :deep(th),
  :deep(td) {
    padding: 6px 10px;
    border: 1px solid var(--color-line);
  }
  :deep(a) {
    color: var(--color-primary);
    text-underline-offset: 3px;
  }
  :deep(img) {
    max-width: 100%;
    height: auto;
  }
  :deep(hr) {
    border: 0;
    border-top: 1px solid var(--color-line);
  }
}
</style>
