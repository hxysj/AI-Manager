<template>
  <section class="assistant-capabilities">
    <nav class="assistant-capability-tabs" aria-label="助手能力管理">
      <button
        v-for="item in tabs"
        :key="item.id"
        class="assistant-capability-tab"
        :class="{ 'assistant-capability-active': tab === item.id }"
        type="button"
        :aria-pressed="tab === item.id"
        @click="tab = item.id"
      >
        {{ item.label }}
      </button>
    </nav>
    <div class="assistant-capability-content">
      <p v-if="error" class="assistant-capability-error" role="alert">
        {{ error }}
      </p>
      <p v-if="loading" class="assistant-capability-hint">正在加载…</p>
      <template v-if="tab === 'tools'">
        <p class="assistant-capability-hint">
          按能力模块管理模型可调用的工具，修改从下一轮对话生效。配置读取会过滤认证信息。
        </p>
        <section
          v-for="module in modules"
          :key="module"
          class="assistant-tool-module"
        >
          <h3 class="assistant-tool-module-title">{{ module }}</h3>
          <label
            v-for="tool in catalog.tools.filter(
              (item) => item.module === module
            )"
            :key="tool.id"
            class="assistant-capability-row"
          >
            <span class="assistant-capability-info"
              ><span class="assistant-capability-name">{{ tool.name }}</span
              ><code class="assistant-capability-id">{{ tool.id }}</code
              ><span class="assistant-capability-hint">{{
                tool.description
              }}</span></span
            >
            <input
              type="checkbox"
              :aria-label="'启用工具 ' + tool.name"
              :checked="!(settings.disabledTools || []).includes(tool.id)"
              :disabled="busy || saving"
              @change="toggleTool(tool.id, $event.target.checked)"
            />
          </label>
        </section>
      </template>
      <template v-else-if="tab === 'software'">
        <p class="assistant-capability-hint">
          这里展示软件已有的 Skills
          配置。复制到助手后独立管理，原技能的安装和启用状态不会随之改变。
        </p>
        <article
          v-for="skill in catalog.softwareSkills || []"
          :key="skill.id"
          class="assistant-capability-row"
        >
          <span class="assistant-capability-info"
            ><span class="assistant-capability-name">{{ skill.name }}</span
            ><span class="assistant-capability-hint">{{
              skill.description
            }}</span
            ><span class="assistant-capability-hint">{{
              skill.disabled ? "软件中已停用" : "软件中已启用"
            }}</span></span
          >
          <button
            class="assistant-capability-button"
            type="button"
            :disabled="busy || saving"
            @click="copySoftwareSkill(skill)"
          >
            复制到助手
          </button>
        </article>
        <p
          v-if="!loading && !catalog.softwareSkills?.length"
          class="assistant-capability-hint"
        >
          软件中还没有 Skills。
        </p>
      </template>
      <template v-else>
        <p class="assistant-capability-hint">
          助手技能是独立的文字指令，按需加载。支持新建、导入 Markdown 和从软件
          Skills 复制；不会自动执行脚本。
        </p>
        <div class="assistant-capability-actions">
          <button
            class="assistant-capability-button"
            type="button"
            :disabled="busy || saving"
            @click="editSkill()"
          >
            <Plus :size="14" />新建技能</button
          ><button
            class="assistant-capability-button"
            type="button"
            :disabled="busy || saving"
            @click="fileInput?.click()"
          >
            <Upload :size="14" />导入 Markdown</button
          ><input
            ref="fileInput"
            class="assistant-skill-file"
            type="file"
            accept=".md,text/markdown,text/plain"
            @change="importSkill"
          />
        </div>
        <form
          v-if="editing"
          class="assistant-skill-editor"
          @submit.prevent="saveSkill"
        >
          <label class="assistant-skill-field"
            ><span>技能名称</span
            ><input
              v-model.trim="editing.name"
              aria-label="技能名称"
              maxlength="100"
              required
          /></label>
          <label class="assistant-skill-field"
            ><span>用途说明</span
            ><input
              v-model.trim="editing.description"
              aria-label="技能用途"
              maxlength="500"
          /></label>
          <label class="assistant-skill-field"
            ><span>技能指令</span
            ><textarea
              v-model="editing.instructions"
              aria-label="技能指令"
              rows="9"
              maxlength="16000"
              required
            ></textarea>
          </label>
          <div class="assistant-capability-actions">
            <button
              class="assistant-capability-button"
              type="button"
              :disabled="saving"
              @click="editing = null"
            >
              取消</button
            ><button
              class="assistant-capability-button"
              :disabled="
                busy || saving || !editing.name || !editing.instructions.trim()
              "
            >
              {{ saving ? "保存中…" : "保存技能" }}
            </button>
          </div>
        </form>
        <article
          v-for="skill in settings.skills || []"
          :key="skill.id"
          class="assistant-capability-row"
        >
          <span class="assistant-capability-info"
            ><span class="assistant-capability-name">{{ skill.name }}</span
            ><span class="assistant-capability-hint">{{
              skill.description || "未填写用途说明"
            }}</span></span
          >
          <input
            type="checkbox"
            :aria-label="'启用技能 ' + skill.name"
            :checked="skill.enabled !== false"
            :disabled="busy || saving"
            @change="
              savePatch({
                skills: settings.skills.map((item) =>
                  item.id === skill.id
                    ? { ...item, enabled: $event.target.checked }
                    : item
                )
              })
            "
          />
          <button
            class="assistant-capability-icon"
            type="button"
            :aria-label="'编辑技能 ' + skill.name"
            :disabled="busy || saving"
            @click="editSkill(skill)"
          >
            <Pencil :size="15" />
          </button>
          <button
            class="assistant-capability-icon"
            type="button"
            :aria-label="'删除技能 ' + skill.name"
            :disabled="busy || saving"
            @click="
              savePatch({
                skills: settings.skills.filter((item) => item.id !== skill.id)
              })
            "
          >
            <Trash2 :size="15" />
          </button>
        </article>
        <p
          v-if="!settings.skills?.length && !editing"
          class="assistant-capability-hint"
        >
          还没有助手技能，添加常用工作流程即可开始。
        </p>
      </template>
    </div>
  </section>
</template>
<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from "vue"
import { Pencil, Plus, Trash2, Upload } from "lucide-vue-next"
import { assistantApi, settingsApi, skillApi } from "@/api"
const props = defineProps({
  settings: { type: Object, required: true },
  busy: Boolean
})
const emit = defineEmits(["change"])
const tab = ref("skills")
const tabs = [
  { id: "skills", label: "助手 Skills" },
  { id: "software", label: "软件 Skills" },
  { id: "tools", label: "工具管理" }
]
const catalog = ref({ tools: [], softwareSkills: [] })
const modules = computed(() => [
  ...new Set(catalog.value.tools.map((tool) => tool.module))
])
const loading = ref(false)
const saving = ref(false)
const error = ref("")
const editing = ref(null)
const fileInput = ref(null)
let disposed = false
// 仅保存当前管理项，保留模型、公共记忆和其他助手配置。
async function savePatch(patch) {
  if (props.busy || saving.value) return false
  saving.value = true
  error.value = ""
  try {
    await settingsApi.saveSettings({
      agents: { assistant: patch }
    })
    if (!disposed) emit("change", { ...props.settings, ...patch })
    return true
  } catch (failure) {
    if (!disposed) error.value = failure.message || String(failure)
    return false
  } finally {
    saving.value = false
  }
}
function toggleTool(id, enabled) {
  return savePatch({
    disabledTools: enabled
      ? (props.settings.disabledTools || []).filter((item) => item !== id)
      : [...new Set([...(props.settings.disabledTools || []), id])]
  })
}
function editSkill(skill) {
  editing.value = skill
    ? { ...skill }
    : {
        id: crypto.randomUUID(),
        name: "",
        description: "",
        instructions: "",
        enabled: true
      }
}
async function saveSkill() {
  if (!editing.value?.name || !editing.value.instructions.trim()) return
  const existing = props.settings.skills || []
  if (
    existing.length >= 32 &&
    !existing.some((skill) => skill.id === editing.value.id)
  ) {
    error.value = "助手最多保存 32 个技能"
    return
  }
  const skill = { ...editing.value }
  if (
    await savePatch({
      skills: existing.some((item) => item.id === skill.id)
        ? existing.map((item) => (item.id === skill.id ? skill : item))
        : [...existing, skill]
    })
  )
    editing.value = null
}
async function importSkill(event) {
  const file = event.target.files?.[0]
  if (!file) return
  try {
    if (file.size > 64000) throw new Error("技能正文最多 16000 个字符")
    const instructions = await file.text()
    if (instructions.length > 16000 || instructions.includes("\0"))
      throw new Error("请选择 16000 个字符以内的 Markdown 技能")
    if (disposed) return
    editSkill()
    editing.value.name = file.name.replace(/\.md$/i, "").slice(0, 100)
    editing.value.instructions = instructions
  } catch (failure) {
    if (!disposed) error.value = failure.message || String(failure)
  } finally {
    event.target.value = ""
  }
}
async function copySoftwareSkill(skill) {
  error.value = ""
  try {
    const result = await skillApi.getSkillContent({ skillName: skill.name })
    if (disposed) return
    if ((result.content || "").length > 16000)
      throw new Error("技能超过 16000 个字符，请精简后导入")
    editSkill()
    editing.value = {
      ...editing.value,
      name: skill.name.slice(0, 100),
      description: (skill.description || "").slice(0, 500),
      instructions: result.content || ""
    }
    tab.value = "skills"
  } catch (failure) {
    if (!disposed) error.value = failure.message || String(failure)
  }
}
onMounted(async () => {
  loading.value = true
  try {
    const result = await assistantApi.catalog()
    if (!disposed) catalog.value = result
  } catch (failure) {
    if (!disposed) error.value = failure.message || String(failure)
  } finally {
    loading.value = false
  }
})
onBeforeUnmount(() => {
  disposed = true
})
</script>
<style scoped lang="less">
.assistant-capabilities {
  display: flex;
  flex: 1;
  min-height: 0;
  flex-direction: column;
  color: var(--color-text);
  font-size: var(--font-size-sm);
  .assistant-capability-tabs {
    display: flex;
    flex: none;
    gap: 4px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--color-line);
    .assistant-capability-tab {
      flex: 1;
      padding: 8px;
      border: 0;
      border-radius: 6px;
      background: transparent;
      color: var(--color-text-muted);
      font: inherit;
      cursor: pointer;
      &.assistant-capability-active {
        background: var(--color-primary-soft);
        color: var(--color-primary);
      }
    }
  }
  .assistant-capability-content {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 16px;
    .assistant-capability-hint {
      margin: 0;
      color: var(--color-text-muted);
      font-size: var(--font-size-xs);
      line-height: 1.7;
    }
    .assistant-capability-error {
      margin: 0 0 12px;
      color: var(--color-danger);
    }
    .assistant-capability-actions {
      display: flex;
      gap: 8px;
      margin: 12px 0;
      .assistant-skill-file {
        display: none;
      }
    }
    .assistant-capability-button,
    .assistant-capability-icon {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      flex: none;
      gap: 6px;
      min-height: 32px;
      padding: 6px 10px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel);
      color: var(--color-text);
      font: inherit;
      cursor: pointer;
      &:hover {
        background: var(--color-panel-soft);
      }
      &:disabled {
        opacity: 0.45;
        cursor: default;
      }
    }
    .assistant-capability-icon {
      width: 28px;
      min-height: 28px;
      padding: 0;
      border-color: transparent;
      color: var(--color-text-muted);
    }
    .assistant-capability-row {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 14px 0;
      border-bottom: 1px solid var(--color-line);
      .assistant-capability-info {
        display: flex;
        flex: 1;
        min-width: 0;
        flex-direction: column;
        gap: 4px;
        .assistant-capability-name {
          font-size: var(--font-size-sm);
          overflow-wrap: anywhere;
        }
        .assistant-capability-id {
          color: var(--color-text-soft);
          font-size: var(--font-size-xs);
        }
      }
    }
    .assistant-tool-module {
      margin-top: 18px;
      .assistant-tool-module-title {
        margin: 0;
        font-size: var(--font-size-base);
      }
    }
    .assistant-skill-editor {
      margin: 12px 0;
      padding: 12px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);
      .assistant-skill-field {
        display: flex;
        flex-direction: column;
        gap: 6px;
        margin-bottom: 10px;
        :deep(input),
        :deep(textarea) {
          width: 100%;
          padding: 8px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel);
          color: var(--color-text);
          font: inherit;
          line-height: 1.6;
        }
      }
    }
  }
}
</style>
