<template>
  <section class="translation-settings">
    <div class="translation-config">
      <header class="translation-header">
        <div class="translation-heading">
          <span data-emphasis>划词翻译</span>
          <span class="translation-hint"
            >启用后，在软件内选中文字，右键点击「翻译」。修改后点击上方「保存设置」。</span
          >
        </div>
        <button class="translation-back" type="button" @click="$emit('back')">
          返回功能扩展
        </button>
      </header>
      <div class="translation-fields">
        <label class="translation-field">
          <span>Provider / 官方账号</span>
          <el-select
            :model-value="modelValue.targetId"
            placeholder="请选择 Codex Provider 或账号"
            filterable
            @update:model-value="selectTarget"
          >
            <el-option-group
              v-for="group in targetGroups"
              :key="group.label"
              :label="group.label"
            >
              <el-option
                v-for="item in group.items"
                :key="item.id"
                :label="item.name"
                :value="item.id"
                :disabled="item.disabled"
              />
            </el-option-group>
          </el-select>
        </label>
        <label class="translation-field">
          <span>翻译模型</span>
          <el-select
            :model-value="modelValue.model"
            placeholder="选择或输入模型名称"
            filterable
            allow-create
            default-first-option
            :disabled="!modelValue.targetId"
            @update:model-value="update('model', $event)"
          >
            <el-option
              v-for="model in modelOptions"
              :key="model"
              :label="model"
              :value="model"
            />
          </el-select>
        </label>
        <label class="translation-field">
          <span>目标语言</span>
          <el-select
            :model-value="modelValue.targetLanguage"
            @update:model-value="update('targetLanguage', $event)"
          >
            <el-option
              v-for="language in languages"
              :key="language"
              :label="language"
              :value="language"
            />
          </el-select>
        </label>
      </div>
      <p class="translation-hint">
        目前仅支持 Codex 模式的 Provider 和官方账号。{{
          official
            ? "官方账号翻译需要已安装 Codex CLI。"
            : "Provider 翻译需要已安装 Node.js 20 或更高版本。"
        }}
      </p>
      <p v-if="invalidTarget" class="translation-error">
        所选 Provider / 账号已删除或禁用，请重新选择。
      </p>
    </div>

    <div class="translation-records">
      <header class="translation-header">
        <div class="translation-tabs">
          <button
            v-for="tab in tabs"
            :key="tab.id"
            type="button"
            :class="[
              'translation-tab',
              { 'translation-tab-active': kind === tab.id }
            ]"
            @click="changeTab(tab.id)"
          >
            {{ tab.label }}
          </button>
        </div>
        <button
          class="translation-button"
          type="button"
          :disabled="loading"
          @click="loadRecords"
        >
          {{ loading ? "刷新中…" : "刷新" }}
        </button>
      </header>
      <div class="translation-summary">
        <span>请求 {{ summary.requestCount || 0 }} 次</span>
        <span>输入 <TokenCount :value="summary.inputTokens" /></span>
        <span>输出 <TokenCount :value="summary.outputTokens" /></span>
        <span>缓存命中 <TokenCount :value="summary.cacheReadTokens" /></span>
        <span>推理 <TokenCount :value="summary.reasoningTokens" /></span>
      </div>
      <p class="translation-hint">
        缓存命中包含在输入中，推理包含在输出中。<template
          v-if="summary.unknownUsageCount"
          >{{ summary.unknownUsageCount }} 次请求未返回用量，未计入 token
          合计。</template
        >
      </p>
      <p v-if="errorMessage" class="translation-error">{{ errorMessage }}</p>
      <div v-if="!items.length && !loading" class="translation-empty">
        暂无{{ kind === "history" ? "翻译历史" : "消耗记录" }}
      </div>
      <div class="translation-list">
        <details
          v-for="item in items"
          :key="item.id"
          class="translation-record"
        >
          <summary class="translation-record-head">
            <span class="translation-record-title">{{
              kind === "history" ? item.sourceText : item.model
            }}</span>
            <span class="translation-record-meta"
              >{{ formatDateTime(item.createdAt) }} ·
              {{ statusLabels[item.status] || item.status }}</span
            >
          </summary>
          <div class="translation-record-body">
            <span class="translation-record-meta"
              >{{ item.providerName }} · {{ item.model }} ·
              {{ item.engine === "codex" ? "Codex" : "LangChain" }} ·
              {{ item.durationMs || 0 }} ms</span
            >
            <div v-if="kind === 'history'" class="translation-texts">
              <span class="translation-hint"
                >原文 → {{ item.targetLanguage }}</span
              >
              <pre class="translation-text">{{ item.sourceText }}</pre>
              <pre
                v-if="item.translatedText"
                class="translation-text translation-result"
                >{{ item.translatedText }}</pre
              >
            </div>
            <p v-if="item.errorMessage" class="translation-error">
              {{ item.errorMessage }}
            </p>
            <div class="translation-record-meta">
              <template v-if="item.usageKnown"
                >输入 {{ item.inputTokens }} · 输出 {{ item.outputTokens }} ·
                缓存命中 {{ item.cacheReadTokens }} · 推理
                {{ item.reasoningTokens }}</template
              >
              <template v-else>未返回完整用量</template>
              <span v-if="kind === 'usage'">
                · HTTP {{ item.statusCode || "—" }} · 翻译记录
                {{ item.translationId }}</span
              >
            </div>
          </div>
        </details>
      </div>
      <footer class="translation-pagination">
        <span>共 {{ total }} 条 · 第 {{ page }} 页</span>
        <button
          class="translation-button"
          type="button"
          :disabled="page <= 1 || loading"
          @click="changePage(-1)"
        >
          上一页
        </button>
        <button
          class="translation-button"
          type="button"
          :disabled="page * 20 >= total || loading"
          @click="changePage(1)"
        >
          下一页
        </button>
      </footer>
    </div>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from "vue"
import { translationApi } from "@/api"
import { ElOption, ElOptionGroup, ElSelect } from "element-plus"
import "element-plus/es/components/select/style/css"
import TokenCount from "@/components/TokenCount.vue"
import { formatDateTime } from "@/utils/formatters"

const props = defineProps({
  modelValue: { type: Object, required: true },
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] }
})
const emit = defineEmits(["update:modelValue", "back"])
const languages = [
  "简体中文",
  "繁體中文",
  "English",
  "日本語",
  "한국어",
  "Français",
  "Deutsch",
  "Español"
]
const tabs = [
  { id: "history", label: "翻译历史" },
  { id: "usage", label: "消耗记录" }
]
const statusLabels = {
  running: "进行中",
  success: "成功",
  failed: "失败",
  interrupted: "已中断"
}
const kind = ref("history")
const items = ref([])
const summary = ref({})
const page = ref(1)
const total = ref(0)
const loading = ref(false)
const errorMessage = ref("")
let requestId = 0
let unsubscribe = null

// 账号与 Provider 使用后端已有的 account: 标识，明确限制为 Codex 模式。
const targetGroups = computed(() => [
  {
    label: "Codex Provider",
    items: props.providers
      .filter((item) => item.cli === "codex")
      .map((item) => ({
        id: item.id,
        name: item.name,
        disabled: item.enabled === false
      }))
  },
  {
    label: "Codex 官方账号",
    items: props.codexAccounts.map((item) => ({
      id: `account:${item.id}`,
      name: item.email || item.accountId || item.id,
      disabled: item.disabled === true
    }))
  }
])
const official = computed(() =>
  props.modelValue.targetId.startsWith("account:")
)
const selectedProvider = computed(() =>
  props.providers.find(
    (item) => item.id === props.modelValue.targetId && item.cli === "codex"
  )
)
const invalidTarget = computed(
  () =>
    props.modelValue.targetId &&
    !targetGroups.value.some((group) =>
      group.items.some(
        (item) => item.id === props.modelValue.targetId && !item.disabled
      )
    )
)
const modelOptions = computed(() => [
  ...new Set(
    [
      selectedProvider.value?.runtimeConfig?.mainModel,
      ...props.runtimeModels
        .filter(
          (item) =>
            item.providerId === props.modelValue.targetId ||
            (official.value &&
              props.providers.some(
                (provider) =>
                  provider.id === item.providerId && provider.cli === "codex"
              ))
        )
        .map((item) => item.name || item.id),
      props.modelValue.model
    ].filter(Boolean)
  )
])

function update(key, value) {
  emit("update:modelValue", { ...props.modelValue, [key]: value })
}

function selectTarget(targetId) {
  const provider = props.providers.find((item) => item.id === targetId)
  const model = provider?.runtimeConfig?.mainModel || ""
  emit("update:modelValue", { ...props.modelValue, targetId, model })
}

async function loadRecords() {
  const current = ++requestId
  loading.value = true
  errorMessage.value = ""
  try {
    const result = await translationApi.list({
      kind: kind.value,
      page: page.value
    })
    if (current !== requestId) return
    items.value = result.items
    total.value = result.total
    summary.value = result.summary
  } catch (error) {
    if (current === requestId)
      errorMessage.value = error.message || String(error)
  } finally {
    if (current === requestId) loading.value = false
  }
}

function changeTab(value) {
  kind.value = value
  page.value = 1
  items.value = []
  loadRecords()
}

function changePage(offset) {
  page.value += offset
  loadRecords()
}

onMounted(() => {
  loadRecords()
  unsubscribe = translationApi.onChanged(loadRecords)
})
onBeforeUnmount(() => {
  requestId += 1
  unsubscribe?.()
})
</script>

<style scoped lang="less">
.translation-settings {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
  .translation-config,
  .translation-records {
    padding: 20px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    .translation-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      .translation-heading {
        display: flex;
        flex-direction: column;
        gap: 8px;
      }
      .translation-back {
        flex: none;
        padding: 6px 10px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
      }
      .translation-tabs {
        display: flex;
        gap: 8px;
        .translation-tab {
          padding: 7px 12px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel);
          color: var(--color-text-muted);
          cursor: pointer;
          &.translation-tab-active {
            color: var(--color-primary);
            background: var(--color-panel-soft);
          }
        }
      }
    }
    .translation-fields {
      display: flex;
      gap: 14px;
      margin: 20px 0 12px;
      .translation-field {
        display: flex;
        flex: 1;
        min-width: 0;
        flex-direction: column;
        gap: 8px;
      }
    }
    .translation-hint {
      margin: 8px 0;
      color: var(--color-text-muted);
      font-size: var(--font-size-sm);
      line-height: 1.6;
    }
    .translation-error {
      color: var(--color-danger);
      overflow-wrap: anywhere;
    }
    .translation-summary {
      display: flex;
      flex-wrap: wrap;
      gap: 16px;
      margin-top: 18px;
    }
    .translation-empty {
      padding: 30px;
      text-align: center;
      color: var(--color-text-muted);
    }
    .translation-list {
      display: flex;
      flex-direction: column;
      gap: 8px;
      .translation-record {
        border: 1px solid var(--color-line);
        border-radius: 6px;
        overflow: hidden;
        .translation-record-head {
          display: flex;
          align-items: center;
          gap: 12px;
          padding: 12px;
          cursor: pointer;
          background: var(--color-panel-soft);
          .translation-record-title {
            flex: 1;
            min-width: 0;
            overflow: hidden;
            white-space: nowrap;
            text-overflow: ellipsis;
          }
        }
        .translation-record-meta {
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
          overflow-wrap: anywhere;
        }
        .translation-record-body {
          display: flex;
          flex-direction: column;
          gap: 12px;
          padding: 14px;
          .translation-texts {
            .translation-text {
              margin: 8px 0;
              white-space: pre-wrap;
              overflow-wrap: anywhere;
              font-family: inherit;
              line-height: 1.7;
            }
            .translation-result {
              border-top: 1px solid var(--color-line);
              padding-top: 12px;
            }
          }
        }
      }
    }
    .translation-button {
      padding: 6px 12px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel);
      color: var(--color-text);
      cursor: pointer;
      &:disabled {
        opacity: 0.5;
        cursor: default;
      }
    }
    .translation-pagination {
      display: flex;
      justify-content: flex-end;
      align-items: center;
      gap: 10px;
      margin-top: 14px;
      color: var(--color-text-muted);
    }
  }
}
</style>
