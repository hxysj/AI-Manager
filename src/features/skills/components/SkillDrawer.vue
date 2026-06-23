<template>
  <div v-if="skill" class="skill-drawer">
    <div class="skill-drawer__overlay" @click="$emit('close')"></div>
    <aside class="skill-drawer__panel">
      <header class="skill-drawer__header">
        <div class="skill-drawer__hero">
          <div
            class="skill-drawer__icon"
            :style="{
              background: skill.icon ? '#ffffff' : hashColor(skill.name)
            }"
          >
            <img
              v-if="skill.icon"
              class="skill-drawer__icon-image"
              :src="toFileUrl(skill.icon)"
              :alt="skill.name"
            />
            <span v-else>{{ iconLetters(skill.name) }}</span>
          </div>
          <div class="skill-drawer__title-wrap">
            <p>{{ skill.repoName }}</p>
            <h2>{{ skill.name }}</h2>
            <span
              :class="[
                'skill-drawer__headline-status',
                `skill-drawer__headline-status--${skill.status}`
              ]"
            >
              {{ formatStatusLabel(skill.status) }}
            </span>
          </div>
        </div>
        <div class="skill-drawer__header-actions">
          <button
            :class="[
              'skill-drawer__enable-button',
              { 'skill-drawer__enable-button--disabled': skill.disabled }
            ]"
            type="button"
            @click="
              $emit('set-enabled', {
                skillName: skill.name,
                enabled: skill.disabled
              })
            "
          >
            <Power v-if="skill.disabled" :size="15" />
            <PowerOff v-else :size="15" />
            {{ skill.disabled ? "恢复" : "禁用" }}
          </button>
          <button
            class="skill-drawer__close"
            type="button"
            title="关闭"
            @click="$emit('close')"
          >
            <X :size="18" />
          </button>
        </div>
      </header>

      <div class="skill-drawer__tabs">
        <button
          v-for="item in tabs"
          :key="item.id"
          :class="[
            'skill-drawer__tab',
            { 'skill-drawer__tab--active': activeTab === item.id }
          ]"
          type="button"
          @click="activeTab = item.id"
        >
          {{ item.label }}
        </button>
      </div>

      <div class="skill-drawer__content">
        <section v-if="activeTab === 'overview'" class="skill-drawer__section">
          <div class="skill-drawer__block">
            <span>描述</span>
            <p>{{ skill.description || "未提供描述。" }}</p>
          </div>

          <div class="skill-drawer__block">
            <span>详细内容</span>
            <pre class="skill-drawer__content-text">{{
              skill.content || "未提供详细内容。"
            }}</pre>
          </div>

          <div class="skill-drawer__block">
            <span>标签</span>
            <div class="skill-drawer__tag-list">
              <strong v-for="tag in skill.tags" :key="tag">{{ tag }}</strong>
              <strong v-if="!skill.tags.length" class="skill-drawer__muted-tag">
                暂无标签
              </strong>
            </div>
          </div>

          <div class="skill-drawer__grid">
            <article>
              <span>Entry</span>
              <strong>{{ skill.entry }}</strong>
            </article>
            <article>
              <span>创建时间</span>
              <strong>{{ formatDateTime(skill.createdAt) }}</strong>
            </article>
            <article>
              <span>更新时间</span>
              <strong>{{ formatDateTime(skill.updatedAt) }}</strong>
            </article>
            <article>
              <span>作者</span>
              <strong>{{ skill.author || "未声明" }}</strong>
            </article>
          </div>
        </section>

        <section v-if="activeTab === 'targets'" class="skill-drawer__section">
          <article
            v-for="cli in cliTargets"
            :key="cli.id"
            class="skill-drawer__target-card"
          >
            <div class="skill-drawer__target-head">
              <div>
                <h3>{{ cli.name }}</h3>
                <p>{{ cli.skillsPath || "该 CLI 不支持 Skill 目录" }}</p>
              </div>
              <span
                :class="[
                  'skill-drawer__state-pill',
                  `skill-drawer__state-pill--${skill.installStates[cli.id]?.state}`
                ]"
              >
                {{ formatStatusLabel(skill.installStates[cli.id]?.state) }}
              </span>
            </div>
            <div class="skill-drawer__target-actions">
              <button
                v-if="skill.installStates[cli.id]?.state === 'installed'"
                class="action-button"
                type="button"
                @click="
                  $emit('uninstall', {
                    skillName: skill.name,
                    targetId: cli.id
                  })
                "
              >
                卸载
              </button>
              <button
                v-else-if="skill.installStates[cli.id]?.state === 'broken-link'"
                class="action-button action-button--alert"
                type="button"
                :disabled="skill.disabled"
                @click="
                  $emit('repair', {
                    skillName: skill.name,
                    targetId: cli.id
                  })
                "
              >
                修复链接
              </button>
              <button
                v-else
                class="action-button action-button--primary"
                type="button"
                :disabled="skill.disabled || !cli.installed"
                @click="
                  $emit('install', {
                    skillName: skill.name,
                    targetId: cli.id
                  })
                "
              >
                安装
              </button>
              <span v-if="skill.disabled" class="skill-drawer__target-note">
                已禁用，恢复后才能安装
              </span>
              <button
                class="action-button"
                type="button"
                @click="$emit('open-path', cli.skillsPath)"
              >
                打开目录
              </button>
            </div>
          </article>
        </section>

        <section v-if="activeTab === 'files'" class="skill-drawer__section">
          <div class="skill-drawer__block">
            <span>源目录</span>
            <button
              class="skill-drawer__path-button"
              type="button"
              @click="$emit('open-path', skill.sourcePath)"
            >
              {{ skill.sourcePath }}
            </button>
          </div>
          <div class="skill-drawer__block">
            <span>入口文件</span>
            <button
              class="skill-drawer__path-button"
              type="button"
              @click="$emit('open-path', skill.entryPath)"
            >
              {{ skill.entryPath }}
            </button>
          </div>
          <div class="skill-drawer__block">
            <span>图标文件</span>
            <p>{{ skill.icon || "未提供 icon，将使用默认图标。" }}</p>
          </div>
          <div class="skill-drawer__file-browser">
            <div class="skill-drawer__file-tree">
              <div class="skill-drawer__file-tree-head">
                <span>目录内容</span>
                <strong>{{ skillFileRows.length }} 项</strong>
              </div>

              <div v-if="skillFilesLoading" class="skill-drawer__file-message">
                正在读取 Skill 目录...
              </div>
              <div
                v-else-if="skillFilesError"
                class="skill-drawer__file-message skill-drawer__file-message--error"
              >
                {{ skillFilesError }}
              </div>
              <div
                v-else-if="skillFileRows.length"
                class="skill-drawer__file-list"
              >
                <button
                  v-for="row in skillFileRows"
                  :key="row.path"
                  :class="[
                    'skill-drawer__file-row',
                    `skill-drawer__file-row--${row.type}`,
                    {
                      'skill-drawer__file-row--active':
                        selectedSkillFilePath === row.path
                    }
                  ]"
                  type="button"
                  :style="{ paddingLeft: `${row.depth * 14 + 10}px` }"
                  @click="selectSkillFile(row)"
                >
                  <Folder v-if="row.type === 'dir'" :size="14" />
                  <Link2 v-else-if="row.type === 'symlink'" :size="14" />
                  <FileText v-else :size="14" />
                  <span>{{ row.name }}</span>
                  <small v-if="row.type === 'file'">
                    {{ formatSkillFileSize(row.size) }}
                  </small>
                </button>
              </div>
              <div v-else class="skill-drawer__file-message">
                暂无目录内容。
              </div>
            </div>

            <div class="skill-drawer__file-preview">
              <div class="skill-drawer__file-preview-head">
                <span>{{ selectedSkillFile?.path || "未选择文件" }}</span>
                <strong>{{
                  selectedSkillFile
                    ? formatSkillFileType(selectedSkillFile)
                    : "内容预览"
                }}</strong>
              </div>

              <pre
                v-if="
                  selectedSkillFile?.type === 'file' &&
                  selectedSkillFile.previewable
                "
                class="skill-drawer__file-content"
                >{{ selectedSkillFile.content }}</pre
              >
              <div
                v-else-if="selectedSkillFile?.type === 'file'"
                class="skill-drawer__file-message"
              >
                该文件不可直接预览，可从源目录打开查看。
              </div>
              <div
                v-else-if="selectedSkillFile?.type === 'symlink'"
                class="skill-drawer__file-message"
              >
                链接目标：{{ selectedSkillFile.target }}
              </div>
              <div
                v-else-if="selectedSkillFile"
                class="skill-drawer__file-message"
              >
                目录项：{{ selectedSkillFile.path }}
              </div>
              <div v-else class="skill-drawer__file-message">
                选择左侧文件查看内容。
              </div>
            </div>
          </div>
        </section>
      </div>
    </aside>
  </div>
</template>

<script setup>
import { computed, ref, watch } from "vue"
import { FileText, Folder, Link2, Power, PowerOff, X } from "lucide-vue-next"
import { skillApi } from "@/api"
import {
  formatDateTime,
  formatStatusLabel,
  hashColor,
  iconLetters
} from "@/utils/formatters"

const props = defineProps({
  cliTargets: {
    type: Array,
    required: true
  },
  skill: {
    type: Object,
    default: null
  }
})

defineEmits([
  "close",
  "install",
  "uninstall",
  "repair",
  "open-path",
  "set-enabled"
])

const tabs = [
  { id: "overview", label: "Overview" },
  { id: "targets", label: "Targets" },
  { id: "files", label: "Files" }
]

const activeTab = ref("overview")
const skillFiles = ref([])
const skillFilesLoading = ref(false)
const skillFilesError = ref("")
const selectedSkillFilePath = ref("")

const skillFileRows = computed(() =>
  skillFiles.value.map((item) => ({
    ...item,
    depth: item.path.split("/").length - 1
  }))
)
const selectedSkillFile = computed(() => {
  return (
    skillFiles.value.find(
      (item) => item.path === selectedSkillFilePath.value
    ) || null
  )
})

watch(
  () => props.skill?.name,
  () => {
    activeTab.value = "overview"
    skillFiles.value = []
    skillFilesError.value = ""
    selectedSkillFilePath.value = ""
  }
)

watch(activeTab, (value) => {
  if (value === "files") {
    loadSkillFiles()
  }
})

function toFileUrl(value) {
  return encodeURI(`file:///${String(value).replace(/\\/g, "/")}`)
}

async function loadSkillFiles() {
  if (!props.skill) {
    return
  }

  skillFilesLoading.value = true
  skillFilesError.value = ""

  try {
    const result = await skillApi.getSkillFiles({
      skillName: props.skill.name
    })
    skillFiles.value = result.entries || []

    const entryFile =
      skillFiles.value.find((item) => item.path === props.skill.entry) ||
      skillFiles.value.find((item) => item.path === "SKILL.md")
    const previewFile = skillFiles.value.find((item) => item.previewable)
    const firstFile = skillFiles.value.find((item) => item.type === "file")
    selectedSkillFilePath.value =
      (entryFile || previewFile || firstFile || skillFiles.value[0])?.path || ""
  } catch (error) {
    skillFiles.value = []
    selectedSkillFilePath.value = ""
    skillFilesError.value = error.message || String(error)
  } finally {
    skillFilesLoading.value = false
  }
}

function selectSkillFile(file) {
  selectedSkillFilePath.value = file.path
}

function formatSkillFileSize(value) {
  const size = Number(value || 0)

  if (size < 1024) {
    return `${size} B`
  }

  if (size < 1024 * 1024) {
    return `${(size / 1024).toFixed(1)} KB`
  }

  return `${(size / 1024 / 1024).toFixed(1)} MB`
}

function formatSkillFileType(file) {
  if (file.type === "dir") {
    return "目录"
  }

  if (file.type === "symlink") {
    return "链接"
  }

  return file.previewable ? "文本内容" : "不可预览"
}
</script>

<style scoped lang="less">
.skill-drawer {
  position: fixed;
  inset: 0;
  z-index: 30;

  &__overlay {
    position: absolute;
    inset: 0;
    background: rgba(15, 23, 42, 0.24);
  }

  &__panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    display: flex;
    width: 620px;
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

  &__header-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
  }

  &__icon {
    display: grid;
    width: 56px;
    height: 56px;
    flex: 0 0 56px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    color: var(--color-text-muted);
    font-size: 1.1rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    overflow: hidden;
  }

  &__icon-image {
    width: 100%;
    height: 100%;
    object-fit: contain;
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

  &__headline-status,
  &__state-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 24px;
    padding: 4px 9px;
    border-radius: 999px;
    font-size: 0.7rem;
    font-weight: 700;
    line-height: 1.2;
  }

  &__headline-status--installed,
  &__state-pill--installed {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  &__headline-status--not-installed,
  &__state-pill--not-installed {
    background: var(--color-primary-soft);
    color: var(--color-text-muted);
  }

  &__headline-status--broken-link,
  &__state-pill--broken-link {
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  &__headline-status--disabled,
  &__state-pill--disabled {
    background: var(--color-primary-soft);
    color: var(--color-text-soft);
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

  &__enable-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 34px;
    padding: 0 11px;
    border: 1px solid #ead1d1;
    border-radius: 8px;
    background: var(--color-danger-soft);
    color: var(--color-danger);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
    white-space: nowrap;
  }

  &__enable-button:hover {
    border-color: #e3b7b7;
    background: var(--color-danger-soft);
  }

  &__enable-button--disabled {
    border-color: #d8e4ee;
    background: #edf3f8;
    color: var(--color-primary);
  }

  &__close:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
    color: var(--color-primary);
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

  &__tag-list {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  &__tag-list strong {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 24px;
    padding: 4px 9px;
    border-radius: 999px;
    background: var(--color-primary-soft);
    color: var(--color-text-muted);
    font-size: 0.76rem;
  }

  &__muted-tag {
    color: var(--color-text-soft);
  }

  &__grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  &__grid article,
  &__target-card {
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 8px 22px rgba(34, 56, 83, 0.04);
  }

  &__grid article span {
    display: block;
    margin-bottom: 6px;
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
  }

  &__grid article strong {
    color: var(--color-text);
    font-size: 0.88rem;
    line-height: 1.45;
    word-break: break-word;
  }

  &__target-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  &__target-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  &__target-head h3 {
    margin: 0 0 6px;
    color: var(--color-text);
    font-size: 0.96rem;
    line-height: 1.2;
  }

  &__target-head p {
    overflow: hidden;
    margin: 0;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.45;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__target-head > div {
    min-width: 0;
  }

  &__target-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  &__target-note {
    color: var(--color-text-soft);
    font-size: 0.76rem;
    font-weight: 700;
  }

  &__path-button {
    overflow: hidden;
    padding: 8px 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.82rem;
    line-height: 1.45;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__path-button:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
  }

  &__file-browser {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    gap: 10px;
    min-height: 380px;
  }

  &__file-tree,
  &__file-preview {
    display: flex;
    min-height: 0;
    flex-direction: column;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    overflow: hidden;
    box-shadow: 0 8px 22px rgba(34, 56, 83, 0.04);
  }

  &__file-tree-head,
  &__file-preview-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 38px;
    padding: 0 10px;
    border-bottom: 1px solid var(--color-line);
    background: #fbfcfd;
  }

  &__file-tree-head span,
  &__file-preview-head span {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.74rem;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__file-tree-head strong,
  &__file-preview-head strong {
    flex: none;
    color: var(--color-text-soft);
    font-size: 0.72rem;
  }

  &__file-list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 6px;
  }

  &__file-row {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 7px;
    min-height: 30px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.78rem;
    text-align: left;
  }

  &__file-row:hover,
  &__file-row--active {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  &__file-row svg {
    flex: 0 0 auto;
  }

  &__file-row span {
    min-width: 0;
    overflow: hidden;
    flex: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &__file-row small {
    flex: none;
    color: var(--color-text-soft);
    font-size: 0.68rem;
  }

  &__file-row--dir {
    color: var(--color-text);
    font-weight: 700;
  }

  &__file-content {
    flex: 1;
    min-height: 0;
    margin: 0;
    overflow: auto;
    padding: 12px;
    background: #ffffff;
    color: var(--color-text);
    font-family: "JetBrains Mono", "Consolas", monospace;
    font-size: 0.76rem;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }

  &__file-message {
    display: grid;
    flex: 1;
    min-height: 120px;
    place-items: center;
    padding: 14px;
    color: var(--color-text-muted);
    font-size: 0.82rem;
    line-height: 1.55;
    text-align: center;
    word-break: break-word;
  }

  &__file-message--error {
    color: var(--color-danger);
  }
}

.action-button {
  height: 34px;
  padding: 0 12px;
  border: 1px solid var(--color-line);
  border-radius: 8px;
  background: var(--color-panel);
  color: var(--color-primary);
  cursor: pointer;
  font-size: 0.84rem;
  font-weight: 600;

  &:hover {
    border-color: #b9ccda;
    background: var(--color-primary-soft);
  }

  &--primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #fff;
  }

  &--primary:hover {
    border-color: #2a4f6f;
    background: #2a4f6f;
  }

  &--alert {
    border-color: var(--color-danger-soft);
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  &--alert:hover {
    border-color: #ead1d1;
    background: var(--color-danger-soft);
  }

  &:disabled {
    cursor: not-allowed;
    opacity: 0.46;
  }
}
</style>
