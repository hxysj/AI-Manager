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
          <span class="status-dot"></span
          >{{ enabled ? "服务运行中" : "服务未开启" }}
        </span>
      </header>
      <div class="protocol-cards">
        <article
          v-for="protocol in protocols"
          :key="protocol.id"
          class="protocol-card"
          :class="[protocol.id, { selected: selectedProtocol === protocol.id }]"
        >
          <div class="card-heading">
            <button
              class="protocol-picker"
              type="button"
              :aria-pressed="selectedProtocol === protocol.id"
              @click="selectEndpoint(protocol)"
            >
              <span class="protocol-mark"><Code2 :size="16" /></span
              >{{ protocol.name }}
            </button>
            <button
              class="base-copy"
              type="button"
              :aria-label="`复制 ${protocol.name} BASE`"
              @click="copyText(`${base}${protocol.basePath}`)"
            >
              <Copy :size="12" />复制 BASE
            </button>
          </div>
          <code class="base-address">{{ base }}{{ protocol.basePath }}</code>
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
                  <span class="endpoint-dot"></span>{{ endpoint.name }}
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
        </article>
      </div>
      <footer class="protocol-footer">
        <p class="auth-note">
          <KeyRound :size="13" />所有接口均需调用 Token；支持
          Bearer、x-api-key、x-goog-api-key。
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
            <span class="browser-caption">选择模型，生成接入示例</span>
            <code class="models-path">GET {{ modelsPath }}</code>
          </div>
          <div class="model-table-wrap">
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
                  v-for="model in modelEntries"
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
                      <span class="model-symbol"><Box :size="15" /></span>
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
                      @click.stop="copyText(model.id)"
                    >
                      <Copy :size="13" />
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <div v-if="!modelEntries.length" class="empty-models">
            <Boxes :size="27" />
            <p class="empty-title">暂时没有可用模型</p>
            <p class="empty-description">
              在“配置与模型”中选择底层目标，添加模型路由并保存后，即可在这里接入。
            </p>
          </div>
          <p v-else class="model-note">
            模型与路由来自已保存配置，可在“用量与追踪”中查看实际转发结果。
          </p>
        </div>
        <aside class="quick-integration">
          <header class="code-heading">
            <span class="code-title"><Terminal :size="16" />快速集成</span>
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
            <span class="request-path"
              ><span class="request-method">POST</span
              >{{ activeEndpoint.path }}</span
            >
            <button
              class="code-copy"
              type="button"
              :disabled="!modelId"
              @click="copyText(snippet)"
            >
              <Copy :size="13" />复制代码
            </button>
          </div>
          <pre
            class="code-example"
          ><code class="code-content"><span v-for="(segment, index) in codeSegments" :key="index" :class="segment.kind">{{ segment.text }}</span></code></pre>
          <footer class="code-footer">
            <KeyRound :size="13" /><span class="code-note"
              >先将调用 Token 写入环境变量
              <code class="token-variable">AI_MANAGER_TOKEN</code
              >，示例会自动读取。</span
            >
          </footer>
        </aside>
      </div>
    </section>
    <p class="connection-note">
      <Info
        :size="14"
      />跨协议支持文本与函数工具调用；图片、可见思考及其他专属字段请使用同协议目标。请求需携带完整历史。
    </p>
  </div>
</template>

<script setup>
import { computed, ref } from "vue"
import {
  ArrowUpRight,
  Box,
  Boxes,
  Code2,
  Copy,
  Info,
  KeyRound,
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
const protocols = [
  {
    id: "openai",
    name: "OpenAI 协议",
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
    basePath: "",
    endpoints: [
      { id: "messages", name: "Anthropic Messages", path: "/v1/messages" }
    ]
  },
  {
    id: "gemini",
    name: "Gemini 协议",
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
// 路由与模型只读取已保存状态；模型被移除后自然回到首个有效模型。
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
// 示例仅引用调用方的环境变量，不读取底层账号或刚创建的 Token 明文。
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
// 用文本插值着色，模型别名中的 HTML 不会进入可执行标记。
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
  gap: 18px;
  color: var(--color-text);

  .protocol-panel {
    padding: 22px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);

    .protocol-heading {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      margin-bottom: 20px;

      .protocol-title {
        display: flex;
        align-items: center;
        gap: 11px;

        .heading-icon {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 38px;
          height: 38px;
          border-radius: 10px;
          color: #5c8fea;
          background: rgba(92, 143, 234, 0.1);
        }
        .heading-copy {
          .heading-name {
            margin: 0 0 3px;
            font-size: 15px;
          }
          .heading-caption {
            color: var(--color-text-soft);
            font-size: 11px;
            letter-spacing: 0.3px;
          }
        }
      }
      .service-status {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 5px 9px;
        border: 1px solid var(--color-line);
        border-radius: 20px;
        color: var(--color-text-muted);
        font-size: 11px;

        &.active {
          color: var(--color-success);
          border-color: var(--color-success-line);
          background: var(--color-success-soft);
        }
        .status-dot {
          width: 5px;
          height: 5px;
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
        padding: 15px;
        border: 1px solid var(--color-line);
        border-radius: 10px;
        background: var(--color-panel-soft);
        transition: border-color 0.15s;

        &.openai {
          --protocol-color: #5c8fea;
          --protocol-soft: rgba(92, 143, 234, 0.09);
        }
        &.anthropic {
          --protocol-color: #a387de;
          --protocol-soft: rgba(163, 135, 222, 0.09);
        }
        &.gemini {
          --protocol-color: #38a58c;
          --protocol-soft: rgba(56, 165, 140, 0.09);
        }
        &.selected {
          border-color: var(--protocol-color);
        }
        .card-heading {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 6px;

          .protocol-picker {
            display: inline-flex;
            min-width: 0;
            align-items: center;
            gap: 8px;
            padding: 0;
            border: 0;
            color: var(--color-text);
            background: transparent;
            text-align: left;
            font-size: 13px;
            cursor: pointer;

            .protocol-mark {
              display: inline-flex;
              padding: 5px;
              border-radius: 7px;
              color: var(--protocol-color);
              background: var(--protocol-soft);
            }
          }
          .base-copy {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            flex-shrink: 0;
            padding: 5px 6px;
            border: 1px solid var(--color-line);
            border-radius: 5px;
            color: var(--color-text-muted);
            background: var(--color-panel);
            font-size: 10px;
            cursor: pointer;
            &:hover {
              color: var(--protocol-color);
              border-color: var(--protocol-color);
            }
          }
        }
        .base-address {
          display: block;
          margin: 14px 0 13px;
          padding: 9px 10px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          color: var(--color-text-muted);
          background: var(--color-panel);
          overflow-wrap: anywhere;
          font-size: 11px;
        }
        .endpoint-list {
          display: flex;
          flex-direction: column;
          gap: 7px;

          .endpoint-item {
            display: flex;
            align-items: center;
            gap: 4px;

            .endpoint-picker {
              flex: 1;
              min-width: 0;
              padding: 6px;
              border: 0;
              border-radius: 5px;
              text-align: left;
              background: transparent;
              color: var(--color-text-muted);
              cursor: pointer;

              &.active {
                background: var(--protocol-soft);
                color: var(--protocol-color);
              }
              &:hover {
                background: var(--protocol-soft);
              }
              .endpoint-heading {
                display: flex;
                align-items: center;
                gap: 6px;
                font-size: 11px;
                .endpoint-dot {
                  width: 4px;
                  height: 4px;
                  flex-shrink: 0;
                  border-radius: 50%;
                  background: var(--protocol-color);
                }
                .endpoint-badge {
                  padding: 1px 5px;
                  border-radius: 4px;
                  color: var(--protocol-color);
                  background: var(--protocol-soft);
                  font-size: 9px;
                }
              }
              .endpoint-path {
                display: block;
                margin: 5px 0 0 10px;
                color: var(--color-text-soft);
                overflow-wrap: anywhere;
                font-size: 10px;
                line-height: 1.5;
              }
            }
            .endpoint-copy {
              display: flex;
              padding: 5px;
              border: 0;
              border-radius: 4px;
              background: transparent;
              color: var(--color-text-soft);
              cursor: pointer;
              &:hover {
                color: var(--protocol-color);
                background: var(--protocol-soft);
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
        gap: 3px;
        flex-shrink: 0;
        padding: 0;
        border: 0;
        color: var(--color-primary);
        background: transparent;
        cursor: pointer;
        font-size: 11px;
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
      padding: 18px 22px;
      border-bottom: 1px solid var(--color-line);

      .integration-title {
        display: flex;
        align-items: center;
        gap: 9px;
        color: var(--color-primary);
        .heading-name {
          margin: 0;
          color: var(--color-text);
          font-size: 14px;
        }
        .model-count {
          padding: 3px 7px;
          border-radius: 5px;
          color: var(--color-text-muted);
          background: var(--color-panel-soft);
          font-size: 10px;
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
          color: var(--color-primary);
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
          margin-bottom: 16px;
          .browser-caption {
            color: var(--color-text-muted);
            font-size: 11px;
          }
          .models-path {
            color: var(--color-text-soft);
            font-size: 10px;
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
                  padding: 10px 8px;
                  border-bottom: 1px solid var(--color-line);
                  text-align: left;
                  white-space: nowrap;
                  color: var(--color-text-soft);
                  font-size: 10px;
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
                  background: var(--color-primary-soft);
                }
                &:hover {
                  background: var(--color-panel-soft);
                }
                .model-cell {
                  padding: 13px 8px;
                  border-bottom: 1px solid var(--color-line);
                  color: var(--color-text-muted);
                  font-size: 11px;
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
                    text-align: left;
                    font-size: 12px;
                    .model-symbol {
                      display: inline-flex;
                      padding: 6px;
                      border: 1px solid var(--color-line);
                      border-radius: 7px;
                      background: var(--color-panel);
                      color: var(--color-text-muted);
                    }
                    .model-name {
                      min-width: 80px;
                      overflow-wrap: anywhere;
                    }
                  }
                  .model-id {
                    display: block;
                    min-width: 95px;
                    max-width: 200px;
                    overflow-wrap: anywhere;
                    color: var(--color-primary);
                    font-size: 11px;
                  }
                  .route-target {
                    display: block;
                    min-width: 70px;
                    max-width: 160px;
                    overflow-wrap: anywhere;
                  }
                  .route-model {
                    display: block;
                    max-width: 160px;
                    margin-top: 4px;
                    color: var(--color-text-soft);
                    overflow-wrap: anywhere;
                    font-size: 10px;
                  }
                  .model-copy {
                    display: inline-flex;
                    padding: 6px;
                    border: 0;
                    border-radius: 5px;
                    color: var(--color-text-muted);
                    background: transparent;
                    cursor: pointer;
                    &:hover {
                      color: var(--color-primary);
                      background: var(--color-primary-soft);
                    }
                  }
                }
              }
            }
          }
        }
        .empty-models {
          display: flex;
          align-items: center;
          flex-direction: column;
          padding: 40px 18px;
          text-align: center;
          color: var(--color-text-soft);
          .empty-title {
            margin: 12px 0 5px;
            color: var(--color-text-muted);
            font-size: 13px;
          }
          .empty-description {
            max-width: 340px;
            margin: 0;
            font-size: 12px;
            line-height: 1.8;
          }
        }
        .model-note {
          margin: 16px 0 0;
          color: var(--color-text-soft);
          font-size: 10px;
          line-height: 1.7;
        }
      }
      .quick-integration {
        display: flex;
        flex: 0 0 40%;
        min-width: 290px;
        max-width: 50%;
        flex-direction: column;
        color-scheme: dark;
        color: #c5cddd;
        background: #151b29;
        border-left: 1px solid var(--color-line);

        .code-heading {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 10px;
          padding: 17px 18px;
          border-bottom: 1px solid #2a3243;
          .code-title {
            display: inline-flex;
            align-items: center;
            gap: 7px;
            white-space: nowrap;
            color: #e1e7f3;
            font-size: 12px;
          }
          .language-select {
            min-width: 0;
            padding: 5px 7px;
            border: 1px solid #384155;
            border-radius: 5px;
            color: #b8c4d8;
            background: #1e2637;
            font-size: 10px;
            cursor: pointer;
          }
        }
        .code-toolbar {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
          gap: 10px;
          padding: 12px 18px 0;
          .request-path {
            color: #8491ab;
            overflow-wrap: anywhere;
            font-family: Consolas, "Cascadia Code", monospace;
            font-size: 10px;
            .request-method {
              margin-right: 7px;
              color: #7abbaa;
            }
          }
          .code-copy {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            flex-shrink: 0;
            padding: 0;
            border: 0;
            background: transparent;
            color: #a6b4cd;
            font-size: 10px;
            cursor: pointer;
            &:hover {
              color: #e1e7f3;
            }
            &:disabled {
              opacity: 0.4;
              cursor: default;
            }
          }
        }
        .code-example {
          flex: 1;
          min-height: 265px;
          max-height: 460px;
          margin: 0;
          padding: 18px;
          overflow: auto;
          tab-size: 2;
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
        .code-footer {
          display: flex;
          align-items: flex-start;
          gap: 6px;
          padding: 14px 18px;
          border-top: 1px solid #2a3243;
          color: #8491ab;
          .code-note {
            flex: 1;
            font-size: 10px;
            line-height: 1.8;
            .token-variable {
              color: #a6b4cd;
              overflow-wrap: anywhere;
            }
          }
        }
      }
    }
  }
  .connection-note {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    margin: 0;
    padding: 0 3px;
    color: var(--color-text-muted);
    font-size: 11px;
    line-height: 1.8;
  }
}
</style>
