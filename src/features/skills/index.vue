<template>
  <section class="skills-view">
    <section v-if="viewMode === 'local'" class="skills-view-local-page">
      <header class="skills-view-head">
        <div class="skills-view-title">
          <p class="skills-view-mark">Skill Registry</p>
          <h1 class="skills-view-title-text">Skills 管理</h1>
        </div>
        <div class="skills-view-actions">
          <button
            class="skills-view-button primary"
            type="button"
            @click="viewMode = 'repository-skills'"
          >
            <Library :size="16" />
            Skill 仓库
          </button>
          <button
            class="skills-view-button"
            type="button"
            @click="openTrashView"
          >
            <Trash2 :size="16" />
            回收站
          </button>
          <button
            class="skills-view-button primary"
            type="button"
            @click="$emit('create-skill')"
          >
            <Plus :size="16" />
            新建 Skill
          </button>
          <button
            class="skills-view-button"
            type="button"
            @click="$emit('import-skills')"
          >
            <Download :size="16" />
            导入Skill
          </button>
          <button
            class="skills-view-button"
            type="button"
            @click="$emit('import-zip-skill')"
          >
            <Archive :size="16" />
            导入 zip
          </button>
          <button
            class="skills-view-button"
            type="button"
            @click="$emit('open-usage')"
          >
            <BarChart3 :size="16" />
            使用统计
          </button>
          <button
            class="skills-view-button"
            type="button"
            @click="$emit('open-path', paths.skillsDir)"
          >
            <FolderOpen :size="16" />
            打开 Skills 目录
          </button>
          <button
            class="skills-view-button"
            type="button"
            @click="$emit('refresh')"
          >
            <RefreshCw :size="16" />
            刷新扫描
          </button>
        </div>
      </header>

      <section class="skills-view-filter-card">
        <label class="skills-view-field search">
          <span class="skills-view-field-label">搜索</span>
          <input
            v-model.trim="searchQuery"
            class="skills-view-field-control"
            type="text"
            placeholder="name / tags / description / repo"
          />
        </label>
        <label class="skills-view-field status">
          <span class="skills-view-field-label">状态</span>
          <select v-model="statusFilter" class="skills-view-field-control">
            <option value="all">全部</option>
            <option value="installed">已安装</option>
            <option value="not-installed">未安装</option>
            <option value="broken-link">链接损坏</option>
            <option value="disabled">不可用</option>
          </select>
        </label>
        <label class="skills-view-field group">
          <span class="skills-view-field-label">分组</span>
          <span class="skills-view-field-row">
            <select v-model="groupFilter" class="skills-view-field-control">
              <option value="all">全部分组</option>
              <option value="ungrouped">未分组</option>
              <option
                v-for="group in skillGroupsView"
                :key="group.id"
                :value="group.id"
              >
                {{ group.name }}
              </option>
            </select>
            <button
              class="skills-view-field-button"
              type="button"
              @click="openGroupManageDialog"
            >
              管理
            </button>
          </span>
        </label>
      </section>

      <section class="skills-view-batch-card">
        <label class="skills-view-check-all">
          <input
            v-model="allFilteredSelected"
            class="skills-view-check-input"
            type="checkbox"
          />
          <span
            >已选 {{ selectedSkillIds.length }} /
            {{ filteredSkills.length }}</span
          >
        </label>
        <div class="skills-view-batch-actions">
          <button
            v-if="canBatchInstall"
            class="skills-view-button"
            type="button"
            @click="openInstallDialog(batchInstallSkillIds)"
          >
            <Power :size="16" />
            一键安装
          </button>
          <button
            v-if="canBatchUninstall"
            class="skills-view-button"
            type="button"
            @click="emitBatchAction('uninstall-all', batchUninstallSkillIds)"
          >
            <PowerOff :size="16" />
            一键卸载
          </button>
          <button
            v-if="canBatchDisable"
            class="skills-view-button"
            type="button"
            @click="emitBatchAction('disable', batchDisableSkillIds)"
          >
            <Ban :size="16" />
            一键禁用
          </button>
          <button
            v-if="canBatchEnable"
            class="skills-view-button"
            type="button"
            @click="emitBatchAction('enable', batchEnableSkillIds)"
          >
            <RotateCcw :size="16" />
            一键恢复
          </button>
          <button
            v-if="hasSelectedSkills && skillGroupsView.length"
            class="skills-view-button"
            type="button"
            @click="openMoveGroupDialog"
          >
            移动到分组
          </button>
          <button
            v-if="canBatchRemoveFromGroup"
            class="skills-view-button"
            type="button"
            @click="removeSelectedFromGroup"
          >
            移出分组
          </button>
          <button
            v-if="hasSelectedSkills"
            class="skills-view-button danger"
            type="button"
            @click="deleteSelectedSkills"
          >
            <Trash2 :size="16" />
            删除
          </button>
        </div>
      </section>

      <div class="skills-view-result-head">
        <span>{{ filteredSkills.length }} / {{ skills.length }} 个 Skill</span>
        <span>Centralized Skill Source + Junction Mount</span>
      </div>

      <div v-if="filteredSkills.length" class="skills-view-list">
        <SkillCard
          v-for="skill in filteredSkills"
          :key="skill.id"
          :cli-targets="cliTargets"
          :group-name="skillGroupLabel(skill.id)"
          :selected="selectedSkillIds.includes(skill.id)"
          :skill="skill"
          @toggle-select="toggleSkillSelection(skill.id)"
          @select="$emit('select-skill', skill)"
          @open-source="$emit('open-path', skill.sourcePath)"
          @install="$emit('install-skill', $event)"
          @set-enabled="$emit('set-skill-enabled', $event)"
          @uninstall="$emit('uninstall-skill', $event)"
        />
      </div>

      <div v-else class="skills-view-empty">
        <strong class="skills-view-empty-title">没有匹配的 Skill</strong>
        <span class="skills-view-empty-desc">
          可以先在本地 skills 目录创建 Skill，或者调整搜索条件。
        </span>
      </div>
    </section>

    <section v-else-if="viewMode === 'trash'" class="skills-view-trash-page">
      <header class="skills-view-head">
        <button
          class="skills-view-back"
          type="button"
          @click="viewMode = 'local'"
        >
          <ArrowLeft :size="16" />
          返回
        </button>
        <div class="skills-view-title skills-view-title-wide">
          <p class="skills-view-mark">Trash</p>
          <h1 class="skills-view-title-text">Skill 回收站</h1>
        </div>
        <div class="skills-view-actions">
          <button
            class="skills-view-button"
            type="button"
            @click="openTrashView"
          >
            <RefreshCw :size="16" />
            刷新
          </button>
        </div>
      </header>

      <section class="skills-view-batch-card">
        <label class="skills-view-check-all">
          <input
            v-model="allTrashSelected"
            class="skills-view-check-input"
            type="checkbox"
          />
          <span
            >已选 {{ selectedTrashIds.length }} /
            {{ skillTrashItems.length }}</span
          >
        </label>
        <div class="skills-view-batch-actions">
          <button
            class="skills-view-button"
            type="button"
            :disabled="!selectedTrashIds.length"
            @click="restoreSelectedTrash"
          >
            <RotateCcw :size="16" />
            一键恢复
          </button>
          <button
            class="skills-view-button danger"
            type="button"
            :disabled="!selectedTrashIds.length"
            @click="purgeSelectedTrash"
          >
            <Trash2 :size="16" />
            永久删除
          </button>
        </div>
      </section>

      <div v-if="skillTrashItems.length" class="skills-view-list">
        <article
          v-for="item in skillTrashItems"
          :key="item.id"
          class="skills-view-trash-item"
        >
          <label class="skills-view-trash-check">
            <input
              v-model="selectedTrashIds"
              class="skills-view-check-input"
              type="checkbox"
              :value="item.id"
            />
          </label>
          <div class="skills-view-trash-main">
            <strong class="skills-view-trash-name">{{ item.name }}</strong>
            <span class="skills-view-trash-path" :title="item.sourcePath">
              {{ item.sourcePath }}
            </span>
          </div>
          <div class="skills-view-trash-meta">
            <span>删除：{{ formatDateTime(item.deletedAt) }}</span>
            <span>过期：{{ formatDateTime(item.expiresAt) }}</span>
          </div>
        </article>
      </div>

      <div v-else class="skills-view-empty">
        <strong class="skills-view-empty-title">回收站为空</strong>
        <span class="skills-view-empty-desc">
          删除的 Skill 会保留 10 天，过期后自动清理。
        </span>
      </div>
    </section>

    <SkillRepositoryList
      v-else-if="viewMode === 'repository-skills'"
      :repositories="skillRepositories"
      :skills="skills"
      @back="viewMode = 'local'"
      @install-skill="installRepositorySkill"
      @open-manager="viewMode = 'repositories'"
      @refresh="$emit('refresh')"
    />

    <SkillRepositoryManager
      v-else
      :repositories="skillRepositories"
      @add-repository="$emit('add-skill-repository', $event)"
      @back="viewMode = 'repository-skills'"
      @refresh-repository="refreshRepository"
      @remove-repository="removeRepository"
    />

    <BaseModal
      v-if="showMoveGroupDialog"
      class="skills-view-move-modal"
      title="移动到分组"
      @close="closeMoveGroupDialog"
    >
      <form
        class="skills-view-move-form"
        @submit.prevent="moveSelectedToGroup"
      >
        <div class="skills-view-move-selection">
          <span class="skills-view-move-selection-icon">
            <FolderInput :size="18" />
          </span>
          <div class="skills-view-move-selection-main">
            <strong class="skills-view-move-selection-title">
              已选择 {{ selectedSkillIds.length }} 个 Skill
            </strong>
            <span
              class="skills-view-move-selection-names"
              :title="selectedSkillNames.join('、')"
            >
              {{ moveSkillPreview }}
            </span>
          </div>
        </div>
        <section
          class="skills-view-move-targets"
          aria-labelledby="skills-view-move-target-title"
        >
          <h3
            id="skills-view-move-target-title"
            class="skills-view-move-target-title"
          >
            选择目标分组
          </h3>
          <div class="skills-view-move-target-list">
            <label
              v-for="group in skillGroupsView"
              :key="group.id"
              class="skills-view-move-target"
              :class="{ active: moveGroupId === group.id }"
            >
              <input
                v-model="moveGroupId"
                type="radio"
                name="skill-move-target"
                :value="group.id"
              />
              <span class="skills-view-move-radio">
                <Check v-if="moveGroupId === group.id" :size="13" />
              </span>
              <span class="skills-view-move-target-main">
                <strong
                  class="skills-view-move-target-name"
                  :title="group.name"
                >
                  <Folder :size="15" />
                  {{ group.name }}
                </strong>
                <span class="skills-view-move-target-meta">
                  {{ group.skills.length }} 个 Skill
                  <template v-if="group.missingCount">
                    · {{ group.missingCount }} 项未找到
                  </template>
                </span>
              </span>
              <span
                v-if="moveGroupAddedCount(group)"
                class="skills-view-move-target-change"
                :title="`将新增 ${moveGroupAddedCount(group)} 个 Skill`"
              >
                +{{ moveGroupAddedCount(group) }}
              </span>
            </label>
          </div>
        </section>
        <footer class="skills-view-dialog-actions">
          <span class="skills-view-move-current-target">
            目标：{{ moveTargetGroup?.name || "未选择分组" }}
          </span>
          <button
            class="skills-view-dialog-button"
            type="button"
            @click="closeMoveGroupDialog"
          >
            取消
          </button>
          <button
            class="skills-view-dialog-button primary"
            type="submit"
            :disabled="!moveGroupId"
          >
            <FolderInput :size="15" />
            移动
          </button>
        </footer>
      </form>
    </BaseModal>

    <BaseModal
      v-if="showGroupManageDialog"
      class="skills-view-group-modal"
      title="分组管理"
      :description="`${skillGroupsView.length} 个分组`"
      @close="closeGroupManageDialog"
    >
      <section class="skills-view-group-manage">
        <form
          class="skills-view-group-create"
          @submit.prevent="createManagedGroup"
        >
          <strong class="skills-view-group-section-title">新建分组</strong>
          <label class="skills-view-group-create-field">
            <span class="skills-view-sr-only">分组名称</span>
            <input
              v-model.trim="groupName"
              class="skills-view-dialog-control"
              type="text"
              placeholder="输入分组名称"
            />
          </label>
          <button
            class="skills-view-dialog-button primary skills-view-group-create-button"
            type="submit"
            :disabled="!groupName.trim()"
          >
            <Plus :size="15" />
            创建
          </button>
        </form>
        <div class="skills-view-group-list-head" aria-hidden="true">
          <span>已有分组</span>
          <span>操作</span>
        </div>
        <div class="skills-view-group-list">
          <article
            v-for="group in skillGroupsView"
            :key="group.id"
            class="skills-view-group-manage-item"
            :class="{
              editing: editingGroupId === group.id,
              deleting: deletingGroupId === group.id
            }"
          >
            <form
              v-if="editingGroupId === group.id"
              class="skills-view-group-edit"
              @submit.prevent="renameGroup(group)"
            >
              <input
                :ref="setGroupRenameInput"
                v-model="groupRenameDraft"
                class="skills-view-dialog-control"
                type="text"
                aria-label="分组名称"
                @keydown.esc.prevent="cancelRenameGroup"
              />
              <div class="skills-view-group-manage-actions">
                <button
                  class="skills-view-group-icon-button"
                  type="button"
                  title="取消重命名"
                  aria-label="取消重命名"
                  @click="cancelRenameGroup"
                >
                  <X :size="16" />
                </button>
                <button
                  class="skills-view-group-icon-button primary"
                  type="submit"
                  title="保存名称"
                  aria-label="保存名称"
                  :disabled="!canRenameGroup(group)"
                >
                  <Check :size="16" />
                </button>
              </div>
            </form>
            <template v-else>
              <div class="skills-view-group-manage-main">
                <div class="skills-view-group-name-row">
                  <Folder :size="16" />
                  <strong :title="group.name">{{ group.name }}</strong>
                </div>
                <div class="skills-view-group-manage-meta">
                  <span>{{ group.skills.length }} 个 Skill</span>
                  <span
                    v-if="group.missingCount"
                    class="skills-view-group-missing"
                  >
                    {{ group.missingCount }} 项未找到
                  </span>
                  <button
                    class="skills-view-group-id-button"
                    type="button"
                    :title="`复制分组 ID：${group.id}`"
                    @click="copyGroupId(group)"
                  >
                    <Copy :size="12" />
                    复制 ID
                  </button>
                </div>
              </div>
              <div
                v-if="deletingGroupId === group.id"
                class="skills-view-group-delete-confirm"
                role="group"
                :aria-label="`确认删除分组 ${group.name}`"
              >
                <span>确认删除？</span>
                <button
                  class="skills-view-group-confirm-button"
                  type="button"
                  @click="cancelRemoveGroup"
                >
                  取消
                </button>
                <button
                  class="skills-view-group-confirm-button danger"
                  type="button"
                  @click="confirmRemoveGroup(group)"
                >
                  删除
                </button>
              </div>
              <div v-else class="skills-view-group-manage-actions">
                <button
                  class="skills-view-group-icon-button"
                  type="button"
                  title="重命名"
                  aria-label="重命名"
                  @click="startRenameGroup(group)"
                >
                  <Pencil :size="15" />
                </button>
                <button
                  class="skills-view-group-icon-button danger"
                  type="button"
                  title="删除分组"
                  aria-label="删除分组"
                  @click="requestRemoveGroup(group)"
                >
                  <Trash2 :size="15" />
                </button>
              </div>
            </template>
          </article>
          <div
            v-if="!skillGroupsView.length"
            class="skills-view-dialog-empty skills-view-group-empty"
          >
            <Folder :size="20" />
            <span>暂无分组</span>
          </div>
        </div>
      </section>
    </BaseModal>

    <BaseModal
      v-if="installDialog"
      title="选择安装目标 CLI"
      :description="`将 ${installDialog.skillIds.length} 个 Skill 安装到选中的 CLI，可多选。`"
      @close="closeInstallDialog"
    >
      <form
        class="skills-view-install-form"
        @submit.prevent="submitInstallTargets"
      >
        <label class="skills-view-check-all">
          <input
            v-model="allInstallTargetsSelected"
            class="skills-view-check-input"
            type="checkbox"
          />
          <span>全部 CLI</span>
        </label>
        <div class="skills-view-install-list">
          <label
            v-for="cli in installedCliTargets"
            :key="cli.id"
            class="skills-view-install-item"
          >
            <input
              v-model="selectedInstallTargetIds"
              class="skills-view-check-input"
              type="checkbox"
              :value="cli.id"
            />
            <span>{{ cli.name }}</span>
          </label>
        </div>
        <div
          v-if="!installedCliTargets.length"
          class="skills-view-dialog-empty"
        >
          当前没有可安装的 CLI。
        </div>
        <footer class="skills-view-dialog-actions">
          <button
            class="skills-view-dialog-button"
            type="button"
            @click="closeInstallDialog"
          >
            取消
          </button>
          <button
            class="skills-view-dialog-button primary"
            type="submit"
            :disabled="!selectedInstallTargetIds.length"
          >
            确认安装
          </button>
        </footer>
      </form>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, nextTick, ref, watch } from "vue"
import {
  ArrowLeft,
  Archive,
  Ban,
  BarChart3,
  Check,
  Copy,
  Download,
  Folder,
  FolderInput,
  FolderOpen,
  Library,
  Pencil,
  Plus,
  Power,
  PowerOff,
  RefreshCw,
  RotateCcw,
  Trash2,
  X
} from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import SkillCard from "./components/SkillCard.vue"
import SkillRepositoryList from "./components/SkillRepositoryList.vue"
import SkillRepositoryManager from "./components/SkillRepositoryManager.vue"
import { formatDateTime } from "@/utils/formatters"
import { createMessage } from "@/utils/message"

const props = defineProps({
  cliTargets: {
    type: Array,
    required: true
  },
  paths: {
    type: Object,
    required: true
  },
  skillGroups: {
    type: Array,
    default: () => []
  },
  skillRepositories: {
    type: Array,
    default: () => []
  },
  skillTrashItems: {
    type: Array,
    default: () => []
  },
  skills: {
    type: Array,
    required: true
  }
})

const emit = defineEmits([
  "add-skill-repository",
  "batch-skill-action",
  "create-skill",
  "delete-skills",
  "import-skills",
  "import-zip-skill",
  "install-repository-skill",
  "install-skill",
  "open-path",
  "open-usage",
  "refresh",
  "refresh-skill-repository",
  "remove-skill-group-items",
  "remove-skill-group",
  "remove-skill-repository",
  "restore-skill-trash",
  "save-skill-group",
  "select-skill",
  "set-skill-enabled",
  "show-skill-trash",
  "purge-skill-trash",
  "uninstall-skill"
])

const viewMode = ref("local")
const searchQuery = ref("")
const statusFilter = ref("all")
const groupFilter = ref("all")
const selectedSkillIds = ref([])
const selectedTrashIds = ref([])
const showMoveGroupDialog = ref(false)
const showGroupManageDialog = ref(false)
const groupName = ref("")
const moveGroupId = ref("")
const editingGroupId = ref("")
const deletingGroupId = ref("")
const groupRenameDraft = ref("")
const groupRenameInput = ref(null)
const installDialog = ref(null)
const selectedInstallTargetIds = ref([])

const skillById = computed(() => {
  return new Map(props.skills.map((skill) => [skill.id, skill]))
})

const installedCliTargets = computed(() => {
  return props.cliTargets.filter((cli) => cli.installed)
})

const selectedSkillNames = computed(() => {
  return selectedSkillIds.value
    .map((skillId) => skillById.value.get(skillId)?.name)
    .filter(Boolean)
})

// 名称摘要限制为三项，避免大量选择撑开弹窗。
const moveSkillPreview = computed(() => {
  const visibleNames = selectedSkillNames.value.slice(0, 3)
  const remainingCount = selectedSkillNames.value.length - visibleNames.length

  if (!visibleNames.length) {
    return "未选择 Skill"
  }

  return `${visibleNames.join("、")}${remainingCount > 0 ? ` 等 ${selectedSkillNames.value.length} 个` : ""}`
})

const selectedSkills = computed(() => {
  return selectedSkillIds.value
    .map((skillId) => skillById.value.get(skillId))
    .filter(Boolean)
})

const hasSelectedSkills = computed(() => {
  return Boolean(selectedSkills.value.length)
})

const batchInstallSkillIds = computed(() => {
  return selectedSkills.value
    .filter((skill) => !skill.disabled && hasInstallableTarget(skill))
    .map((skill) => skill.id)
})

const batchUninstallSkillIds = computed(() => {
  return selectedSkills.value
    .filter((skill) => hasInstalledTarget(skill))
    .map((skill) => skill.id)
})

const batchDisableSkillIds = computed(() => {
  return selectedSkills.value
    .filter((skill) => !skill.disabled)
    .map((skill) => skill.id)
})

const batchEnableSkillIds = computed(() => {
  return selectedSkills.value
    .filter((skill) => skill.disabled)
    .map((skill) => skill.id)
})

const canBatchInstall = computed(() => {
  return Boolean(batchInstallSkillIds.value.length)
})

const canBatchUninstall = computed(() => {
  return Boolean(batchUninstallSkillIds.value.length)
})

const canBatchDisable = computed(() => {
  return Boolean(batchDisableSkillIds.value.length)
})

const canBatchEnable = computed(() => {
  return Boolean(batchEnableSkillIds.value.length)
})

const canBatchRemoveFromGroup = computed(() => {
  return selectedSkills.value.some((skill) => skillGroupBySkillId.value.has(skill.id))
})

const skillGroupBySkillId = computed(() => {
  const groupMap = new Map()

  for (const group of props.skillGroups) {
    for (const skillId of group.skillIds || []) {
      groupMap.set(skillId, group)
    }
  }

  return groupMap
})

const filteredSkills = computed(() => {
  const keyword = searchQuery.value.toLowerCase()

  return props.skills.filter((skill) => {
    const matchStatus =
      statusFilter.value === "all" || skill.status === statusFilter.value
    const group = skillGroupBySkillId.value.get(skill.id)
    const matchGroup =
      groupFilter.value === "all" ||
      (groupFilter.value === "ungrouped" && !group) ||
      group?.id === groupFilter.value
    const searchSource = [
      skill.name,
      skill.description,
      skill.repoName,
      ...(skill.tags || [])
    ]
      .join(" ")
      .toLowerCase()
    const matchKeyword = !keyword || searchSource.includes(keyword)

    return matchStatus && matchGroup && matchKeyword
  })
})

const skillGroupsView = computed(() => {
  return props.skillGroups.map((group) => {
    const skills = (group.skillIds || [])
      .map((skillId) => skillById.value.get(skillId))
      .filter(Boolean)

    return {
      ...group,
      skills,
      missingCount: Math.max(0, (group.skillIds || []).length - skills.length),
      skillNames: skills.map((skill) => skill.name)
    }
  })
})

// 底部始终同步当前选中的目标分组。
const moveTargetGroup = computed(() => {
  return skillGroupsView.value.find((group) => group.id === moveGroupId.value)
})

const allFilteredSelected = computed({
  get() {
    return Boolean(
      filteredSkills.value.length &&
      filteredSkills.value.every((skill) =>
        selectedSkillIds.value.includes(skill.id)
      )
    )
  },
  set(checked) {
    selectedSkillIds.value = checked
      ? filteredSkills.value.map((skill) => skill.id)
      : []
  }
})

const allTrashSelected = computed({
  get() {
    return Boolean(
      props.skillTrashItems.length &&
      props.skillTrashItems.every((item) =>
        selectedTrashIds.value.includes(item.id)
      )
    )
  },
  set(checked) {
    selectedTrashIds.value = checked
      ? props.skillTrashItems.map((item) => item.id)
      : []
  }
})

const allInstallTargetsSelected = computed({
  get() {
    return Boolean(
      installedCliTargets.value.length &&
      installedCliTargets.value.every((cli) =>
        selectedInstallTargetIds.value.includes(cli.id)
      )
    )
  },
  set(checked) {
    selectedInstallTargetIds.value = checked
      ? installedCliTargets.value.map((cli) => cli.id)
      : []
  }
})

watch(filteredSkills, (skills) => {
  const visibleIds = new Set(skills.map((skill) => skill.id))

  selectedSkillIds.value = selectedSkillIds.value.filter((skillId) =>
    visibleIds.has(skillId)
  )
})

watch(
  () => props.skillTrashItems,
  (items) => {
    const trashIds = new Set(items.map((item) => item.id))

    selectedTrashIds.value = selectedTrashIds.value.filter((id) =>
      trashIds.has(id)
    )
  }
)

watch(
  () => props.skills,
  (skills) => {
    const skillIds = new Set(skills.map((skill) => skill.id))

    selectedSkillIds.value = selectedSkillIds.value.filter((id) =>
      skillIds.has(id)
    )
  }
)

watch(
  () => props.skillGroups,
  (groups) => {
    if (
      !["all", "ungrouped"].includes(groupFilter.value) &&
      !groups.some((group) => group.id === groupFilter.value)
    ) {
      groupFilter.value = "all"
    }

    if (
      editingGroupId.value &&
      !groups.some((group) => group.id === editingGroupId.value)
    ) {
      cancelRenameGroup()
    }

    if (
      deletingGroupId.value &&
      !groups.some((group) => group.id === deletingGroupId.value)
    ) {
      cancelRemoveGroup()
    }
  }
)

function toggleSkillSelection(skillId) {
  if (selectedSkillIds.value.includes(skillId)) {
    selectedSkillIds.value = selectedSkillIds.value.filter(
      (id) => id !== skillId
    )
    return
  }

  selectedSkillIds.value = [...selectedSkillIds.value, skillId]
}

function resolveSkillNames(skillIds) {
  return skillIds
    .map((skillId) => skillById.value.get(skillId)?.name)
    .filter(Boolean)
}

function targetInstallState(skill, cli) {
  return skill.installStates?.[cli.id]?.state || "not-installed"
}

function hasInstallableTarget(skill) {
  return installedCliTargets.value.some((cli) =>
    ["not-installed", "broken-link"].includes(targetInstallState(skill, cli))
  )
}

function hasInstalledTarget(skill) {
  return installedCliTargets.value.some(
    (cli) => targetInstallState(skill, cli) === "installed"
  )
}

function emitBatchAction(
  action,
  skillIds = selectedSkillIds.value,
  resetSelection = true
) {
  const skillNames = resolveSkillNames(skillIds)

  if (!skillNames.length) {
    return
  }

  emit("batch-skill-action", {
    action,
    skillNames
  })

  if (resetSelection) {
    selectedSkillIds.value = []
  }
}

function openInstallDialog(
  skillIds = selectedSkillIds.value,
  resetSelection = true
) {
  const usableSkillIds = skillIds.filter((skillId) =>
    skillById.value.has(skillId)
  )

  if (!usableSkillIds.length) {
    return
  }

  installDialog.value = {
    skillIds: usableSkillIds,
    resetSelection
  }
  selectedInstallTargetIds.value = installedCliTargets.value.map(
    (cli) => cli.id
  )
}

function submitInstallTargets() {
  const skillNames = resolveSkillNames(installDialog.value?.skillIds || [])

  if (!skillNames.length || !selectedInstallTargetIds.value.length) {
    return
  }

  emit("batch-skill-action", {
    action: "install-all",
    skillNames,
    targetIds: [...selectedInstallTargetIds.value]
  })

  if (installDialog.value?.resetSelection) {
    selectedSkillIds.value = []
  }

  closeInstallDialog()
}

function closeInstallDialog() {
  installDialog.value = null
  selectedInstallTargetIds.value = []
}

function createManagedGroup() {
  if (!groupName.value.trim()) {
    return
  }

  emit("save-skill-group", {
    name: groupName.value.trim(),
    skillIds: []
  })
  groupName.value = ""
}

function openMoveGroupDialog() {
  if (!selectedSkillIds.value.length || !skillGroupsView.value.length) {
    return
  }

  moveGroupId.value = skillGroupsView.value[0].id
  showMoveGroupDialog.value = true
}

// 移动弹窗关闭后清理目标，避免下次打开短暂显示旧选择。
function closeMoveGroupDialog() {
  showMoveGroupDialog.value = false
  moveGroupId.value = ""
}

// 目标行展示本次移动后实际新增的 Skill 数量。
function moveGroupAddedCount(group) {
  const groupSkillIds = new Set(group.skillIds || [])

  return selectedSkillIds.value.filter((skillId) => !groupSkillIds.has(skillId))
    .length
}

function moveSelectedToGroup() {
  const group = props.skillGroups.find((item) => item.id === moveGroupId.value)

  if (!group || !selectedSkillIds.value.length) {
    return
  }

  emit("save-skill-group", {
    groupId: group.id,
    name: group.name,
    skillIds: [
      ...new Set([...(group.skillIds || []), ...selectedSkillIds.value])
    ]
  })
  closeMoveGroupDialog()
  selectedSkillIds.value = []
}

function openGroupManageDialog() {
  groupName.value = ""
  editingGroupId.value = ""
  deletingGroupId.value = ""
  groupRenameDraft.value = ""
  showGroupManageDialog.value = true
}

function closeGroupManageDialog() {
  showGroupManageDialog.value = false
  groupName.value = ""
  editingGroupId.value = ""
  deletingGroupId.value = ""
  groupRenameDraft.value = ""
}

// 每次仅允许一行处于编辑或删除确认状态。
function startRenameGroup(group) {
  deletingGroupId.value = ""
  editingGroupId.value = group.id
  groupRenameDraft.value = group.name

  nextTick(() => {
    groupRenameInput.value?.focus()
    groupRenameInput.value?.select()
  })
}

function setGroupRenameInput(element) {
  groupRenameInput.value = element
}

function cancelRenameGroup() {
  editingGroupId.value = ""
  groupRenameDraft.value = ""
}

function canRenameGroup(group) {
  const nextName = groupRenameDraft.value.trim()

  return Boolean(nextName && nextName !== group.name)
}

function renameGroup(group) {
  const nextName = groupRenameDraft.value.trim()

  if (!nextName || nextName === group.name) {
    return
  }

  emit("save-skill-group", {
    groupId: group.id,
    name: nextName,
    skillIds: [...(group.skillIds || [])]
  })
  cancelRenameGroup()
}

function requestRemoveGroup(group) {
  editingGroupId.value = ""
  groupRenameDraft.value = ""
  deletingGroupId.value = group.id
}

function cancelRemoveGroup() {
  deletingGroupId.value = ""
}

function confirmRemoveGroup(group) {
  emit("remove-skill-group", { groupId: group.id })
  cancelRemoveGroup()
}

// 复制 ID 后给出结果反馈，避免用户重复点击。
async function copyGroupId(group) {
  try {
    await navigator.clipboard.writeText(group.id)
    createMessage.success("分组 ID 已复制。")
  } catch {
    createMessage.error("复制分组 ID 失败。")
  }
}

function removeSelectedFromGroup() {
  if (!selectedSkillIds.value.length) {
    return
  }

  emit("remove-skill-group-items", {
    groupId:
      groupFilter.value === "all" || groupFilter.value === "ungrouped"
        ? ""
        : groupFilter.value,
    skillIds: [...selectedSkillIds.value]
  })
  selectedSkillIds.value = []
}

function deleteSelectedSkills() {
  emit("delete-skills", {
    skillNames: [...selectedSkillNames.value]
  })
  selectedSkillIds.value = []
}

function skillGroupLabel(skillId) {
  return skillGroupBySkillId.value.get(skillId)?.name || ""
}

function openTrashView() {
  viewMode.value = "trash"
  selectedTrashIds.value = []
  emit("show-skill-trash")
}

function restoreSelectedTrash() {
  emit("restore-skill-trash", {
    ids: [...selectedTrashIds.value]
  })
  selectedTrashIds.value = []
}

function purgeSelectedTrash() {
  emit("purge-skill-trash", {
    ids: [...selectedTrashIds.value]
  })
  selectedTrashIds.value = []
}

function refreshRepository(repository) {
  emit("refresh-skill-repository", {
    repositoryId: repository.id
  })
}

function removeRepository(repository) {
  emit("remove-skill-repository", {
    repositoryId: repository.id
  })
}

function installRepositorySkill(skill) {
  emit("install-repository-skill", {
    repositoryId: skill.repositoryId,
    skillId: skill.id
  })
}
</script>

<style scoped lang="less">
.skills-view {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  overflow: hidden;

  .skills-view-local-page {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 12px;
    overflow: hidden;
  }

  .skills-view-trash-page {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 12px;
    overflow: hidden;
  }

  .skills-view-head {
    display: flex;
    flex: none;
    align-items: flex-start;
    gap: 12px;
  }

  .skills-view-title {
    display: flex;
    width: 96px;
    flex: none;
    flex-direction: column;
    gap: 3px;
  }

  .skills-view-title-wide {
    width: auto;
    flex: 1;
  }

  .skills-view-mark {
    margin: 0;
    color: var(--color-text-soft);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .skills-view-title-text {
    margin: 0;
    color: var(--color-text);
    font-size: 1.26rem;
    line-height: 1.2;
    white-space: nowrap;
  }

  .skills-view-actions {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    row-gap: 8px;
    flex-wrap: wrap;
  }

  .skills-view-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 34px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
    white-space: nowrap;
  }

  .skills-view-button:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
  }

  .skills-view-button.primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  .skills-view-button.primary:hover {
    border-color: var(--color-primary);
    background: var(--color-primary);
  }

  .skills-view-button.danger {
    border-color: var(--color-danger);
    color: var(--color-danger);
  }

  .skills-view-button.danger:hover {
    border-color: var(--color-danger);
    background: var(--color-danger-soft);
  }

  .skills-view-button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .skills-view-back {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 34px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  .skills-view-filter-card {
    display: flex;
    flex: none;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
  }

  .skills-view-field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
  }

  .skills-view-field.search {
    flex: 1;
  }

  .skills-view-field.status {
    width: 220px;
    flex: none;
  }

  .skills-view-field.group {
    width: 220px;
    flex: none;
  }

  .skills-view-field-label {
    color: var(--color-text-muted);
    font-size: 0.74rem;
    font-weight: 700;
  }

  .skills-view-field-control {
    height: 36px;
    min-width: 0;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #fbfcfd;
    color: var(--color-text);
    font: inherit;
    font-size: 0.84rem;
    outline: none;
  }

  .skills-view-field-control:focus {
    border-color: #8eb6d9;
    box-shadow: 0 0 0 3px rgba(47, 95, 145, 0.1);
  }

  .skills-view-field-row {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
  }

  .skills-view-field-row .skills-view-field-control {
    flex: 1;
  }

  .skills-view-field-button {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    height: 36px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 700;
  }

  .skills-view-batch-card {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
  }

  .skills-view-check-all {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    font-weight: 700;
  }

  .skills-view-check-input {
    width: 16px;
    height: 16px;
    accent-color: var(--color-primary);
  }

  .skills-view-batch-actions {
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    flex-wrap: wrap;
  }

  .skills-view-result-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    font-weight: 700;
  }

  .skills-view-list {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: 0 10px 28px rgba(34, 56, 83, 0.05);
  }

  .skills-view-trash-item {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 10px 14px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel);
  }

  .skills-view-trash-check {
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .skills-view-trash-main {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 4px;
  }

  .skills-view-trash-name {
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.9rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skills-view-trash-path {
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.75rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skills-view-trash-meta {
    display: flex;
    min-width: 220px;
    flex-direction: column;
    gap: 4px;
    color: var(--color-text-soft);
    font-size: 0.74rem;
    text-align: right;
  }

  .skills-view-empty {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border: 1px dashed var(--color-line-strong);
    border-radius: 8px;
    background: #ffffff;
    color: var(--color-text-muted);
    text-align: center;
  }

  .skills-view-empty-title {
    color: var(--color-text);
    font-size: 0.98rem;
  }

  .skills-view-empty-desc {
    font-size: 0.82rem;
    line-height: 1.45;
  }

  .skills-view-install-form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .skills-view-dialog-field {
    display: flex;
    flex-direction: column;
    gap: 7px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    font-weight: 700;
  }

  .skills-view-dialog-control {
    height: 36px;
    min-width: 0;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #fbfcfd;
    color: var(--color-text);
    font: inherit;
    outline: none;
  }

  .skills-view-dialog-control:focus {
    border-color: #8eb6d9;
    box-shadow: 0 0 0 3px rgba(47, 95, 145, 0.1);
  }

  .skills-view-install-list {
    display: flex;
    max-height: 240px;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
  }

  .skills-view-install-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel-soft);
    color: var(--color-text);
    font-size: 0.84rem;
    font-weight: 700;
  }

  .skills-view-group-modal,
  .skills-view-move-modal {
    :deep(.base-modal__header) {
      align-items: center;
      padding: 18px 20px 14px;
      border-bottom: 1px solid var(--color-line);
    }

    :deep(.base-modal__header h2) {
      font-size: 1.12rem;
    }

    :deep(.base-modal__header p) {
      margin-top: 3px;
      font-size: 0.78rem;
    }

    :deep(.base-modal__close) {
      width: 32px;
      height: 32px;
      border: 0;
      background: transparent;
      font-size: 1.2rem;
    }

    :deep(.base-modal__content) {
      padding: 0;
    }
  }

  .skills-view-group-modal {
    :deep(.base-modal__panel) {
      width: 760px;
    }
  }

  .skills-view-move-modal {
    :deep(.base-modal__panel) {
      width: 620px;
    }
  }

  .skills-view-move-form {
    display: flex;
    min-height: 0;
    flex-direction: column;
  }

  .skills-view-move-selection {
    display: flex;
    flex: none;
    align-items: center;
    gap: 10px;
    padding: 13px 18px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  .skills-view-move-selection-icon {
    display: grid;
    width: 34px;
    height: 34px;
    flex: none;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-primary);
  }

  .skills-view-move-selection-main {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 2px;
  }

  .skills-view-move-selection-title {
    color: var(--color-text);
    font-size: 0.82rem;
  }

  .skills-view-move-selection-names {
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.74rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skills-view-move-targets {
    min-height: 0;
    padding: 15px 18px 17px;
  }

  .skills-view-move-target-title {
    margin: 0 0 8px;
    color: var(--color-text-muted);
    font-size: 0.76rem;
    letter-spacing: 0;
  }

  .skills-view-move-target-list {
    max-height: min(340px, calc(100vh - 306px));
    overflow: auto;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
  }

  .skills-view-move-target {
    position: relative;
    display: grid;
    min-height: 58px;
    grid-template-columns: 18px minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel);
    cursor: pointer;
    transition:
      background 0.15s ease,
      box-shadow 0.15s ease;
  }

  .skills-view-move-target:last-child {
    border-bottom: 0;
  }

  .skills-view-move-target:hover {
    background: #fbfcfd;
  }

  .skills-view-move-target.active {
    background: var(--color-primary-soft);
    box-shadow: inset 3px 0 0 var(--color-primary);
  }

  .skills-view-move-target > input {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    opacity: 0;
    pointer-events: none;
  }

  .skills-view-move-radio {
    display: grid;
    width: 18px;
    height: 18px;
    place-items: center;
    border: 1px solid var(--color-line-strong);
    border-radius: 50%;
    background: var(--color-panel);
    color: #ffffff;
  }

  .skills-view-move-target.active .skills-view-move-radio {
    border-color: var(--color-primary);
    background: var(--color-primary);
  }

  .skills-view-move-target:focus-within {
    outline: 2px solid rgba(47, 70, 104, 0.24);
    outline-offset: -2px;
  }

  .skills-view-move-target-main {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }

  .skills-view-move-target-name {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 7px;
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.82rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skills-view-move-target-name svg {
    flex: none;
    color: var(--color-primary);
  }

  .skills-view-move-target-meta {
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.72rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skills-view-move-target-change {
    min-width: 30px;
    padding: 2px 6px;
    border-radius: 6px;
    background: var(--color-success-soft);
    color: var(--color-success);
    font-size: 0.7rem;
    font-weight: 700;
    text-align: center;
  }

  .skills-view-move-form .skills-view-dialog-actions {
    flex: none;
    padding: 12px 18px;
    border-top: 1px solid var(--color-line);
    background: var(--color-panel);
  }

  .skills-view-move-current-target {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.75rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skills-view-move-form .skills-view-dialog-button.primary {
    gap: 6px;
    min-width: 76px;
  }

  .skills-view-group-manage {
    display: flex;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
  }

  .skills-view-group-create {
    display: grid;
    flex: none;
    grid-template-columns: 104px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    padding: 16px 20px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel-soft);
  }

  .skills-view-group-section-title {
    color: var(--color-text);
    font-size: 0.82rem;
  }

  .skills-view-group-create-field {
    display: block;
    min-width: 0;
  }

  .skills-view-group-create-field .skills-view-dialog-control {
    width: 100%;
    background: var(--color-panel);
  }

  .skills-view-group-create-button {
    gap: 6px;
    min-width: 76px;
  }

  .skills-view-group-list-head {
    display: grid;
    flex: none;
    grid-template-columns: minmax(0, 1fr) 76px;
    gap: 16px;
    padding: 10px 20px 8px;
    border-bottom: 1px solid var(--color-line);
    color: var(--color-text-soft);
    font-size: 0.7rem;
    font-weight: 700;
  }

  .skills-view-group-list-head span:last-child {
    text-align: right;
  }

  .skills-view-group-list {
    min-height: 84px;
    max-height: min(440px, calc(100vh - 272px));
    overflow: auto;
  }

  .skills-view-group-manage-item {
    display: grid;
    min-height: 64px;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 16px;
    padding: 10px 20px;
    border-bottom: 1px solid var(--color-line);
    background: var(--color-panel);
    transition: background 0.15s ease;
  }

  .skills-view-group-manage-item:hover,
  .skills-view-group-manage-item.editing,
  .skills-view-group-manage-item.deleting {
    background: #fbfcfd;
  }

  .skills-view-group-manage-item:last-child {
    border-bottom: 0;
  }

  .skills-view-group-manage-main {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 4px;
  }

  .skills-view-group-name-row {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
    color: var(--color-text);
  }

  .skills-view-group-name-row svg {
    flex: none;
    color: var(--color-primary);
  }

  .skills-view-group-name-row strong {
    overflow: hidden;
    font-size: 0.84rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skills-view-group-manage-meta {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
    padding-left: 24px;
    color: var(--color-text-muted);
    font-size: 0.72rem;
  }

  .skills-view-group-manage-meta > * + * {
    position: relative;
    padding-left: 9px;
  }

  .skills-view-group-manage-meta > * + *::before {
    position: absolute;
    top: 50%;
    left: 0;
    width: 2px;
    height: 2px;
    border-radius: 50%;
    background: var(--color-text-soft);
    content: "";
    transform: translateY(-50%);
  }

  .skills-view-group-missing {
    color: var(--color-warning);
  }

  .skills-view-group-id-button {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-text-soft);
    cursor: pointer;
    font-size: inherit;
  }

  .skills-view-group-id-button:hover {
    color: var(--color-primary);
  }

  .skills-view-group-manage-actions {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
  }

  .skills-view-group-icon-button {
    display: grid;
    width: 32px;
    height: 32px;
    flex: none;
    place-items: center;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .skills-view-group-icon-button:hover {
    border-color: var(--color-line);
    background: var(--color-panel);
    color: var(--color-primary);
  }

  .skills-view-group-icon-button.primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  .skills-view-group-icon-button.danger:hover {
    border-color: #ffc7c2;
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .skills-view-group-icon-button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .skills-view-group-edit {
    display: grid;
    grid-column: 1 / -1;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
  }

  .skills-view-group-edit .skills-view-dialog-control {
    width: 100%;
    background: var(--color-panel);
  }

  .skills-view-group-delete-confirm {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
  }

  .skills-view-group-delete-confirm > span {
    margin-right: 2px;
    color: var(--color-danger);
    font-size: 0.75rem;
    font-weight: 700;
    white-space: nowrap;
  }

  .skills-view-group-confirm-button {
    height: 30px;
    padding: 0 9px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 0.75rem;
    font-weight: 700;
  }

  .skills-view-group-confirm-button.danger {
    border-color: var(--color-danger);
    background: var(--color-danger);
    color: #ffffff;
  }

  .skills-view-dialog-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 42px;
    color: var(--color-text-muted);
    font-size: 0.8rem;
    text-align: center;
  }

  .skills-view-group-empty {
    min-height: 120px;
    flex-direction: column;
    gap: 8px;
  }

  .skills-view-dialog-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .skills-view-dialog-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 34px;
    padding: 0 14px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-primary);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 700;
  }

  .skills-view-dialog-button.primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  .skills-view-dialog-button.danger {
    border-color: var(--color-danger);
    color: var(--color-danger);
  }

  .skills-view-dialog-button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .skills-view-sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    clip-path: inset(50%);
  }

  @media (max-width: 720px) {
    .skills-view-group-modal,
    .skills-view-move-modal {
      :deep(.base-modal__panel) {
        width: 100%;
      }
    }

    .skills-view-group-create {
      grid-template-columns: minmax(0, 1fr) auto;
    }

    .skills-view-group-section-title {
      grid-column: 1 / -1;
    }

    .skills-view-group-manage-item {
      padding-inline: 14px;
    }

    .skills-view-group-list-head {
      padding-inline: 14px;
    }

    .skills-view-move-selection,
    .skills-view-move-targets,
    .skills-view-move-form .skills-view-dialog-actions {
      padding-inline: 14px;
    }
  }
}
</style>
