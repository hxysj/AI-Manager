<template>
  <div class="gateway-connection">
    <section class="protocol-panel">
      <header class="protocol-heading">
        <div class="protocol-title">
          <span class="heading-icon"><Code2 :size="20" /></span>
          <div class="heading-copy">
            <h3 class="heading-name">多协议支持</h3>
            <span class="heading-caption">Multi-Protocol Support</span>
          </div>
        </div>
        <span class="service-status" :class="{ active: enabled }">
          <span class="status-dot"></span>
          {{ enabled ? "服务运行中" : "服务未开启" }}
        </span>
      </header>

      <div class="protocol-cards">
        <article
          v-for="protocol in protocols"
          :key="protocol.id"
          class="protocol-card"
          :class="[protocol.id, { selected: selectedProtocol === protocol.id }]"
        >
          <div class="card-header">
            <button
              class="protocol-picker"
              type="button"
              :aria-pressed="selectedProtocol === protocol.id"
              @click="selectEndpoint(protocol)"
            >
              <span class="protocol-mark"><Code2 :size="16" /></span>
              <div class="protocol-name-wrap">
                <span class="protocol-title-text">{{ protocol.name }}</span>
                <span class="protocol-desc-text">{{ protocol.desc }}</span>
              </div>
            </button>
          </div>

          <div class="base-row">
            <span class="base-label">BASE 地址</span>
            <div class="base-box">
              <code class="base-address"
                >{{ base }}{{ protocol.basePath }}</code
              >
              <button
                class="base-copy"
                type="button"
                :aria-label="`复制 ${protocol.name} BASE`"
                @click="copyText(`${base}${protocol.basePath}`)"
              >
                <Copy :size="12" />复制
              </button>
            </div>
          </div>

          <div class="endpoints-section">
            <span class="endpoints-title">主要接口 (示例)</span>
            <div class="endpoint-list">
              <div
                v-for="endpoint in protocol.endpoints"
                :key="endpoint.id"
                class="endpoint-item"
              >
                <button
                  class="endpoint-picker"
                  :class="{
                    active:
                      selectedProtocol === protocol.id &&
                      activeEndpoint.id === endpoint.id
                  }"
                  type="button"
                  :aria-pressed="
                    selectedProtocol === protocol.id &&
                    activeEndpoint.id === endpoint.id
                  "
                  @click="selectEndpoint(protocol, endpoint)"
                >
                  <span class="endpoint-heading">
                    <span class="method-tag">POST</span>
                    <span v-if="endpoint.label" class="endpoint-badge">{{
                      endpoint.label
                    }}</span>
                  </span>
                  <code class="endpoint-path">{{ endpoint.path }}</code>
                </button>
                <button
                  class="endpoint-copy"
                  type="button"
                  :aria-label="`复制 ${endpoint.name} 地址`"
                  @click="copyText(endpointUrl(endpoint))"
                >
                  <Copy :size="13" />
                </button>
              </div>
            </div>
          </div>
        </article>
      </div>

      <footer class="protocol-footer">
        <p class="auth-note">
          <KeyRound :size="13" />所有接口均需调用 Token；支持
          Bearer、x-api-key、x-goog-api-key 鉴权请求头。
        </p>
        <button class="token-link" type="button" @click="emit('manage-tokens')">
          管理调用 Token<ArrowUpRight :size="13" />
        </button>
      </footer>
    </section>

    <section class="integration-panel">
      <header class="integration-heading">
        <div class="integration-title">
          <Boxes :size="18" />
          <h3 class="heading-name">支持模型与集成</h3>
          <span class="model-count">{{ modelEntries.length }} 个模型</span>
        </div>
        <button class="models-copy" type="button" @click="copyText(modelsUrl)">
          <Copy :size="13" />复制模型列表接口
        </button>
      </header>

      <div class="integration-body">
        <div class="model-browser">
          <div class="model-browser-heading">
            <span class="browser-caption"
              >模型来自顶部「底层池」中所选 Provider
              或官方账号的已有配置。</span
            >
            <code class="models-path">GET {{ modelsPath }}</code>
          </div>

          <!-- 筛选服务商与模型搜索栏 -->
          <div v-if="modelEntries.length > 0" class="model-filter-bar">
            <div class="search-input-wrap">
              <Search :size="13" class="filter-icon" />
              <input
                v-model.trim="modelSearch"
                class="model-search-input"
                placeholder="搜索模型名称或 ID..."
              />
            </div>
            <div class="target-select-wrap">
              <select v-model="selectedTargetFilter" class="target-select">
                <option value="all">
                  全部服务商 ({{ modelEntries.length }})
                </option>
                <option
                  v-for="target in providerFilterOptions"
                  :key="target.id"
                  :value="target.id"
                >
                  {{ target.name }} ({{ target.count }})
                </option>
              </select>
            </div>
          </div>

          <div v-if="paginatedModelEntries.length > 0" class="model-table-wrap">
            <table class="model-table">
              <thead class="model-head">
                <tr class="model-heading-row">
                  <th class="table-heading">模型名称</th>
                  <th class="table-heading">模型 ID</th>
                  <th class="table-heading">路由目标</th>
                  <th class="table-heading action-heading">操作</th>
                </tr>
              </thead>
              <tbody class="model-body">
                <tr
                  v-for="model in paginatedModelEntries"
                  :key="model.id"
                  class="model-row"
                  :class="{ selected: modelId === model.id }"
                  @click="selectedModelId = model.id"
                >
                  <td class="model-cell">
                    <button
                      class="model-picker"
                      type="button"
                      :aria-pressed="modelId === model.id"
                      @click="selectedModelId = model.id"
                    >
                      <span class="model-symbol"><Box :size="14" /></span>
                      <span class="model-name">{{ model.name }}</span>
                    </button>
                  </td>
                  <td class="model-cell">
                    <code class="model-id">{{ model.id }}</code>
                  </td>
                  <td class="model-cell">
                    <span class="route-target">{{ model.targetName }}</span>
                    <code v-if="model.realModel" class="route-model"
                      >→ {{ model.realModel }}</code
                    >
                  </td>
                  <td class="model-cell action-cell">
                    <button
                      class="model-copy"
                      type="button"
                      :aria-label="`复制模型 ${model.id}`"
                      title="复制模型 ID"
                      @click.stop="copyText(model.id)"
                    >
                      <Copy :size="13" />
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div v-else-if="modelEntries.length > 0" class="filter-empty-models">
            <Boxes :size="24" class="empty-icon" />
            <p class="empty-title">未找到匹配的模型</p>
            <button
              class="reset-filter-btn"
              type="button"
              @click="resetModelFilters"
            >
              重置筛选条件
            </button>
          </div>

          <div v-else class="empty-models">
            <Boxes :size="32" class="empty-icon" />
            <p class="empty-title">暂时没有可用模型</p>
            <p class="empty-description">
              点击顶部「底层池」选择 Provider
              或官方账号，并确认所选来源已有可用模型。
            </p>
          </div>

          <!-- 模型分页设置栏 -->
          <div v-if="filteredModelEntries.length > 0" class="model-pagination">
            <span class="pagination-info"
              >共 {{ filteredModelEntries.length }} 个模型</span
            >
            <div class="pagination-actions">
              <select v-model.number="pageSize" class="page-size-select">
                <option :value="5">5 条/页</option>
                <option :value="10">10 条/页</option>
                <option :value="20">20 条/页</option>
              </select>
              <button
                class="page-nav-btn"
                type="button"
                :disabled="currentPage <= 1"
                @click="currentPage--"
              >
                &lt;
              </button>
              <span class="page-current"
                >{{ currentPage }} / {{ totalPages }}</span
              >
              <button
                class="page-nav-btn"
                type="button"
                :disabled="currentPage >= totalPages"
                @click="currentPage++"
              >
                &gt;
              </button>
            </div>
          </div>
        </div>

        <aside class="quick-integration">
          <header class="code-heading">
            <span class="code-title"><Terminal :size="15" />快速集成</span>
            <select
              v-model="language"
              class="language-select"
              aria-label="代码示例语言"
            >
              <option value="python">Python · 标准库</option>
              <option value="curl">cURL · Bash</option>
              <option value="powershell">PowerShell</option>
            </select>
          </header>

          <div class="code-toolbar">
            <span class="request-path">
              <span class="request-method">POST</span>{{ activeEndpoint.path }}
            </span>
            <button
              class="code-copy"
              type="button"
              :disabled="!modelId"
              @click="copyText(snippet)"
            >
              <Copy :size="13" />复制代码
            </button>
          </div>

          <div class="code-container">
            <div class="code-lines-col" aria-hidden="true">
              <span
                v-for="(_, idx) in snippetLines"
                :key="idx"
                class="line-number"
                >{{ idx + 1 }}</span
              >
            </div>
            <pre
              class="code-example"
            ><code class="code-content"><span v-for="(segment, index) in codeSegments" :key="index" :class="segment.kind">{{ segment.text }}</span></code></pre>
          </div>

          <footer class="code-footer">
            <KeyRound :size="13" />
            <span class="code-note">
              先将调用 Token 写入环境变量
              <code class="token-variable">AI_MANAGER_TOKEN</code
              >，示例会自动读取。
            </span>
          </footer>
        </aside>
      </div>
    </section>

    <p class="connection-note">
      <Info :size="14" />
      跨协议支持文本与函数工具调用；图片、可见思考及其他专属字段请使用同协议目标。请求需携带完整历史。
    </p>
  </div>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import {
  ArrowUpRight,
  Box,
  Boxes,
  Code2,
  Copy,
  Info,
  KeyRound,
  Search,
  Terminal
} from "lucide-vue-next"
import { createMessage } from "@/utils/message"

const props = defineProps({
  baseUrl: { type: String, default: "" },
  enabled: { type: Boolean, default: false },
  models: { type: Array, default: () => [] },
  modelMappings: { type: Object, default: () => ({}) },
  targets: { type: Array, default: () => [] }
})
const emit = defineEmits(["manage-tokens"])
const selectedProtocol = ref("openai")
const selectedEndpoint = ref("chat")
const selectedModelId = ref("")
const language = ref("python")

// 模型列表服务商筛选与分页设置
const selectedTargetFilter = ref("all")
const modelSearch = ref("")
const currentPage = ref(1)
const pageSize = ref(10)
const protocols = [
  {
    id: "openai",
    name: "OpenAI 协议",
    desc: "兼容 OpenAI API (如 openai-python、openai-js 等)。",
    basePath: "/v1",
    endpoints: [
      { id: "chat", name: "Chat Completions", path: "/v1/chat/completions" },
      {
        id: "responses",
        name: "Responses",
        path: "/v1/responses",
        label: "Codex"
      }
    ]
  },
  {
    id: "anthropic",
    name: "Anthropic 协议",
    desc: "兼容 Anthropic Messages API (即 Claude API)。",
    basePath: "",
    endpoints: [
      { id: "messages", name: "Anthropic Messages", path: "/v1/messages" }
    ]
  },
  {
    id: "gemini",
    name: "Gemini 协议",
    desc: "兼容 Gemini API (即 Google Generative Language API)。",
    basePath: "",
    endpoints: [
      {
        id: "generate",
        name: "Gemini Generate",
        path: "/v1beta/models/{model}:generateContent"
      },
      {
        id: "stream",
        name: "Gemini 流式",
        path: "/v1beta/models/{model}:streamGenerateContent?alt=sse",
        label: "SSE"
      }
    ]
  }
]
const base = computed(() =>
  (props.baseUrl || "http://127.0.0.1:8046").replace(/\/+$/, "")
)
const activeProtocol = computed(() =>
  protocols.find((protocol) => protocol.id === selectedProtocol.value)
)
const activeEndpoint = computed(
  () =>
    activeProtocol.value.endpoints.find(
      (endpoint) => endpoint.id === selectedEndpoint.value
    ) || activeProtocol.value.endpoints[0]
)
const modelEntries = computed(() =>
  props.models.map((model) => {
    const mapping = Object.hasOwn(props.modelMappings, model.id)
      ? props.modelMappings[model.id]
      : null
    return {
      id: model.id,
      name: model.displayName || model.name || model.id,
      targetName: mapping
        ? props.targets.find((target) => target.id === mapping.targetId)
            ?.name || mapping.targetId
        : model.owned_by || "按模型匹配",
      realModel: mapping?.model || ""
    }
  })
)
const providerFilterOptions = computed(() => {
  const map = new Map()
  for (const model of modelEntries.value) {
    const key = model.targetName || "其他"
    if (!map.has(key)) {
      map.set(key, { id: key, name: key, count: 0 })
    }
    map.get(key).count++
  }
  return Array.from(map.values())
})

const filteredModelEntries = computed(() => {
  let list = modelEntries.value
  if (selectedTargetFilter.value !== "all") {
    list = list.filter((m) => m.targetName === selectedTargetFilter.value)
  }
  if (modelSearch.value) {
    const q = modelSearch.value.toLowerCase()
    list = list.filter(
      (m) =>
        m.id.toLowerCase().includes(q) ||
        m.name.toLowerCase().includes(q) ||
        m.targetName.toLowerCase().includes(q)
    )
  }
  return list
})

const totalPages = computed(() =>
  Math.max(1, Math.ceil(filteredModelEntries.value.length / pageSize.value))
)

const paginatedModelEntries = computed(() => {
  const start = (currentPage.value - 1) * pageSize.value
  return filteredModelEntries.value.slice(start, start + pageSize.value)
})

function resetModelFilters() {
  selectedTargetFilter.value = "all"
  modelSearch.value = ""
  currentPage.value = 1
}

watch([selectedTargetFilter, modelSearch, pageSize], () => {
  currentPage.value = 1
})

const modelId = computed(
  () =>
    modelEntries.value.find((model) => model.id === selectedModelId.value)
      ?.id ||
    modelEntries.value[0]?.id ||
    ""
)
const modelsPath = computed(() =>
  selectedProtocol.value === "gemini" ? "/v1beta/models" : "/v1/models"
)
const modelsUrl = computed(() => `${base.value}${modelsPath.value}`)
const snippet = computed(() => {
  const protocol = selectedProtocol.value
  const endpoint = activeEndpoint.value.id
  const model = modelId.value || "YOUR_MODEL"
  const url = endpointUrl(activeEndpoint.value).replace("{model}", "YOUR_MODEL")
  const header =
    protocol === "openai"
      ? "Authorization"
      : protocol === "anthropic"
        ? "x-api-key"
        : "x-goog-api-key"
  const prefix = protocol === "openai" ? "Bearer " : ""
  const payload =
    protocol === "gemini"
      ? { contents: [{ role: "user", parts: [{ text: "你好" }] }] }
      : endpoint === "responses"
        ? { model, input: "你好" }
        : {
            model,
            ...(protocol === "anthropic" ? { max_tokens: 1024 } : {}),
            messages: [{ role: "user", content: "你好" }]
          }
  const body = JSON.stringify(payload, null, 2)
  if (language.value === "curl") {
    return [
      `curl${endpoint === "stream" ? " --no-buffer" : ""} '${url.replace(/'/g, "'\\''")}' \\`,
      "  -H 'Content-Type: application/json' \\",
      `  -H "${header}: ${prefix}\${AI_MANAGER_TOKEN}" \\`,
      `  --data-raw '${body.replace(/'/g, "'\\''")}'`
    ].join("\n")
  }
  if (language.value === "powershell") {
    return [
      `$body = @'\n${body}\n'@`,
      "$headers = @{",
      `  '${header}' = "${prefix}$env:AI_MANAGER_TOKEN"`,
      "}",
      "",
      `Invoke-RestMethod -Uri '${url.replace(/'/g, "''")}' \``,
      "  -Method Post `",
      "  -Headers $headers `",
      "  -ContentType 'application/json; charset=utf-8' `",
      "  -Body ([System.Text.Encoding]::UTF8.GetBytes($body))"
    ].join("\n")
  }
  return [
    "import json",
    "import os",
    "from urllib.request import Request, urlopen",
    "",
    `url = ${JSON.stringify(url)}`,
    'token = os.environ["AI_MANAGER_TOKEN"]',
    `payload = ${body}`,
    "",
    "request = Request(",
    "    url,",
    '    data=json.dumps(payload, ensure_ascii=False).encode("utf-8"),',
    "    headers={",
    '        "Content-Type": "application/json",',
    `        "${header}": f"${prefix}{token}"`,
    "    },",
    '    method="POST"',
    ")",
    "with urlopen(request) as response:",
    ...(endpoint === "stream"
      ? [
          "    for line in response:",
          '        print(line.decode("utf-8").rstrip(), flush=True)'
        ]
      : ['    print(response.read().decode("utf-8"))'])
  ].join("\n")
})
const snippetLines = computed(() => snippet.value.split("\n"))
const codeSegments = computed(() =>
  snippet.value
    .split(
      /("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|\b(?:import|from|with|as|for|in)\b|\b\d+\b)/g
    )
    .map((text) => ({
      text,
      kind: /^["']/.test(text)
        ? "code-string"
        : /^(import|from|with|as|for|in)$/.test(text)
          ? "code-keyword"
          : /^\d+$/.test(text)
            ? "code-number"
            : "code-plain"
    }))
)

function selectEndpoint(protocol, endpoint = protocol.endpoints[0]) {
  selectedProtocol.value = protocol.id
  selectedEndpoint.value = endpoint.id
}

function endpointUrl(endpoint) {
  return `${base.value}${endpoint.path.replace("{model}", modelId.value ? encodeURIComponent(modelId.value) : "{model}")}`
}

async function copyText(value) {
  try {
    await navigator.clipboard.writeText(value)
    createMessage.success("已复制")
  } catch (error) {
    createMessage.error(error.message || "复制失败")
  }
}
</script>

<style scoped lang="less">
.gateway-connection {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 16px;
  color: var(--color-text);

  .protocol-panel {
    padding: 20px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);

    .protocol-heading {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      margin-bottom: 18px;

      .protocol-title {
        display: flex;
        align-items: center;
        gap: 12px;

        .heading-icon {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 36px;
          height: 36px;
          border-radius: 9px;
          color: #7c3aed;
          background: rgba(124, 58, 237, 0.1);
        }
        .heading-copy {
          .heading-name {
            margin: 0 0 3px;
            font-size: 15px;
          }
          .heading-caption {
            color: var(--color-text-muted);
            font-size: 12px;
          }
        }
      }
      .service-status {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 4px 10px;
        border: 1px solid var(--color-line);
        border-radius: 20px;
        color: var(--color-text-muted);
        font-size: 11px;

        &.active {
          color: #10b981;
          border-color: rgba(16, 185, 129, 0.25);
          background: rgba(16, 185, 129, 0.08);
        }
        .status-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          background: currentColor;
        }
      }
    }

    .protocol-cards {
      display: flex;
      align-items: stretch;
      gap: 14px;

      .protocol-card {
        display: flex;
        flex: 1;
        min-width: 0;
        flex-direction: column;
        padding: 16px;
        border: 1px solid var(--color-line);
        border-radius: 10px;
        background: var(--color-panel-soft);
        transition:
          border-color 0.15s,
          box-shadow 0.15s;

        &.openai {
          --proto-color: #10b981;
          --proto-soft: rgba(16, 185, 129, 0.08);
        }
        &.anthropic {
          --proto-color: #d97706;
          --proto-soft: rgba(217, 119, 6, 0.08);
        }
        &.gemini {
          --proto-color: #2563eb;
          --proto-soft: rgba(37, 99, 235, 0.08);
        }
        &.selected {
          border-color: var(--proto-color);
          box-shadow: 0 0 0 1px var(--proto-color);
        }

        .card-header {
          margin-bottom: 12px;

          .protocol-picker {
            display: flex;
            align-items: flex-start;
            gap: 10px;
            width: 100%;
            padding: 0;
            border: 0;
            background: transparent;
            text-align: left;
            cursor: pointer;

            .protocol-mark {
              display: inline-flex;
              align-items: center;
              justify-content: center;
              padding: 7px;
              border-radius: 7px;
              color: var(--proto-color);
              background: var(--proto-soft);
            }
            .protocol-name-wrap {
              display: flex;
              flex-direction: column;
              gap: 2px;

              .protocol-title-text {
                font-size: 14px;
                color: var(--color-text);
              }
              .protocol-desc-text {
                font-size: 11px;
                color: var(--color-text-muted);
                line-height: 1.4;
              }
            }
          }
        }

        .base-row {
          display: flex;
          flex-direction: column;
          gap: 6px;
          margin-bottom: 14px;

          .base-label {
            font-size: 11px;
            color: var(--color-text-muted);
          }
          .base-box {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 6px 8px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel);

            .base-address {
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
              color: var(--color-text);
              font-size: 11px;
            }
            .base-copy {
              display: inline-flex;
              align-items: center;
              gap: 4px;
              padding: 3px 6px;
              border: 1px solid var(--color-line);
              border-radius: 4px;
              color: var(--color-text-muted);
              background: var(--color-panel-soft);
              font-size: 10px;
              cursor: pointer;

              &:hover {
                color: var(--proto-color);
                border-color: var(--proto-color);
              }
            }
          }
        }

        .endpoints-section {
          display: flex;
          flex-direction: column;
          gap: 7px;

          .endpoints-title {
            font-size: 11px;
            color: var(--color-text-muted);
          }
        }

        .endpoint-list {
          display: flex;
          flex-direction: column;
          gap: 6px;

          .endpoint-item {
            display: flex;
            align-items: center;
            gap: 6px;
            padding: 6px 8px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel);

            .endpoint-picker {
              display: flex;
              align-items: center;
              gap: 8px;
              flex: 1;
              min-width: 0;
              padding: 0;
              border: 0;
              background: transparent;
              text-align: left;
              cursor: pointer;

              .endpoint-heading {
                display: inline-flex;
                align-items: center;
                gap: 4px;

                .method-tag {
                  padding: 2px 5px;
                  border-radius: 4px;
                  color: #7c3aed;
                  background: rgba(124, 58, 237, 0.1);
                  font-size: 10px;
                }
                .endpoint-badge {
                  padding: 1px 4px;
                  border-radius: 3px;
                  color: var(--proto-color);
                  background: var(--proto-soft);
                  font-size: 9px;
                }
              }
              .endpoint-path {
                flex: 1;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
                color: var(--color-text);
                font-size: 11px;
              }
            }

            .endpoint-copy {
              display: inline-flex;
              align-items: center;
              justify-content: center;
              padding: 4px;
              border: 0;
              border-radius: 4px;
              color: var(--color-text-muted);
              background: transparent;
              cursor: pointer;

              &:hover {
                color: var(--proto-color);
                background: var(--proto-soft);
              }
            }
          }
        }
      }
    }

    .protocol-footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      margin-top: 16px;
      padding-top: 14px;
      border-top: 1px solid var(--color-line);

      .auth-note {
        display: flex;
        align-items: center;
        gap: 6px;
        margin: 0;
        color: var(--color-text-muted);
        font-size: 11px;
      }
      .token-link {
        display: inline-flex;
        align-items: center;
        gap: 4px;
        padding: 0;
        border: 0;
        color: #7c3aed;
        background: transparent;
        cursor: pointer;
        font-size: 12px;

        &:hover {
          text-decoration: underline;
        }
      }
    }
  }

  .integration-panel {
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);

    .integration-heading {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 14px;
      padding: 16px 20px;
      border-bottom: 1px solid var(--color-line);

      .integration-title {
        display: flex;
        align-items: center;
        gap: 8px;
        color: #7c3aed;

        .heading-name {
          margin: 0;
          color: var(--color-text);
          font-size: 15px;
        }
        .model-count {
          padding: 2px 7px;
          border-radius: 12px;
          color: var(--color-text-muted);
          background: var(--color-panel-soft);
          font-size: 11px;
        }
      }
      .models-copy {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 0;
        border: 0;
        background: transparent;
        color: var(--color-text-muted);
        font-size: 11px;
        cursor: pointer;

        &:hover {
          color: #7c3aed;
        }
      }
    }

    .integration-body {
      display: flex;
      align-items: stretch;

      .model-browser {
        flex: 1;
        min-width: 0;
        padding: 18px;

        .model-browser-heading {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 10px;
          margin-bottom: 14px;

          .browser-caption {
            color: var(--color-text-muted);
            font-size: 11px;
          }
          .models-path {
            color: var(--color-text-muted);
            font-size: 10px;
          }
        }

        .model-filter-bar {
          display: flex;
          align-items: center;
          gap: 10px;
          margin-bottom: 12px;

          .search-input-wrap {
            position: relative;
            display: flex;
            align-items: center;
            flex: 1;

            .filter-icon {
              position: absolute;
              left: 9px;
              color: var(--color-text-muted);
            }
            .model-search-input {
              width: 100%;
              padding: 6px 10px 6px 28px;
              box-sizing: border-box;
              border: 1px solid var(--color-line);
              border-radius: 6px;
              color: var(--color-text);
              background: var(--color-panel-soft);
              font-size: 11px;
              outline: none;

              &:focus {
                border-color: #7c3aed;
              }
            }
          }

          .target-select-wrap {
            .target-select {
              padding: 6px 10px;
              border: 1px solid var(--color-line);
              border-radius: 6px;
              color: var(--color-text);
              background: var(--color-panel-soft);
              font-size: 11px;
              outline: none;
              cursor: pointer;

              &:focus {
                border-color: #7c3aed;
              }
            }
          }
        }

        .model-table-wrap {
          overflow-x: auto;

          .model-table {
            width: 100%;
            border-collapse: collapse;

            .model-head {
              .model-heading-row {
                .table-heading {
                  padding: 8px 10px;
                  border-bottom: 1px solid var(--color-line);
                  text-align: left;
                  color: var(--color-text-muted);
                  font-size: 11px;

                  &.action-heading {
                    text-align: right;
                  }
                }
              }
            }

            .model-body {
              .model-row {
                cursor: pointer;

                &.selected {
                  background: rgba(124, 58, 237, 0.06);
                }
                &:hover:not(.selected) {
                  background: var(--color-panel-soft);
                }

                .model-cell {
                  padding: 10px;
                  border-bottom: 1px solid var(--color-line);
                  color: var(--color-text);
                  font-size: 12px;

                  &.action-cell {
                    text-align: right;
                  }

                  .model-picker {
                    display: flex;
                    align-items: center;
                    gap: 8px;
                    padding: 0;
                    border: 0;
                    color: var(--color-text);
                    background: transparent;
                    cursor: pointer;

                    .model-symbol {
                      display: inline-flex;
                      padding: 5px;
                      border: 1px solid var(--color-line);
                      border-radius: 6px;
                      color: var(--color-text-muted);
                      background: var(--color-panel);
                    }
                    .model-name {
                      overflow: hidden;
                      text-overflow: ellipsis;
                      white-space: nowrap;
                    }
                  }

                  .model-id {
                    color: #7c3aed;
                    font-size: 11px;
                  }
                  .route-target {
                    color: var(--color-text-muted);
                    font-size: 11px;
                  }
                  .route-model {
                    display: block;
                    margin-top: 3px;
                    color: var(--color-text-muted);
                    font-size: 10px;
                  }
                  .model-copy {
                    display: inline-flex;
                    padding: 5px;
                    border: 0;
                    border-radius: 4px;
                    color: var(--color-text-muted);
                    background: transparent;
                    cursor: pointer;

                    &:hover {
                      color: #7c3aed;
                      background: rgba(124, 58, 237, 0.1);
                    }
                  }
                }
              }
            }
          }
        }

        .empty-models {
          display: flex;
          flex-direction: column;
          align-items: center;
          padding: 36px 16px;
          text-align: center;

          .empty-icon {
            color: var(--color-text-muted);
            opacity: 0.6;
            margin-bottom: 10px;
          }
          .empty-title {
            margin: 0 0 6px;
            color: var(--color-text);
            font-size: 14px;
          }
          .empty-description {
            margin: 0 0 14px;
            color: var(--color-text-muted);
            font-size: 12px;
          }
          .empty-add-btn {
            padding: 6px 14px;
            border: 1px solid #7c3aed;
            border-radius: 6px;
            color: #7c3aed;
            background: rgba(124, 58, 237, 0.06);
            font-size: 12px;
            cursor: pointer;

            &:hover {
              background: rgba(124, 58, 237, 0.12);
            }
          }
        }

        .filter-empty-models {
          display: flex;
          flex-direction: column;
          align-items: center;
          padding: 28px 16px;
          text-align: center;
          gap: 8px;

          .empty-icon {
            color: var(--color-text-muted);
            opacity: 0.6;
          }
          .empty-title {
            margin: 0;
            color: var(--color-text-muted);
            font-size: 12px;
          }
          .reset-filter-btn {
            padding: 4px 10px;
            border: 1px solid var(--color-line);
            border-radius: 5px;
            color: #7c3aed;
            background: transparent;
            font-size: 11px;
            cursor: pointer;

            &:hover {
              background: rgba(124, 58, 237, 0.08);
            }
          }
        }

        .model-pagination {
          display: flex;
          align-items: center;
          justify-content: space-between;
          margin-top: 12px;
          padding-top: 10px;
          border-top: 1px solid var(--color-line);

          .pagination-info {
            color: var(--color-text-muted);
            font-size: 11px;
          }
          .pagination-actions {
            display: flex;
            align-items: center;
            gap: 6px;

            .page-size-select {
              padding: 3px 6px;
              border: 1px solid var(--color-line);
              border-radius: 4px;
              color: var(--color-text);
              background: var(--color-panel-soft);
              font-size: 11px;
              outline: none;
              cursor: pointer;
            }
            .page-nav-btn {
              display: inline-flex;
              align-items: center;
              justify-content: center;
              width: 24px;
              height: 24px;
              border: 1px solid var(--color-line);
              border-radius: 4px;
              color: var(--color-text);
              background: var(--color-panel);
              cursor: pointer;
              font-size: 11px;

              &:disabled {
                opacity: 0.4;
                cursor: default;
              }
              &:hover:not(:disabled) {
                border-color: #7c3aed;
                color: #7c3aed;
              }
            }
            .page-current {
              color: var(--color-text-muted);
              font-size: 11px;
              padding: 0 4px;
            }
          }
        }
      }

      .quick-integration {
        display: flex;
        flex: 0 0 42%;
        min-width: 320px;
        flex-direction: column;
        color-scheme: dark;
        color: #c5cddd;
        background: #151b29;
        border-left: 1px solid var(--color-line);

        .code-heading {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 14px 16px;
          border-bottom: 1px solid #232c3f;

          .code-title {
            display: inline-flex;
            align-items: center;
            gap: 7px;
            color: #e1e7f3;
            font-size: 12px;
          }
          .language-select {
            padding: 4px 8px;
            border: 1px solid #384155;
            border-radius: 5px;
            color: #b8c4d8;
            background: #1e2637;
            font-size: 11px;
            cursor: pointer;
            outline: none;
          }
        }

        .code-toolbar {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 10px 16px;
          border-bottom: 1px solid #1e2637;

          .request-path {
            font-family: Consolas, "Cascadia Code", monospace;
            font-size: 11px;
            color: #8491ab;

            .request-method {
              margin-right: 6px;
              color: #10b981;
            }
          }
          .code-copy {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            padding: 3px 8px;
            border: 1px solid #384155;
            border-radius: 4px;
            color: #a6b4cd;
            background: #1e2637;
            font-size: 11px;
            cursor: pointer;

            &:hover:not(:disabled) {
              color: #fff;
              border-color: #4b5875;
            }
            &:disabled {
              opacity: 0.5;
              cursor: default;
            }
          }
        }

        .code-container {
          display: flex;
          flex: 1;
          min-height: 260px;
          max-height: 480px;
          overflow: auto;

          .code-lines-col {
            display: flex;
            flex-direction: column;
            padding: 14px 10px 14px 14px;
            border-right: 1px solid #232c3f;
            user-select: none;
            color: #4b5875;
            font-family: Consolas, "Cascadia Code", monospace;
            font-size: 11px;
            line-height: 1.8;
            text-align: right;
          }

          .code-example {
            flex: 1;
            margin: 0;
            padding: 14px;
            overflow-x: auto;
            font-size: 11px;
            line-height: 1.8;

            .code-content {
              font-family: Consolas, "Cascadia Code", monospace;

              .code-string {
                color: #a6cfb2;
              }
              .code-keyword {
                color: #c4a6e9;
              }
              .code-number {
                color: #e5b78c;
              }
              .code-plain {
                color: #c5cddd;
              }
            }
          }
        }

        .code-footer {
          display: flex;
          align-items: flex-start;
          gap: 6px;
          padding: 12px 16px;
          border-top: 1px solid #232c3f;
          color: #8491ab;
          font-size: 11px;

          .code-note {
            line-height: 1.5;

            .token-variable {
              color: #a6b4cd;
            }
          }
        }
      }
    }
  }

  .connection-note {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    color: var(--color-text-muted);
    font-size: 11px;
  }
}
</style>
