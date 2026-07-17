<template>
  <section class="codex-pet-manager">
    <header class="codex-pet-manager-head">
      <div class="codex-pet-manager-title">
        <p class="codex-pet-manager-mark">Codex Pets</p>
        <h2 class="codex-pet-manager-title-text">宠物管理</h2>
        <span class="codex-pet-manager-title-desc">
          {{ pets.length }} 只宠物，已启用 {{ enabledCount }} 只
        </span>
      </div>
      <div class="codex-pet-manager-head-actions">
        <button
          class="codex-pet-manager-icon-button"
          type="button"
          title="打开 AI Manager 宠物目录"
          :disabled="!managedPetsPath"
          @click="openPath(managedPetsPath)"
        >
          <FolderOpen :size="16" />
        </button>
        <button
          class="codex-pet-manager-icon-button"
          type="button"
          title="刷新宠物列表"
          :disabled="loading || actionPending"
          @click="loadPets"
        >
          <RefreshCw :class="{ spinning: loading }" :size="16" />
        </button>
      </div>
    </header>

    <section class="codex-pet-manager-paths">
      <button
        class="codex-pet-manager-path"
        type="button"
        :disabled="!managedPetsPath"
        @click="openPath(managedPetsPath)"
      >
        <span class="codex-pet-manager-path-label">AI Manager</span>
        <span class="codex-pet-manager-path-value">{{ managedPetsPath }}</span>
      </button>
      <button
        class="codex-pet-manager-path"
        type="button"
        :disabled="!codexPetsPath"
        @click="openPath(codexPetsPath)"
      >
        <span class="codex-pet-manager-path-label">Codex</span>
        <span class="codex-pet-manager-path-value">{{ codexPetsPath }}</span>
      </button>
    </section>

    <section v-if="loading" class="codex-pet-manager-state">
      正在同步 Codex 宠物...
    </section>
    <section v-else-if="!pets.length" class="codex-pet-manager-empty">
      <PawPrint :size="26" />
      <strong class="codex-pet-manager-empty-title">暂无可管理宠物</strong>
      <span class="codex-pet-manager-empty-desc">
        在 Codex pets 目录中放入包含 pet.json 和 spritesheet.webp 的宠物目录后，刷新即可接管。
      </span>
    </section>
    <section v-else class="codex-pet-manager-list">
      <article
        v-for="pet in pets"
        :key="pet.id"
        :class="['codex-pet-manager-item', { disabled: !pet.enabled }]"
        role="button"
        tabindex="0"
        @click="openPreview(pet)"
        @keydown.enter.self="openPreview(pet)"
        @keydown.space.prevent.self="openPreview(pet)"
      >
        <div
          class="codex-pet-manager-preview"
          :style="{ backgroundImage: `url('${pet.spritesheetData}')` }"
          role="img"
          :aria-label="`${pet.displayName || pet.id} 动画预览`"
        ></div>
        <div class="codex-pet-manager-item-main">
          <div class="codex-pet-manager-item-title-row">
            <strong class="codex-pet-manager-item-name" :title="pet.displayName || pet.id">
              {{ pet.displayName || pet.id }}
            </strong>
            <span :class="['codex-pet-manager-status', { disabled: !pet.enabled }]">
              {{ pet.enabled ? '已启用' : '已禁用' }}
            </span>
          </div>
          <span class="codex-pet-manager-item-id">{{ pet.id }}</span>
          <p v-if="pet.description" class="codex-pet-manager-item-desc">
            {{ pet.description }}
          </p>
          <span class="codex-pet-manager-shape">{{ pet.shape }}</span>
        </div>
        <div class="codex-pet-manager-item-actions">
          <button
            class="codex-pet-manager-icon-button"
            type="button"
            title="修改名称"
            :disabled="actionPending"
            @click.stop="openRenameDialog(pet)"
          >
            <Pencil :size="15" />
          </button>
          <button
            :class="['codex-pet-manager-icon-button', { active: !pet.enabled }]"
            type="button"
            :title="pet.enabled ? '禁用宠物' : '启用宠物'"
            :disabled="actionPending"
            @click.stop="togglePet(pet)"
          >
            <PowerOff v-if="pet.enabled" :size="15" />
            <Power v-else :size="15" />
          </button>
          <button
            class="codex-pet-manager-icon-button danger"
            type="button"
            title="删除宠物"
            :disabled="actionPending"
            @click.stop="deletePet(pet)"
          >
            <Trash2 :size="15" />
          </button>
        </div>
      </article>
    </section>

    <BaseModal
      v-if="selectedPet"
      :title="`${selectedPet.displayName || selectedPet.id} 动画图谱`"
      description="Codex 宠物精灵图的全部状态行。"
      @close="closePreview"
    >
      <section class="codex-pet-manager-animation-grid">
        <article
          v-for="row in animationRows"
          :key="row.id"
          class="codex-pet-manager-animation-row"
        >
          <div
            :class="[
              'codex-pet-manager-row-preview',
              `frames-${row.frameCount}`
            ]"
            :style="{
              animationDuration: `${row.duration}ms`,
              backgroundImage: `url('${selectedPet.spritesheetData}')`,
              backgroundPositionY: `${(row.index / 8) * 100}%`
            }"
            role="img"
            :aria-label="`${row.label}动画预览`"
          ></div>
          <div class="codex-pet-manager-animation-row-info">
            <strong class="codex-pet-manager-animation-row-name">{{ row.label }}</strong>
            <span class="codex-pet-manager-animation-row-meta">
              {{ row.frameCount }} 帧 · {{ row.duration }} ms
            </span>
          </div>
        </article>
      </section>
    </BaseModal>

    <BaseModal
      v-if="renamePet"
      title="修改宠物名称"
      description="仅更新 pet.json 中的显示名称，不改变 Codex 使用的目录标识。"
      @close="closeRenameDialog"
    >
      <form class="codex-pet-manager-rename-form" @submit.prevent="renamePetName">
        <label class="codex-pet-manager-name-field">
          <span class="codex-pet-manager-name-label">显示名称</span>
          <input
            v-model.trim="renameName"
            class="codex-pet-manager-name-input"
            type="text"
            maxlength="80"
            autofocus
          />
        </label>
        <div class="codex-pet-manager-modal-actions">
          <button
            class="codex-pet-manager-button"
            type="button"
            :disabled="actionPending"
            @click="closeRenameDialog"
          >
            取消
          </button>
          <button
            class="codex-pet-manager-button primary"
            type="submit"
            :disabled="actionPending || !renameName"
          >
            保存
          </button>
        </div>
      </form>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, onMounted, ref } from 'vue'
import {
  FolderOpen,
  PawPrint,
  Pencil,
  Power,
  PowerOff,
  RefreshCw,
  Trash2
} from 'lucide-vue-next'
import BaseModal from '@/components/BaseModal.vue'
import { systemApi, toolboxApi } from '@/api'
import { createMessage } from '@/utils/message'

const pets = ref([])
const loading = ref(false)
const actionPending = ref(false)
const managedPetsPath = ref('')
const codexPetsPath = ref('')
const renamePet = ref(null)
const renameName = ref('')
const selectedPet = ref(null)

// Codex 的 9 行精灵图状态与每行可用帧数。
const animationRows = [
  { id: 'idle', label: '待机', index: 0, frameCount: 6, duration: 1100 },
  { id: 'running-right', label: '向右移动', index: 1, frameCount: 8, duration: 1060 },
  { id: 'running-left', label: '向左移动', index: 2, frameCount: 8, duration: 1060 },
  { id: 'waving', label: '挥手', index: 3, frameCount: 4, duration: 700 },
  { id: 'jumping', label: '跳跃', index: 4, frameCount: 5, duration: 840 },
  { id: 'failed', label: '失败', index: 5, frameCount: 8, duration: 1220 },
  { id: 'waiting', label: '等待', index: 6, frameCount: 6, duration: 1010 },
  { id: 'running', label: '工作中', index: 7, frameCount: 6, duration: 820 },
  { id: 'review', label: '检查', index: 8, frameCount: 6, duration: 1010 }
]

const enabledCount = computed(() => pets.value.filter(pet => pet.enabled).length)

// 重新读取时同步迁移未受管的 Codex 宠物，并刷新链接状态。
async function loadPets() {
  loading.value = true

  try {
    const result = await toolboxApi.listCodexPets()
    pets.value = result.pets || []
    managedPetsPath.value = result.managedPetsPath || ''
    codexPetsPath.value = result.codexPetsPath || ''
  } catch (error) {
    createMessage.error(error.message || String(error))
  } finally {
    loading.value = false
  }
}

async function openPath(targetPath) {
  if (!targetPath) {
    return
  }

  try {
    await systemApi.openPath({ targetPath })
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

function openRenameDialog(pet) {
  renamePet.value = pet
  renameName.value = pet.displayName || pet.id
}

function closeRenameDialog() {
  renamePet.value = null
  renameName.value = ''
}

function openPreview(pet) {
  selectedPet.value = pet
}

function closePreview() {
  selectedPet.value = null
}

// 所有写操作完成后统一回读，避免界面与文件系统状态不一致。
async function runPetAction(action, successMessage) {
  actionPending.value = true

  try {
    await action()
    await loadPets()
    createMessage.success(successMessage)
    return true
  } catch (error) {
    createMessage.error(error.message || String(error))
    return false
  } finally {
    actionPending.value = false
  }
}

async function renamePetName() {
  if (!renamePet.value || !renameName.value) {
    return
  }

  const renamed = await runPetAction(
    () =>
      toolboxApi.renameCodexPet({
        id: renamePet.value.id,
        displayName: renameName.value
      }),
    '宠物名称已更新。'
  )

  if (renamed) {
    closeRenameDialog()
  }
}

function togglePet(pet) {
  return runPetAction(
    () => toolboxApi.toggleCodexPet({ id: pet.id, enabled: !pet.enabled }),
    pet.enabled ? '宠物已禁用。' : '宠物已启用。'
  )
}

async function deletePet(pet) {
  if (!window.confirm(`确定删除宠物「${pet.displayName || pet.id}」吗？此操作不可恢复。`)) {
    return
  }

  await runPetAction(
    () => toolboxApi.deleteCodexPet({ id: pet.id }),
    '宠物已删除。'
  )
}

onMounted(loadPets)
</script>

<style scoped lang="less">
.codex-pet-manager {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;

  .codex-pet-manager-head {
    display: flex;
    flex: none;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 2px 2px 0;
  }

  .codex-pet-manager-title {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  .codex-pet-manager-mark {
    margin: 0;
    color: var(--color-text-soft);
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  .codex-pet-manager-title-text {
    margin: 0;
    color: var(--color-text);
    font-size: 1.14rem;
    line-height: 1.2;
  }

  .codex-pet-manager-title-desc {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  .codex-pet-manager-head-actions,
  .codex-pet-manager-item-actions,
  .codex-pet-manager-modal-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 7px;
  }

  .codex-pet-manager-icon-button,
  .codex-pet-manager-button {
    display: inline-grid;
    height: 32px;
    place-items: center;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #ffffff;
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .codex-pet-manager-icon-button {
    width: 32px;
  }

  .codex-pet-manager-button {
    padding: 0 12px;
    color: var(--color-primary);
    font-size: 0.82rem;
    font-weight: 700;
  }

  .codex-pet-manager-icon-button:hover,
  .codex-pet-manager-icon-button.active,
  .codex-pet-manager-button:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
    color: var(--color-primary);
  }

  .codex-pet-manager-icon-button.danger:hover {
    border-color: #edb9b9;
    background: var(--color-danger-soft);
    color: var(--color-danger);
  }

  .codex-pet-manager-icon-button:disabled,
  .codex-pet-manager-button:disabled {
    cursor: not-allowed;
    opacity: 0.52;
  }

  .codex-pet-manager-button.primary {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: #ffffff;
  }

  .codex-pet-manager-paths {
    display: flex;
    flex: none;
    gap: 8px;
  }

  .codex-pet-manager-path {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    overflow: hidden;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #fbfcfd;
    color: var(--color-text-muted);
    cursor: pointer;
    text-align: left;
  }

  .codex-pet-manager-path:hover {
    border-color: #b9ccda;
    background: #f7f9fc;
  }

  .codex-pet-manager-path:disabled {
    cursor: default;
  }

  .codex-pet-manager-path-label {
    flex: none;
    color: var(--color-primary);
    font-size: 0.72rem;
    font-weight: 700;
  }

  .codex-pet-manager-path-value {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text-soft);
    font-size: 0.72rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .codex-pet-manager-list {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
    padding-right: 2px;
  }

  .codex-pet-manager-item {
    display: flex;
    min-height: 126px;
    align-items: center;
    gap: 13px;
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: #ffffff;
    cursor: pointer;
    transition:
      border-color 0.18s ease,
      background-color 0.18s ease;
  }

  .codex-pet-manager-item:hover,
  .codex-pet-manager-item:focus-visible {
    border-color: #b9ccda;
    background: #f8fbff;
    outline: none;
  }

  .codex-pet-manager-item.disabled {
    background: #fbfcfd;
  }

  .codex-pet-manager-preview {
    width: 88px;
    aspect-ratio: 192 / 208;
    flex: 0 0 88px;
    overflow: hidden;
    border: 1px solid #d6e2ec;
    border-radius: 7px;
    background-color: #ffffff;
    background-position: 0 0;
    background-repeat: no-repeat;
    background-size: 800% auto;
    animation: codex-pet-manager-idle 1.1s steps(1, end) infinite;
  }

  .codex-pet-manager-item.disabled .codex-pet-manager-preview {
    animation-play-state: paused;
    filter: grayscale(1);
    opacity: 0.48;
  }

  .codex-pet-manager-item-main {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 5px;
  }

  .codex-pet-manager-item-title-row {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
  }

  .codex-pet-manager-item-name {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.94rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .codex-pet-manager-status,
  .codex-pet-manager-shape {
    display: inline-flex;
    width: fit-content;
    flex: none;
    align-items: center;
    min-height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    font-size: 0.72rem;
    font-weight: 700;
  }

  .codex-pet-manager-status {
    background: var(--color-success-soft);
    color: var(--color-success);
  }

  .codex-pet-manager-status.disabled {
    background: #eef2f5;
    color: var(--color-text-soft);
  }

  .codex-pet-manager-item-id {
    overflow: hidden;
    color: var(--color-text-soft);
    font-size: 0.74rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .codex-pet-manager-item-desc {
    display: -webkit-box;
    margin: 0;
    overflow: hidden;
    color: var(--color-text-muted);
    font-size: 0.78rem;
    line-height: 1.45;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
  }

  .codex-pet-manager-shape {
    border: 1px solid #d8e4ee;
    background: #f8fafc;
    color: var(--color-text-soft);
  }

  .codex-pet-manager-animation-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
    overflow: auto;
    padding: 2px;
  }

  .codex-pet-manager-animation-row {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 10px;
    padding: 9px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #fbfcfd;
  }

  .codex-pet-manager-row-preview {
    width: 72px;
    aspect-ratio: 192 / 208;
    flex: 0 0 72px;
    border: 1px solid #d6e2ec;
    border-radius: 6px;
    background-color: #ffffff;
    background-position-x: 0;
    background-repeat: no-repeat;
    background-size: 800% 900%;
    animation-iteration-count: infinite;
    animation-timing-function: steps(1, end);
  }

  .codex-pet-manager-row-preview.frames-4 {
    animation-name: codex-pet-manager-frames-4;
  }

  .codex-pet-manager-row-preview.frames-5 {
    animation-name: codex-pet-manager-frames-5;
  }

  .codex-pet-manager-row-preview.frames-6 {
    animation-name: codex-pet-manager-frames-6;
  }

  .codex-pet-manager-row-preview.frames-8 {
    animation-name: codex-pet-manager-frames-8;
  }

  .codex-pet-manager-animation-row-info {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 4px;
  }

  .codex-pet-manager-animation-row-name {
    overflow: hidden;
    color: var(--color-text);
    font-size: 0.84rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .codex-pet-manager-animation-row-meta {
    color: var(--color-text-muted);
    font-size: 0.72rem;
    font-weight: 700;
  }

  .codex-pet-manager-state,
  .codex-pet-manager-empty {
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
    font-size: 0.82rem;
    text-align: center;
  }

  .codex-pet-manager-empty-title {
    color: var(--color-text);
    font-size: 0.94rem;
  }

  .codex-pet-manager-empty-desc {
    max-width: 440px;
    font-size: 0.8rem;
    line-height: 1.5;
  }

  .codex-pet-manager-rename-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .codex-pet-manager-name-field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .codex-pet-manager-name-label {
    color: var(--color-text-muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  .codex-pet-manager-name-input {
    height: 38px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: #fbfcfd;
    color: var(--color-text);
    font: inherit;
    font-size: 0.86rem;
    outline: none;
  }

  .codex-pet-manager-name-input:focus {
    border-color: #8eb6d9;
    box-shadow: 0 0 0 3px rgba(47, 95, 145, 0.1);
  }

  .codex-pet-manager-modal-actions {
    justify-content: flex-end;
  }

  .spinning {
    animation: codex-pet-manager-spin 0.9s linear infinite;
  }
}

@keyframes codex-pet-manager-idle {
  0%,
  100% {
    background-position-x: 0;
  }

  25.4545% {
    background-position-x: 14.2857%;
  }

  35.4545% {
    background-position-x: 28.5714%;
  }

  45.4545% {
    background-position-x: 42.8571%;
  }

  58.1818% {
    background-position-x: 57.1429%;
  }

  70.9091% {
    background-position-x: 71.4286%;
  }
}

@keyframes codex-pet-manager-frames-4 {
  0%,
  100% {
    background-position-x: 0;
  }

  25% {
    background-position-x: 14.2857%;
  }

  50% {
    background-position-x: 28.5714%;
  }

  75% {
    background-position-x: 42.8571%;
  }
}

@keyframes codex-pet-manager-frames-5 {
  0%,
  100% {
    background-position-x: 0;
  }

  20% {
    background-position-x: 14.2857%;
  }

  40% {
    background-position-x: 28.5714%;
  }

  60% {
    background-position-x: 42.8571%;
  }

  80% {
    background-position-x: 57.1429%;
  }
}

@keyframes codex-pet-manager-frames-6 {
  0%,
  100% {
    background-position-x: 0;
  }

  16.6667% {
    background-position-x: 14.2857%;
  }

  33.3333% {
    background-position-x: 28.5714%;
  }

  50% {
    background-position-x: 42.8571%;
  }

  66.6667% {
    background-position-x: 57.1429%;
  }

  83.3333% {
    background-position-x: 71.4286%;
  }
}

@keyframes codex-pet-manager-frames-8 {
  0%,
  100% {
    background-position-x: 0;
  }

  12.5% {
    background-position-x: 14.2857%;
  }

  25% {
    background-position-x: 28.5714%;
  }

  37.5% {
    background-position-x: 42.8571%;
  }

  50% {
    background-position-x: 57.1429%;
  }

  62.5% {
    background-position-x: 71.4286%;
  }

  75% {
    background-position-x: 85.7143%;
  }

  87.5% {
    background-position-x: 100%;
  }
}

@keyframes codex-pet-manager-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
