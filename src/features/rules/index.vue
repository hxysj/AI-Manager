<template>
  <section class="rules-view">
    <header class="rules-view__toolbar">
      <div class="rules-view__cli-tabs">
        <button
          v-for="cli in ruleTabs"
          :key="cli.id"
          :class="[
            'rules-view__cli-tab',
            { 'rules-view__cli-tab--active': activeCli === cli.id }
          ]"
          type="button"
          @click="selectCli(cli.id)"
        >
          <AiIcon
            v-if="cli.icon"
            class="rules-view__cli-icon"
            :name="cli.icon"
            :alt="`${cli.name} 图标`"
          />
          {{ cli.name }}
        </button>
      </div>

      <div class="rules-view__toolbar-actions">
        <button
          class="rules-view__secondary"
          type="button"
          :disabled="pending || !activeRuntimeCli || isCommonTab"
          @click="openImportDialog"
        >
          导入全局 Prompt
        </button>
        <button
          class="rules-view__add"
          type="button"
          :disabled="pending || !activeCli"
          @click="openCreatePrompt"
        >
          <Plus :size="20" />
        </button>
      </div>
    </header>

    <section class="rules-view__runtime">
      <div>
        <strong>{{ runtimeTitle }}</strong>
        <span>{{ runtimePathText }}</span>
      </div>
      <span
        v-if="!isCommonTab"
        :class="['rules-view__status', `rules-view__status--${runtimeStatus}`]"
      >
        {{ runtimeStatusText }}
      </span>
    </section>

    <section class="rules-view__list-panel">
      <article
        v-for="prompt in scopedPrompts"
        :key="prompt.id"
        :class="[
          'rules-view__prompt-card',
          {
            'rules-view__prompt-card--active': isPromptEnabledInCurrentScope(prompt),
            'rules-view__prompt-card--modified':
              isPromptModifiedInCurrentScope(prompt),
            'rules-view__prompt-card--selected': prompt.id === selectedPromptId
          }
        ]"
        @click="openDrawer(prompt.id)"
      >
        <div class="rules-view__prompt-main">
          <strong>{{ prompt.name }}</strong>
          <span>{{ prompt.description || "未填写描述" }}</span>
          <small>{{ formatTime(prompt.updatedAt) }}</small>
        </div>
        <div class="rules-view__prompt-actions">
          <div v-if="isCommonTab" class="rules-view__target-buttons">
            <button
              v-for="cli in visibleCliTargets"
              :key="cli.id"
              :class="[
                'rules-view__target-button',
                {
                  'rules-view__target-button--active': isPromptEnabledOnRuntime(
                    prompt,
                    cli.id
                  ),
                  'rules-view__target-button--modified':
                    isPromptModifiedOnRuntime(prompt, cli.id)
                }
              ]"
              type="button"
              :title="promptTargetTitle(prompt, cli)"
              :disabled="pending"
              @click.stop="togglePromptTarget(prompt, cli.id)"
            >
              <AiIcon
                v-if="cli.icon"
                class="rules-view__target-icon"
                :name="cli.icon"
                :alt="`${cli.name} 图标`"
              />
              <span v-else>{{ cli.name.slice(0, 1) }}</span>
            </button>
          </div>
          <span
            v-if="!isCommonTab && isPromptEnabledOnRuntime(prompt)"
            class="rules-view__active-tag"
          >
            已启用
          </span>
          <button
            v-if="showPromptCompareButton(prompt)"
            class="rules-view__compare-button"
            type="button"
            :disabled="pending"
            @click.stop="openCompareDialog(prompt)"
          >
            对比
          </button>
          <button
            v-if="!isCommonTab"
            class="rules-view__state-button"
            type="button"
            :disabled="pending || !activeRuntimeCli"
            @click.stop="togglePrompt(prompt)"
          >
            {{ promptActionText(prompt) }}
          </button>
          <button
            class="rules-view__icon-button"
            type="button"
            title="查看详情"
            :disabled="pending"
            @click.stop="openDrawer(prompt.id)"
          >
            <Eye :size="15" />
          </button>
          <button
            class="rules-view__icon-button"
            type="button"
            title="编辑 Prompt"
            :disabled="pending"
            @click.stop="openEditPrompt(prompt)"
          >
            <Pencil :size="15" />
          </button>
          <button
            class="rules-view__icon-button rules-view__icon-button--danger"
            type="button"
            title="删除 Prompt"
            :disabled="pending || isPromptActive(prompt)"
            @click.stop="deletePrompt(prompt)"
          >
            <Trash2 :size="15" />
          </button>
        </div>
      </article>

      <div v-if="!scopedPrompts.length" class="rules-view__empty">
        {{ emptyText }}
      </div>
    </section>

    <BaseModal
      v-if="showEditor"
      :title="draft.id ? '编辑 Prompt' : '新增 Prompt'"
      :description="editorDescription"
      @close="closeEditor"
    >
      <form class="rules-view__editor" @submit.prevent="submitPrompt">
        <div class="rules-view__form-grid">
          <label class="rules-view__field">
            <span>CLI</span>
            <select v-model="draft.cli" :disabled="Boolean(draft.id)">
              <option
                v-for="cli in promptScopeOptions"
                :key="cli.id"
                :value="cli.id"
              >
                {{ cli.name }}
              </option>
            </select>
          </label>
          <label class="rules-view__field">
            <span>名称</span>
            <input v-model.trim="draft.name" type="text" />
          </label>
          <label class="rules-view__field rules-view__field--wide">
            <span>描述</span>
            <input v-model.trim="draft.description" type="text" />
          </label>
          <label class="rules-view__field rules-view__field--wide">
            <span>Prompt 内容</span>
            <textarea v-model="draft.content" rows="15"></textarea>
          </label>
        </div>

        <footer class="rules-view__editor-footer">
          <button type="button" @click="closeEditor">取消</button>
          <button type="submit" :disabled="pending">保存</button>
        </footer>
      </form>
    </BaseModal>

    <BaseModal
      v-if="showImport"
      title="导入全局 Prompt"
      :description="`读取 ${activeCliName} 当前全局文件，并保存为 Monkey Thief Prompt 资产。导入后不会自动启用。`"
      @close="showImport = false"
    >
      <form class="rules-view__editor" @submit.prevent="submitImport">
        <div class="rules-view__form-grid">
          <label class="rules-view__field rules-view__field--wide">
            <span>名称</span>
            <input v-model.trim="importDraft.name" type="text" />
          </label>
          <label class="rules-view__field rules-view__field--wide">
            <span>描述</span>
            <input v-model.trim="importDraft.description" type="text" />
          </label>
        </div>

        <footer class="rules-view__editor-footer">
          <button type="button" @click="showImport = false">取消</button>
          <button type="submit" :disabled="pending">导入</button>
        </footer>
      </form>
    </BaseModal>

    <div v-if="drawerPrompt" class="rules-drawer">
      <div class="rules-drawer__overlay" @click="closeDrawer"></div>
      <aside class="rules-drawer__panel">
        <header class="rules-drawer__header">
          <div class="rules-drawer__hero">
            <div class="rules-drawer__title-wrap">
              <p>{{ activeScopeName }}</p>
              <h2>{{ drawerPrompt.name }}</h2>
              <span>{{ drawerPrompt.description || "未填写描述" }}</span>
            </div>
          </div>
          <button
            class="rules-drawer__close"
            type="button"
            @click="closeDrawer"
          >
            <X :size="18" />
          </button>
        </header>

        <div class="rules-drawer__tabs">
          <button
            v-for="item in tabs"
            :key="item.id"
            :class="[
              'rules-drawer__tab',
              { 'rules-drawer__tab--active': activeTab === item.id }
            ]"
            type="button"
            @click="activeTab = item.id"
          >
            {{ item.label }}
          </button>
        </div>

        <div class="rules-drawer__content">
          <section
            v-if="activeTab === 'overview'"
            class="rules-drawer__section"
          >
            <div class="rules-drawer__block">
              <span>状态</span>
              <p>{{ drawerRuntimeStatusText }}</p>
            </div>

            <div class="rules-drawer__block">
              <span>更新时间</span>
              <p>{{ formatTime(drawerPrompt.updatedAt) }}</p>
            </div>

            <div class="rules-drawer__block">
              <span>Prompt 内容</span>
              <pre class="rules-drawer__content-text">{{
                drawerPrompt.content
              }}</pre>
            </div>
          </section>

          <section
            v-else-if="activeTab === 'runtime'"
            class="rules-drawer__section"
          >
            <div class="rules-drawer__block">
              <span>全局文件路径</span>
              <p>{{ drawerRuntimePathText }}</p>
            </div>
            <div class="rules-drawer__block">
              <span>运行状态</span>
              <p>{{ drawerRuntimeStatusText }}</p>
            </div>
            <div class="rules-drawer__block">
              <span>管理器版本</span>
              <pre class="rules-drawer__content-text">{{
                drawerPrompt.content
              }}</pre>
            </div>
          </section>

          <section v-else class="rules-drawer__section">
            <div class="rules-drawer__grid">
              <article>
                <span>CLI</span>
                <strong>{{ promptScopeName(drawerPrompt.cli) }}</strong>
              </article>
              <article>
                <span>ID</span>
                <strong>{{ drawerPrompt.id }}</strong>
              </article>
              <article>
                <span>CreatedAt</span>
                <strong>{{ formatTime(drawerPrompt.createdAt) }}</strong>
              </article>
              <article>
                <span>UpdatedAt</span>
                <strong>{{ formatTime(drawerPrompt.updatedAt) }}</strong>
              </article>
            </div>
            <div class="rules-drawer__block">
              <span>管理目录</span>
              <p>{{ drawerPrompt.storageDir || "未找到保存目录" }}</p>
              <button
                class="rules-drawer__directory-button"
                type="button"
                :disabled="!drawerPrompt.storageDir"
                @click="openPromptDirectory(drawerPrompt)"
              >
                <FolderOpen :size="15" />
                打开目录
              </button>
            </div>
          </section>
        </div>

        <footer class="rules-drawer__footer">
          <div v-if="isCommonTab" class="rules-view__target-buttons">
            <button
              v-for="cli in visibleCliTargets"
              :key="cli.id"
              :class="[
                'rules-view__target-button',
                {
                  'rules-view__target-button--active': isPromptEnabledOnRuntime(drawerPrompt, cli.id),
                  'rules-view__target-button--modified': isPromptModifiedOnRuntime(drawerPrompt, cli.id)
                }
              ]"
              type="button"
              :title="promptTargetTitle(drawerPrompt, cli)"
              :disabled="pending"
              @click="togglePromptTarget(drawerPrompt, cli.id)"
            >
              <AiIcon
                v-if="cli.icon"
                class="rules-view__target-icon"
                :name="cli.icon"
                :alt="`${cli.name} 图标`"
              />
              <span v-else>{{ cli.name.slice(0, 1) }}</span>
            </button>
          </div>
          <button
            v-if="!isCommonTab"
            class="rules-drawer__primary"
            type="button"
            :disabled="pending || !activeRuntimeCli"
            @click="togglePrompt(drawerPrompt)"
          >
            {{ promptActionText(drawerPrompt) }}
          </button>
          <button
            v-if="!isCommonTab"
            class="rules-drawer__secondary"
            type="button"
            :disabled="pending"
            @click="openCompareDialog(drawerPrompt)"
          >
            对比
          </button>
          <button
            class="rules-drawer__secondary"
            type="button"
            :disabled="pending"
            @click="openEditPrompt(drawerPrompt)"
          >
            编辑
          </button>
          <button
            class="rules-drawer__danger"
            type="button"
            :disabled="pending || isPromptActive(drawerPrompt)"
            @click="deletePrompt(drawerPrompt)"
          >
            删除
          </button>
        </footer>
      </aside>
    </div>

    <BaseModal
      v-if="showDiff"
      class="rules-view__diff-modal"
      title="Prompt Diff"
      :description="diffDescription"
      @close="closeDiffDialog"
    >
      <div ref="diffEditorRef" class="rules-view__diff-editor"></div>
      <footer class="rules-view__diff-footer">
        <button
          class="rules-view__diff-button rules-view__diff-button--primary"
          type="button"
          :disabled="pending || !canResolveDiff"
          @click="resolveDiff('manager')"
        >
          {{
            diffMode === "import"
              ? "保留管理器版本"
              : "保留管理器版本并覆盖全局"
          }}
        </button>
        <button
          class="rules-view__diff-button"
          type="button"
          :disabled="pending || !canResolveDiff"
          @click="resolveDiff('runtime')"
        >
          {{
            diffMode === "import"
              ? "使用全局版本覆盖管理器"
              : "保留全局版本并导入管理器"
          }}
        </button>
      </footer>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, reactive, ref, watch } from "vue"
import * as monaco from "monaco-editor/esm/vs/editor/editor.api"
import { Eye, FolderOpen, Pencil, Plus, Trash2, X } from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import BaseModal from "@/components/BaseModal.vue"
import { ruleApi } from "@/api"
import { useGlobalLoading } from "@/utils/global-loading"
import { createMessage } from "@/utils/message"

const props = defineProps({
  cliTargets: {
    type: Array,
    required: true
  },
  pending: {
    type: Boolean,
    required: true
  },
  rules: {
    type: Object,
    required: true
  }
})

const emit = defineEmits([
  "delete-rule",
  "enable-rule",
  "import-rule",
  "open-path",
  "resolve-import-conflict",
  "resolve-drift",
  "save-rule",
  "toggle-rule"
])

const { withGlobalLoading } = useGlobalLoading()
const COMMON_PROMPT_CLI = "common"
const commonPromptTab = {
  id: COMMON_PROMPT_CLI,
  name: "通用"
}
const activeCli = ref("")
const selectedPromptId = ref("")
const activeTab = ref("overview")
const showEditor = ref(false)
const showImport = ref(false)
const showDiff = ref(false)
const diffEditorRef = ref(null)
const diffRuntimePath = ref("")
const diffMode = ref("")
const diffPrompt = ref(null)
const diffRuntimeContent = ref("")
const diffSimilarity = ref(0)
const drawerPrompt = ref(null)

let diffEditor = null

const tabs = [
  { id: "overview", label: "Overview" },
  { id: "runtime", label: "Runtime" },
  { id: "meta", label: "Meta" }
]

const draft = reactive({
  id: "",
  cli: "",
  name: "",
  description: "",
  content: ""
})

const importDraft = reactive({
  name: "",
  description: ""
})

const diffDescription = computed(() => {
  if (diffMode.value === "import") {
    return `全局 Prompt 与「${diffPrompt.value?.name || "未知 Prompt"}」相似度 ${Math.round(diffSimilarity.value * 100)}%，请选择保存到管理器的版本。`
  }

  return diffRuntimePath.value || "当前没有全局 Runtime 文件"
})

const canResolveDiff = computed(() => {
  return diffMode.value === "import"
    ? Boolean(diffPrompt.value)
    : Boolean(drawerPrompt.value)
})

const supportedCliIds = computed(() => {
  return new Set((props.rules.supportedClis || []).map((item) => item.id))
})

const visibleCliTargets = computed(() => {
  const installedTargets = props.cliTargets.filter((item) =>
    supportedCliIds.value.has(item.id)
  )

  if (installedTargets.length) {
    return installedTargets
  }

  return props.rules.supportedClis || []
})

const ruleTabs = computed(() => {
  return [...visibleCliTargets.value, commonPromptTab]
})

const promptScopeOptions = computed(() => {
  return [...visibleCliTargets.value, commonPromptTab]
})

const cliNameMap = computed(() => {
  return Object.fromEntries(
    promptScopeOptions.value.map((item) => [item.id, item.name])
  )
})

const isCommonTab = computed(() => {
  return activeCli.value === COMMON_PROMPT_CLI
})

const activeRuntimeCli = computed(() => {
  return isCommonTab.value ? "" : activeCli.value
})

const activeCliName = computed(() => {
  return cliNameMap.value[activeCli.value] || activeCli.value || "Prompt"
})

const activeScopeName = computed(() => {
  if (!isCommonTab.value) {
    return activeCliName.value
  }

  return "通用"
})

const runtimeTitle = computed(() => {
  if (!isCommonTab.value) {
    return `${activeCliName.value} Runtime`
  }

  return "通用 Prompt"
})

const runtimePathText = computed(() => {
  if (isCommonTab.value) {
    return "点击 Prompt 右侧 CLI 图标，直接挂载或取消对应 CLI 的全局 Prompt。"
  }

  return runtimePath.value || "尚未发现全局 Prompt 文件路径"
})

const editorDescription = computed(() => {
  return `当前配置到 ${cliNameMap.value[draft.cli] || draft.cli || activeCliName.value}`
})

const emptyText = computed(() => {
  return isCommonTab.value
    ? "当前还没有通用 Prompt，先新建一个可挂载到 CLI 的 Prompt。"
    : "当前 CLI 还没有 Prompt，先新建或导入全局 Prompt。"
})

const scopedPrompts = computed(() => {
  return (props.rules.prompts || []).filter(
    (item) => item.cli === activeCli.value
  )
})

const activePromptId = computed(() => {
  return props.rules.profiles?.[activeRuntimeCli.value]?.activePromptId || ""
})

const runtimeState = computed(() => {
  return props.rules.runtimeState?.[activeRuntimeCli.value] || {}
})

const runtimePath = computed(() => {
  return runtimeState.value.runtimePath || ""
})

const runtimeStatus = computed(() => {
  return runtimeState.value.status || "NO_ACTIVE"
})

const isModifiedExternally = computed(() => {
  return runtimeStatus.value === "MODIFIED_EXTERNALLY"
})

const runtimeStatusText = computed(() => {
  const statusMap = {
    SYNCED: "已同步",
    MODIFIED_EXTERNALLY: "Prompt 已被外部修改",
    DIRTY_MANAGER: "管理器版本待同步",
    CONFLICT: "版本冲突",
    NO_ACTIVE: "未启用 Prompt"
  }

  return statusMap[runtimeStatus.value] || runtimeStatus.value
})

const drawerRuntimePathText = computed(() => {
  if (isCommonTab.value) {
    return "通用 Prompt 通过 CLI 图标挂载到对应全局文件。"
  }

  return runtimePath.value || "未找到全局文件路径"
})

const drawerRuntimeStatusText = computed(() => {
  if (!isCommonTab.value || !drawerPrompt.value) {
    return runtimeStatusText.value
  }

  const enabledTargets = visibleCliTargets.value.filter((cli) =>
    isPromptEnabledOnRuntime(drawerPrompt.value, cli.id)
  )
  const modifiedTargets = enabledTargets.filter((cli) =>
    isPromptModifiedOnRuntime(drawerPrompt.value, cli.id)
  )

  if (modifiedTargets.length) {
    return `${modifiedTargets.map((cli) => cli.name).join("、")} 与全局 Prompt 有差异`
  }

  if (enabledTargets.length) {
    return `已挂载到 ${enabledTargets.map((cli) => cli.name).join("、")}`
  }

  return "未挂载到 CLI"
})

function selectCli(cli) {
  activeCli.value = cli
  selectedPromptId.value = ""
  drawerPrompt.value = null
}

function ensureActiveCli() {
  if (ruleTabs.value.find((item) => item.id === activeCli.value)) {
    return
  }

  activeCli.value = visibleCliTargets.value[0]?.id || ""
}

function ensureSelectedPrompt() {
  if (scopedPrompts.value.find((item) => item.id === selectedPromptId.value)) {
    return
  }

  selectedPromptId.value =
    scopedPrompts.value.find((item) => item.id === activePromptId.value)?.id ||
    scopedPrompts.value[0]?.id ||
    ""
}

function hasDuplicatePromptName(name, cli, promptId = "") {
  const normalizedName = String(name || "")
    .trim()
    .toLowerCase()

  return (props.rules.prompts || []).some((item) => {
    return (
      item.cli === cli &&
      item.id !== promptId &&
      item.name.trim().toLowerCase() === normalizedName
    )
  })
}

function targetActivePromptId(cli) {
  return props.rules.profiles?.[cli]?.activePromptId || ""
}

function targetRuntimeState(cli) {
  return props.rules.runtimeState?.[cli] || {}
}

function isPromptEnabledOnRuntime(prompt, cli = activeRuntimeCli.value) {
  return prompt.id === targetActivePromptId(cli)
}

function isPromptModifiedOnRuntime(prompt, cli = activeRuntimeCli.value) {
  return (
    isPromptEnabledOnRuntime(prompt, cli) &&
    ["MODIFIED_EXTERNALLY", "DIRTY_MANAGER", "CONFLICT"].includes(
      targetRuntimeState(cli).status
    )
  )
}

function isPromptEnabledInCurrentScope(prompt) {
  if (!isCommonTab.value) {
    return isPromptEnabledOnRuntime(prompt)
  }

  return visibleCliTargets.value.some((cli) =>
    isPromptEnabledOnRuntime(prompt, cli.id)
  )
}

function isPromptModifiedInCurrentScope(prompt) {
  if (!isCommonTab.value) {
    return isModifiedExternally.value && isPromptEnabledOnRuntime(prompt)
  }

  return visibleCliTargets.value.some((cli) =>
    isPromptModifiedOnRuntime(prompt, cli.id)
  )
}

function isPromptActive(prompt) {
  return Object.values(props.rules.profiles || {}).some((profile) => {
    return profile?.activePromptId === prompt.id
  })
}

function promptActionText(prompt) {
  if (isPromptEnabledOnRuntime(prompt)) {
    return isCommonTab.value ? "取消挂载" : "取消启用"
  }

  return isCommonTab.value ? "挂载并同步" : "启用并同步"
}

function promptTargetTitle(prompt, cli) {
  const active = isPromptEnabledOnRuntime(prompt, cli.id)
  const modified = isPromptModifiedOnRuntime(prompt, cli.id)

  if (active && modified) {
    return `${cli.name}：已挂载，和全局 Prompt 有差异`
  }

  if (active) {
    return `${cli.name}：已挂载，点击取消`
  }

  return `${cli.name}：点击挂载并同步`
}

function promptScopeName(cli) {
  return cliNameMap.value[cli] || cli
}

function showPromptCompareButton(prompt) {
  return (
    !isCommonTab.value &&
    isModifiedExternally.value &&
    isPromptEnabledOnRuntime(prompt)
  )
}

function openDrawer(promptId) {
  selectedPromptId.value = promptId
  drawerPrompt.value =
    scopedPrompts.value.find((item) => item.id === promptId) || null
  activeTab.value = "overview"
}

function closeDrawer() {
  drawerPrompt.value = null
}

function openCreatePrompt() {
  draft.id = ""
  draft.cli = activeCli.value
  draft.name = ""
  draft.description = ""
  draft.content = ""
  showEditor.value = true
}

function openEditPrompt(prompt) {
  draft.id = prompt.id
  draft.cli = prompt.cli
  draft.name = prompt.name
  draft.description = prompt.description || ""
  draft.content = prompt.content
  showEditor.value = true
}

function closeEditor() {
  showEditor.value = false
}

function submitPrompt() {
  if (hasDuplicatePromptName(draft.name, draft.cli, draft.id)) {
    createMessage.error("当前 CLI 已存在同名 Prompt")
    return
  }

  emit("save-rule", {
    id: draft.id || undefined,
    cli: draft.cli,
    name: draft.name,
    description: draft.description,
    content: draft.content
  })

  showEditor.value = false
}

function deletePrompt(prompt) {
  const shouldContinue = window.confirm(`删除 Prompt「${prompt.name}」？`)

  if (shouldContinue) {
    emit("delete-rule", prompt.id)
    closeDrawer()
  }
}

function togglePrompt(prompt) {
  if (isPromptEnabledOnRuntime(prompt)) {
    emit("toggle-rule", {
      cli: activeRuntimeCli.value,
      ruleId: prompt.id,
      enabled: false
    })
    return
  }

  emit("enable-rule", {
    cli: activeRuntimeCli.value,
    ruleId: prompt.id
  })
}

function togglePromptTarget(prompt, cli) {
  if (isPromptEnabledOnRuntime(prompt, cli)) {
    emit("toggle-rule", {
      cli,
      ruleId: prompt.id,
      enabled: false
    })
    return
  }

  emit("enable-rule", {
    cli,
    ruleId: prompt.id
  })
}

function openPromptDirectory(prompt) {
  emit("open-path", prompt.storageDir)
}

async function openImportDialog() {
  if (isCommonTab.value) {
    return
  }

  await withGlobalLoading(async () => {
    try {
      const result = await ruleApi.previewImportGlobalRule({
        cli: activeCli.value
      })

      if (result.status === "SAME_CONTENT") {
        createMessage.warning(
          `当前全局 Prompt 已存在于「${result.prompt.name}」，无需重复导入`
        )
        return
      }

      if (result.status === "DIFF") {
        openImportDiffDialog(result)
        return
      }

      importDraft.name = `${activeCliName.value} Global Prompt`
      importDraft.description = ""
      showImport.value = true
    } catch (error) {
      createMessage.error(error.message || String(error))
    }
  })
}

function submitImport() {
  if (hasDuplicatePromptName(importDraft.name, activeCli.value)) {
    createMessage.error("当前 CLI 已存在同名 Prompt")
    return
  }

  emit("import-rule", {
    cli: activeCli.value,
    name: importDraft.name,
    description: importDraft.description
  })
  showImport.value = false
}

async function openCompareDialog(prompt) {
  await withGlobalLoading(async () => {
    try {
      const result = await ruleApi.compareRule({
        cli: activeRuntimeCli.value,
        ruleId: prompt.id
      })

      diffMode.value = "drift"
      diffPrompt.value = null
      diffRuntimeContent.value = ""
      diffSimilarity.value = 0
      showDiff.value = true
      diffRuntimePath.value = result.runtimePath || ""
      await nextTick()
      renderDiff(result.managerContent || "", result.runtimeContent || "")
    } catch (error) {
      createMessage.error(error.message || String(error))
    }
  })
}

async function openImportDiffDialog(result) {
  diffMode.value = "import"
  diffPrompt.value = result.prompt
  diffRuntimeContent.value = result.runtimeContent || ""
  diffRuntimePath.value = result.runtimePath || ""
  diffSimilarity.value = result.similarity || 0
  showDiff.value = true
  await nextTick()
  renderDiff(result.managerContent || "", result.runtimeContent || "")
}

function closeDiffDialog() {
  showDiff.value = false
  diffMode.value = ""
  diffPrompt.value = null
  diffRuntimeContent.value = ""
  diffSimilarity.value = 0

  if (diffEditor) {
    diffEditor.dispose()
    diffEditor = null
  }
}

function renderDiff(managerContent, runtimeContent) {
  if (!diffEditorRef.value) {
    return
  }

  if (diffEditor) {
    diffEditor.dispose()
  }

  diffEditor = monaco.editor.createDiffEditor(diffEditorRef.value, {
    automaticLayout: true,
    minimap: { enabled: false },
    readOnly: true,
    renderSideBySide: true,
    scrollBeyondLastLine: false
  })
  diffEditor.setModel({
    original: monaco.editor.createModel(managerContent, "markdown"),
    modified: monaco.editor.createModel(runtimeContent, "markdown")
  })
}

function resolveDrift(source) {
  emit("resolve-drift", {
    cli: activeRuntimeCli.value,
    source
  })
  closeDiffDialog()
}

function resolveImportConflict(source) {
  emit("resolve-import-conflict", {
    ruleId: diffPrompt.value.id,
    source,
    runtimeContent: diffRuntimeContent.value
  })
  closeDiffDialog()
}

function resolveDiff(source) {
  if (diffMode.value === "import") {
    resolveImportConflict(source)
    return
  }

  resolveDrift(source)
}

function formatTime(value) {
  return new Date(value || Date.now()).toLocaleString("zh-CN")
}

watch(
  () => [visibleCliTargets.value, props.rules],
  () => {
    ensureActiveCli()
    ensureSelectedPrompt()
    if (drawerPrompt.value) {
      const nextPrompt = scopedPrompts.value.find(
        (item) => item.id === drawerPrompt.value.id
      )

      drawerPrompt.value = nextPrompt || null
    }
  },
  { deep: true, immediate: true }
)

onBeforeUnmount(() => {
  if (diffEditor) {
    diffEditor.dispose()
  }
})
</script>

<style scoped lang="less">
.rules-view {
  display: flex;
  min-height: 100%;
  flex-direction: column;
  background: #ffffff;

  &__toolbar,
  &__toolbar-actions,
  &__cli-tabs,
  &__runtime,
  &__prompt-actions,
  &__diff-footer {
    display: flex;
    align-items: center;
  }

  &__toolbar {
    justify-content: space-between;
    gap: 12px;
    min-height: 58px;
    padding: 0 14px;
    border-bottom: 1px solid #edf0f3;
    background: #ffffff;
  }

  &__toolbar-actions {
    gap: 10px;
  }

  &__cli-tabs {
    justify-content: center;
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: #f5f6f8;
  }

  &__cli-tab {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 38px;
    padding: 0 16px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: #667085;
    cursor: pointer;
    font-weight: 600;
  }

  &__cli-tab--active {
    background: #ffffff;
    color: #111827;
    box-shadow: 0 1px 5px rgba(15, 23, 42, 0.08);
  }

  &__cli-icon {
    width: 18px;
    height: 18px;
  }

  &__add,
  &__secondary,
  &__compare-button,
  &__icon-button,
  &__state-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    background: transparent;
    color: #667085;
    cursor: pointer;
  }

  &__secondary {
    height: 38px;
    padding: 0 14px;
    border: 1px solid #dfe3e8;
    border-radius: 10px;
    background: #ffffff;
  }

  &__add {
    width: 38px;
    height: 38px;
    border-radius: 12px;
    background: var(--color-primary);
    color: #ffffff;
  }

  &__list-panel {
    display: flex;
    overflow: auto;
    flex: 1;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
  }

  &__prompt-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 15px 16px;
    border: 1px solid #dfe3e8;
    border-radius: 14px;
    background: #ffffff;
    cursor: pointer;
  }

  &__prompt-card--selected {
    border-color: #1682ff;
  }

  &__prompt-card--active {
    background: #eef7ff;
  }

  &__prompt-card--modified {
    border-color: #d92d20;
    background: #fff7f7;
  }

  &__prompt-main {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 6px;
  }

  &__prompt-main strong {
    color: #111827;
  }

  &__prompt-main span,
  &__prompt-main small {
    overflow: hidden;
    color: #667085;
    font-size: 0.82rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__prompt-actions {
    flex: none;
    gap: 8px;
  }

  &__target-buttons {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  &__target-button {
    display: inline-grid;
    width: 32px;
    height: 32px;
    place-items: center;
    overflow: hidden;
    border: 1px solid transparent;
    border-radius: 50%;
    background: #ffffff;
    color: #667085;
    cursor: pointer;
    font-size: 0.74rem;
    font-weight: 700;
  }

  &__target-button--active {
    border-color: #cbd6e4;
    background: #e8f8ee;
    color: #17803d;
  }

  &__target-button--modified {
    border-color: #f7c879;
    background: #fff4e5;
    color: #c25a00;
  }

  &__target-button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  &__target-icon {
    width: 18px;
    height: 18px;
    object-fit: contain;
  }

  &__icon-button {
    width: 32px;
    height: 32px;
    border-radius: 8px;
  }

  &__icon-button--danger {
    color: #c12626;
  }

  &__state-button {
    height: 30px;
    padding: 0 10px;
    border: 1px solid #dfe3e8;
    border-radius: 999px;
    background: #ffffff;
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__compare-button {
    height: 30px;
    padding: 0 10px;
    border: 1px solid #d92d20;
    border-radius: 999px;
    background: #ffffff;
    color: #b42318;
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__active-tag {
    padding: 3px 8px;
    border-radius: 999px;
    background: #1682ff;
    color: #ffffff;
    font-size: 0.72rem;
    font-weight: 700;
  }

  &__runtime {
    justify-content: space-between;
    gap: 14px;
    padding: 14px 16px;
    border-bottom: 1px solid #edf0f3;
    background: #fbfcfd;
  }

  &__runtime div {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 5px;
  }

  &__runtime strong {
    color: #111827;
  }

  &__runtime span {
    overflow: hidden;
    color: #667085;
    font-size: 0.84rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__status {
    flex: none;
    padding: 5px 10px;
    border-radius: 999px;
    background: #edf0f4;
    color: #667085;
    font-size: 0.78rem;
    font-weight: 700;
  }

  &__status--SYNCED {
    background: #e8f8ee;
    color: #17803d;
  }

  &__status--MODIFIED_EXTERNALLY,
  &__status--DIRTY_MANAGER,
  &__status--CONFLICT {
    background: #fff4e5;
    color: #c25a00;
  }

  &__empty {
    display: flex;
    min-height: 220px;
    align-items: center;
    justify-content: center;
    border: 1px dashed #d8dde5;
    border-radius: 14px;
    color: #667085;
    text-align: center;
  }

  &__editor {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 18px;
    overflow: hidden;
  }

  &__form-grid {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    min-height: 0;
    overflow: auto;
    gap: 16px;
    padding-right: 4px;
  }

  &__field {
    display: flex;
    min-width: 0;
    flex: 1 1 calc(50% - 8px);
    flex-direction: column;
    gap: 8px;
  }

  &__field--wide {
    flex-basis: 100%;
  }

  &__field span {
    color: #667085;
    font-size: 0.88rem;
  }

  &__field input,
  &__field select,
  &__field textarea {
    min-width: 0;
    padding: 0 12px;
    border: 1px solid #dfe3e8;
    border-radius: 8px;
    background: #ffffff;
    color: #111827;
  }

  &__field input,
  &__field select {
    height: 38px;
  }

  &__field textarea {
    height: 360px;
    padding: 12px;
    line-height: 1.6;
    resize: none;
  }

  &__editor-footer {
    display: flex;
    flex: none;
    justify-content: flex-end;
    gap: 10px;
    padding-top: 4px;
    background: #ffffff;
  }

  &__editor-footer button {
    height: 36px;
    padding: 0 14px;
    border: 0;
    border-radius: 8px;
    cursor: pointer;
    font-weight: 600;
  }

  &__editor-footer button[type="submit"] {
    background: #1682ff;
    color: #ffffff;
  }

  &__editor-footer button[type="button"] {
    border: 1px solid #dfe3e8;
    background: #ffffff;
    color: #667085;
  }

  &__diff-modal {
    :deep(.base-modal__panel) {
      width: 1180px;
    }

    :deep(.base-modal__header) {
      padding: 18px 22px 10px;
    }

    :deep(.base-modal__header h2) {
      color: #1f2937;
      font-size: 1.05rem;
      line-height: 1.35;
    }

    :deep(.base-modal__header p) {
      font-size: 0.86rem;
      line-height: 1.5;
    }
  }

  &__diff-editor {
    height: 560px;
    border: 1px solid #dfe3e8;
  }

  &__diff-footer {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding-top: 12px;
    background: #ffffff;
  }

  &__diff-button {
    min-width: 176px;
    height: 34px;
    padding: 0 14px;
    border: 1px solid #d0d5dd;
    border-radius: 7px;
    background: #ffffff;
    color: #475467;
    cursor: pointer;
    font-size: 0.86rem;
    font-weight: 600;
  }

  &__diff-button:hover:not(:disabled) {
    border-color: #b9c0cb;
    background: #f8fafc;
    color: #344054;
  }

  &__diff-button--primary {
    border-color: #1570ef;
    background: #1570ef;
    color: #ffffff;
  }

  &__diff-button--primary:hover:not(:disabled) {
    border-color: #175cd3;
    background: #175cd3;
    color: #ffffff;
  }

  &__diff-button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
}

.rules-drawer {
  position: fixed;
  inset: 0;
  z-index: 30;

  &__overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.2);
  }

  &__panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    display: flex;
    width: min(700px, 100%);
    flex-direction: column;
    border-left: 1px solid var(--color-line);
    background: var(--color-panel);
    box-shadow: var(--shadow-panel);
  }

  &__header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 18px;
    padding: 20px 22px 16px;
    border-bottom: 1px solid var(--color-line);
    background: #fbfcfd;
  }

  &__hero {
    display: flex;
    min-width: 0;
    gap: 14px;
  }

  &__title-wrap {
    min-width: 0;
  }

  &__title-wrap p {
    overflow: hidden;
    margin: 0 0 5px;
    color: var(--color-text-soft);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-overflow: ellipsis;
    text-transform: uppercase;
    white-space: nowrap;
  }

  &__title-wrap h2 {
    overflow: hidden;
    margin: 0 0 8px;
    color: var(--color-text);
    font-size: 1.42rem;
    line-height: 1.18;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__title-wrap span {
    color: var(--color-text-muted);
  }

  &__close {
    display: grid;
    width: 34px;
    height: 34px;
    flex: 0 0 34px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
  }

  &__tabs {
    display: flex;
    gap: 6px;
    padding: 10px 22px 0;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel);
  }

  &__tab {
    position: relative;
    padding: 9px 10px;
    border: 0;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.86rem;
    font-weight: 700;
  }

  &__tab--active {
    color: var(--color-text);
  }

  &__tab--active::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 2px;
    border-radius: 999px;
    background: var(--color-primary);
  }

  &__content {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 16px 22px 22px;
    background: var(--color-page);
  }

  &__section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  &__block {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 8px 22px rgba(34, 56, 83, 0.04);
  }

  &__block span {
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  &__block p {
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.88rem;
    line-height: 1.55;
  }

  &__content-text {
    max-height: 360px;
    margin: 0;
    overflow: auto;
    color: var(--color-text-muted);
    font-family: inherit;
    font-size: 0.84rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }

  &__grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  &__grid article {
    padding: 16px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  &__grid span {
    display: block;
    margin-bottom: 6px;
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  &__grid strong {
    color: var(--color-text);
    font-size: 0.88rem;
    line-height: 1.45;
    word-break: break-word;
  }

  &__footer {
    display: flex;
    gap: 8px;
    padding: 14px 22px 22px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel);
  }

  &__primary,
  &__secondary,
  &__danger,
  &__directory-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 36px;
    padding: 0 14px;
    border: 0;
    border-radius: 8px;
    cursor: pointer;
    font-weight: 600;
  }

  &__primary {
    background: #1682ff;
    color: #ffffff;
  }

  &__secondary,
  &__directory-button {
    border: 1px solid #dfe3e8;
    background: #ffffff;
    color: #667085;
  }

  &__directory-button {
    align-self: flex-start;
    gap: 6px;
  }

  &__danger {
    background: #fff1f1;
    color: #c12626;
  }
}
</style>
