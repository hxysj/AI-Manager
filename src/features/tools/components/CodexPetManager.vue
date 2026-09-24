<template>
  <section class="codex-pet-manager">
    <!-- 顶部主标题栏 -->
    <header class="codex-pet-manager-head">
      <div class="codex-pet-manager-head-left">
        <div class="codex-pet-manager-badge">
          <PawPrint :size="22" :stroke-width="2.1" />
        </div>
        <div class="codex-pet-manager-title-group">
          <h2 class="codex-pet-manager-title-text">宠物管理</h2>
          <span class="codex-pet-manager-title-desc">
            {{ pets.length }} 只宠物，已启用 {{ enabledCount }} 只
          </span>
        </div>
      </div>
      <div class="codex-pet-manager-head-actions">
        <button
          class="codex-pet-manager-icon-button"
          type="button"
          title="打开已禁用宠物目录"
          :disabled="!disabledPetsPath"
          @click="openPath(disabledPetsPath)"
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

    <!-- 路径卡片区域（2列网格） -->
    <section class="codex-pet-manager-paths">
      <div
        class="codex-pet-path-card"
        role="button"
        tabindex="0"
        title="点击打开目录"
        @click="openPath(disabledPetsPath)"
        @keydown.enter.self="openPath(disabledPetsPath)"
      >
        <div class="codex-pet-path-icon-box">
          <Folder :size="18" :stroke-width="1.8" />
        </div>
        <div class="codex-pet-path-info">
          <span class="codex-pet-path-label">已禁用宠物文件夹</span>
          <span class="codex-pet-path-value" :title="disabledPetsPath">
            {{ disabledPetsPath || "未设置" }}
          </span>
        </div>
        <button
          class="codex-pet-path-copy-btn"
          type="button"
          title="复制路径"
          aria-label="复制已禁用宠物文件夹路径"
          @click.stop="copyPath(disabledPetsPath, '已禁用宠物文件夹路径')"
        >
          <Copy :size="13" />
        </button>
      </div>

      <div
        class="codex-pet-path-card"
        role="button"
        tabindex="0"
        title="点击打开目录"
        @click="openPath(codexPetsPath)"
        @keydown.enter.self="openPath(codexPetsPath)"
      >
        <div class="codex-pet-path-icon-box">
          <Folder :size="18" :stroke-width="1.8" />
        </div>
        <div class="codex-pet-path-info">
          <span class="codex-pet-path-label">Codex 宠物文件夹</span>
          <span class="codex-pet-path-value" :title="codexPetsPath">
            {{ codexPetsPath || "未设置" }}
          </span>
        </div>
        <button
          class="codex-pet-path-copy-btn"
          type="button"
          title="复制路径"
          aria-label="复制 Codex 宠物文件夹路径"
          @click.stop="copyPath(codexPetsPath, 'Codex 宠物文件夹路径')"
        >
          <Copy :size="13" />
        </button>
      </div>
    </section>

    <!-- 加载与空状态 -->
    <section v-if="loading" class="codex-pet-manager-state">
      <RefreshCw class="spinning" :size="24" />
      <span>正在读取 Codex 宠物...</span>
    </section>
    <section v-else-if="!pets.length" class="codex-pet-manager-empty">
      <div class="codex-empty-icon-box">
        <PawPrint :size="32" />
      </div>
      <span class="codex-pet-manager-empty-title">暂无可管理宠物</span>
      <span class="codex-pet-manager-empty-desc">
        在 Codex pets 目录中放入包含 pet.json 和 spritesheet.webp
        的宠物目录后，刷新即可显示。
      </span>
    </section>

    <!-- 宠物卡片双列网格 -->
    <section v-else class="codex-pet-manager-grid">
      <article
        v-for="pet in pets"
        :key="pet.id"
        class="codex-pet-card"
        :class="{ 'is-enabled': pet.enabled }"
      >
        <div class="codex-pet-card-body">
          <!-- 左侧精灵预览图 -->
          <div
            class="codex-pet-preview"
            :class="{ disabled: !pet.enabled }"
            :style="{ backgroundImage: `url('${pet.spritesheetData}')` }"
            role="button"
            tabindex="0"
            title="点击查看精灵动画图谱"
            :aria-label="`${pet.displayName || pet.id} 动画预览`"
            @click="openPreview(pet)"
            @keydown.enter.self="openPreview(pet)"
          ></div>

          <!-- 右侧信息 -->
          <div class="codex-pet-info">
            <div class="codex-pet-header-row">
              <span class="codex-pet-name" :title="pet.displayName || pet.id">
                {{ pet.displayName || pet.id }}
              </span>
              <span
                class="codex-pet-status-pill"
                :class="{ 'is-enabled': pet.enabled }"
              >
                <span class="status-dot"></span>
                <span>{{ pet.enabled ? "已启用" : "已禁用" }}</span>
              </span>
            </div>

            <span class="codex-pet-id">{{ pet.id }}</span>

            <p class="codex-pet-desc" :title="pet.description">
              {{ pet.description || "暂无描述信息。" }}
            </p>

            <div class="codex-pet-shape-tag">
              <ImageIcon :size="12" />
              <span>{{ pet.shape || "8×9 动画精灵" }}</span>
            </div>
          </div>
        </div>

        <!-- 底部三个操作按钮 -->
        <div class="codex-pet-card-actions">
          <button
            class="codex-pet-action-btn"
            type="button"
            title="修改名称"
            :disabled="actionPending"
            @click="openRenameDialog(pet)"
          >
            <Pencil :size="13" />
            <span>编辑</span>
          </button>
          <button
            class="codex-pet-action-btn codex-pet-action-btn--toggle"
            type="button"
            :title="pet.enabled ? '禁用宠物' : '启用宠物'"
            :disabled="actionPending"
            @click="togglePet(pet)"
          >
            <Ban v-if="pet.enabled" :size="13" />
            <Power v-else :size="13" />
            <span>{{ pet.enabled ? "禁用" : "启用" }}</span>
          </button>
          <button
            class="codex-pet-action-btn codex-pet-action-btn--delete"
            type="button"
            title="删除宠物"
            :disabled="actionPending"
            @click="deletePet(pet)"
          >
            <Trash2 :size="13" />
            <span>删除</span>
          </button>
        </div>
      </article>
    </section>

    <!-- 动画图谱全览弹窗 -->
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
            <span data-emphasis class="codex-pet-manager-animation-row-name">{{
              row.label
            }}</span>
            <span class="codex-pet-manager-animation-row-meta">
              {{ row.frameCount }} 帧 · {{ row.duration }} ms
            </span>
          </div>
        </article>
      </section>
    </BaseModal>

    <!-- 修改名称弹窗 -->
    <BaseModal
      v-if="renamePet"
      title="修改宠物名称"
      description="仅更新 pet.json 中的显示名称，不改变 Codex 使用的目录标识。"
      @close="closeRenameDialog"
    >
      <form
        class="codex-pet-manager-rename-form"
        @submit.prevent="renamePetName"
      >
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
import { computed, onMounted, ref } from "vue"
import {
  Ban,
  Copy,
  Folder,
  FolderOpen,
  Image as ImageIcon,
  PawPrint,
  Pencil,
  Power,
  RefreshCw,
  Trash2
} from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import { systemApi, toolboxApi } from "@/api"
import { createMessage } from "@/utils/message"

const pets = ref([])
const loading = ref(false)
const actionPending = ref(false)
const disabledPetsPath = ref("")
const codexPetsPath = ref("")
const renamePet = ref(null)
const renameName = ref("")
const selectedPet = ref(null)

// Codex 的 9 行精灵图状态与每行可用帧数。
const animationRows = [
  { id: "idle", label: "待机", index: 0, frameCount: 6, duration: 1100 },
  {
    id: "running-right",
    label: "向右移动",
    index: 1,
    frameCount: 8,
    duration: 1060
  },
  {
    id: "running-left",
    label: "向左移动",
    index: 2,
    frameCount: 8,
    duration: 1060
  },
  { id: "waving", label: "挥手", index: 3, frameCount: 4, duration: 700 },
  { id: "jumping", label: "跳跃", index: 4, frameCount: 5, duration: 840 },
  { id: "failed", label: "失败", index: 5, frameCount: 8, duration: 1220 },
  { id: "waiting", label: "等待", index: 6, frameCount: 6, duration: 1010 },
  { id: "running", label: "工作中", index: 7, frameCount: 6, duration: 820 },
  { id: "review", label: "检查", index: 8, frameCount: 6, duration: 1010 }
]

const enabledCount = computed(
  () => pets.value.filter((pet) => pet.enabled).length
)

// 直接读取 Codex 宠物目录与应用数据目录中的已禁用宠物。
async function loadPets() {
  loading.value = true

  try {
    const result = await toolboxApi.listCodexPets()
    pets.value = result.pets || []
    disabledPetsPath.value = result.disabledPetsPath || ""
    codexPetsPath.value = result.codexPetsPath || ""
  } catch (error) {
    createMessage.error(error.message || String(error))
  } finally {
    loading.value = false
  }
}

async function openPath(targetPath) {
  if (!targetPath) return

  try {
    await systemApi.openPath({ targetPath })
  } catch (error) {
    createMessage.error(error.message || String(error))
  }
}

async function copyPath(path, label = "路径") {
  if (!path) return
  try {
    await navigator.clipboard.writeText(path)
    createMessage.success(`已复制${label}`)
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
  renameName.value = ""
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
  if (!renamePet.value || !renameName.value) return

  const renamed = await runPetAction(
    () =>
      toolboxApi.renameCodexPet({
        id: renamePet.value.id,
        displayName: renameName.value
      }),
    "宠物名称已更新。"
  )

  if (renamed) {
    closeRenameDialog()
  }
}

function togglePet(pet) {
  return runPetAction(
    () => toolboxApi.toggleCodexPet({ id: pet.id, enabled: !pet.enabled }),
    pet.enabled ? "宠物已禁用。" : "宠物已启用。"
  )
}

async function deletePet(pet) {
  if (
    !window.confirm(
      `确定删除宠物「${pet.displayName || pet.id}」吗？此操作不可恢复。`
    )
  ) {
    return
  }

  await runPetAction(
    () => toolboxApi.deleteCodexPet({ id: pet.id }),
    "宠物已删除。"
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
  overflow: hidden;
  background: var(--color-panel);
  border: 1px solid var(--color-line);
  border-radius: 14px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.03);
  padding: 20px 22px;
  box-sizing: border-box;

  /* 顶部控制头 */
  .codex-pet-manager-head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 16px;

    .codex-pet-manager-head-left {
      display: flex;
      align-items: center;
      gap: 14px;

      .codex-pet-manager-badge {
        display: grid;
        width: 44px;
        height: 44px;
        flex: none;
        place-items: center;
        border-radius: 50%;
        background: #eff6ff;
        color: #2563eb;
      }

      .codex-pet-manager-title-group {
        display: flex;
        flex-direction: column;
        gap: 3px;

        .codex-pet-manager-title-text {
          margin: 0;
          color: var(--color-text);
          font-size: 18px;
          font-weight: 700;
          line-height: 1.25;
          letter-spacing: 0.2px;
        }

        .codex-pet-manager-title-desc {
          color: var(--color-text-muted);
          font-size: 13px;
        }
      }
    }

    .codex-pet-manager-head-actions {
      display: flex;
      align-items: center;
      gap: 8px;

      .codex-pet-manager-icon-button {
        display: grid;
        width: 34px;
        height: 34px;
        place-items: center;
        padding: 0;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.2s;

        &:hover:not(:disabled) {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }

        &:disabled {
          cursor: not-allowed;
          opacity: 0.45;
        }
      }
    }
  }

  /* 路径双卡片区域 */
  .codex-pet-manager-paths {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    margin: 16px 0 18px;
    flex: none;

    .codex-pet-path-card {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 12px 14px;
      background: var(--color-panel-soft);
      border: 1px solid var(--color-line);
      border-radius: 10px;
      cursor: pointer;
      transition: all 0.2s;

      &:hover {
        border-color: var(--color-line-strong);
        background: var(--color-panel-soft);
      }

      .codex-pet-path-icon-box {
        display: grid;
        width: 36px;
        height: 36px;
        flex: none;
        place-items: center;
        border-radius: 8px;
        background: #eff6ff;
        color: #2563eb;
      }

      .codex-pet-path-info {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 3px;

        .codex-pet-path-label {
          color: var(--color-text-muted);
          font-size: 12px;
          font-weight: 500;
        }

        .codex-pet-path-value {
          overflow: hidden;
          color: var(--color-text);
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 12px;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }

      .codex-pet-path-copy-btn {
        display: grid;
        width: 28px;
        height: 28px;
        flex: none;
        place-items: center;
        padding: 0;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.2s;

        &:hover {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }
      }
    }
  }

  /* 宠物双列网格 */
  .codex-pet-manager-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    padding-right: 2px;
    scrollbar-width: thin;
    scrollbar-color: var(--color-line) transparent;

    .codex-pet-card {
      display: flex;
      flex-direction: column;
      justify-content: space-between;
      padding: 16px;
      border: 1px solid var(--color-line);
      border-radius: 12px;
      background: var(--color-panel);
      transition: all 0.2s ease;

      &:hover {
        border-color: var(--color-line-strong);
        box-shadow: 0 4px 14px rgba(0, 0, 0, 0.04);
      }

      &.is-enabled {
        border-color: #93c5fd;
        box-shadow: 0 2px 10px rgba(37, 99, 235, 0.05);

        &:hover {
          border-color: #60a5fa;
          box-shadow: 0 4px 16px rgba(37, 99, 235, 0.08);
        }
      }

      .codex-pet-card-body {
        display: flex;
        gap: 14px;
        align-items: flex-start;
      }

      .codex-pet-preview {
        width: 90px;
        height: 98px;
        flex: 0 0 90px;
        overflow: hidden;
        border: 1px solid var(--color-line);
        border-radius: 10px;
        background-color: var(--color-panel-soft);
        background-position: 0 0;
        background-repeat: no-repeat;
        background-size: 800% auto;
        cursor: zoom-in;
        animation: codex-pet-manager-idle 1.1s steps(1, end) infinite;
        transition: transform 0.2s;

        &:hover {
          transform: scale(1.03);
        }

        &.disabled {
          animation-play-state: paused;
          filter: grayscale(1);
          opacity: 0.5;
        }
      }

      .codex-pet-info {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 3px;

        .codex-pet-header-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 8px;

          .codex-pet-name {
            overflow: hidden;
            color: var(--color-text);
            font-size: 16px;
            font-weight: 700;
            text-overflow: ellipsis;
            white-space: nowrap;
          }

          .codex-pet-status-pill {
            display: inline-flex;
            flex: none;
            align-items: center;
            gap: 5px;
            padding: 2px 8px;
            border-radius: 9999px;
            font-size: 11px;
            font-weight: 500;
            background: #f1f5f9;
            border: 1px solid #e2e8f0;
            color: #64748b;

            .status-dot {
              width: 6px;
              height: 6px;
              border-radius: 50%;
              background: #94a3b8;
            }

            &.is-enabled {
              background: #ecfdf5;
              border-color: #a7f3d0;
              color: #10b981;
              font-weight: 600;

              .status-dot {
                background: #10b981;
                box-shadow: 0 0 5px rgba(16, 185, 129, 0.4);
              }
            }
          }
        }

        .codex-pet-id {
          overflow: hidden;
          color: var(--color-text-soft);
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 12px;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .codex-pet-desc {
          margin: 4px 0 6px;
          overflow: hidden;
          color: var(--color-text-muted);
          font-size: 12px;
          line-height: 1.5;
          display: -webkit-box;
          -webkit-line-clamp: 2;
          -webkit-box-orient: vertical;
          min-height: 36px;
        }

        .codex-pet-shape-tag {
          display: inline-flex;
          width: fit-content;
          align-items: center;
          gap: 5px;
          padding: 2px 8px;
          border-radius: 5px;
          background: var(--color-panel-soft);
          border: 1px solid var(--color-line);
          color: var(--color-text-muted);
          font-size: 11px;
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        }
      }

      /* 底部操作按钮栏 */
      .codex-pet-card-actions {
        display: flex;
        align-items: center;
        gap: 8px;
        margin-top: 14px;

        .codex-pet-action-btn {
          display: inline-flex;
          height: 31px;
          flex: 1;
          align-items: center;
          justify-content: center;
          gap: 5px;
          padding: 0 8px;
          border: 1px solid var(--color-line);
          border-radius: 7px;
          background: var(--color-panel);
          color: var(--color-text);
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.2s;

          &:hover:not(:disabled) {
            border-color: var(--color-line-strong);
            background: var(--color-panel-soft);
          }

          &--toggle {
            color: #2563eb;

            &:hover:not(:disabled) {
              border-color: #93c5fd;
              background: #eff6ff;
              color: #1d4ed8;
            }
          }

          &--delete {
            color: #ef4444;

            &:hover:not(:disabled) {
              border-color: #fca5a5;
              background: #fef2f2;
              color: #dc2626;
            }
          }

          &:disabled {
            cursor: not-allowed;
            opacity: 0.45;
          }
        }
      }
    }
  }

  /* 加载与空状态 */
  .codex-pet-manager-state,
  .codex-pet-manager-empty {
    display: flex;
    min-height: 240px;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 13.5px;
    text-align: center;

    .codex-empty-icon-box {
      display: grid;
      width: 60px;
      height: 60px;
      place-items: center;
      border-radius: 50%;
      background: var(--color-panel-soft);
      border: 1px solid var(--color-line);
      color: var(--color-text-soft);
    }

    .codex-pet-manager-empty-title {
      color: var(--color-text);
      font-size: 16px;
      font-weight: 600;
    }

    .codex-pet-manager-empty-desc {
      max-width: 420px;
      font-size: 13px;
      color: var(--color-text-muted);
      line-height: 1.6;
    }
  }

  /* 动画全览网格 */
  .codex-pet-manager-animation-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
    max-height: 480px;
    overflow-y: auto;
    padding: 4px 2px;
  }

  .codex-pet-manager-animation-row {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 10px;
    padding: 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel-soft);
  }

  .codex-pet-manager-row-preview {
    width: 68px;
    height: 74px;
    flex: 0 0 68px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background-color: var(--color-panel);
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

    .codex-pet-manager-animation-row-name {
      overflow: hidden;
      color: var(--color-text);
      font-size: 13px;
      font-weight: 600;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .codex-pet-manager-animation-row-meta {
      color: var(--color-text-muted);
      font-size: 11.5px;
      font-family:
        ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    }
  }

  /* 弹窗表单样式 */
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
    font-size: 13px;
  }

  .codex-pet-manager-name-input {
    height: 38px;
    padding: 0 12px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel-soft);
    color: var(--color-text);
    font: inherit;
    font-size: 13.5px;
    outline: none;
    transition: all 0.2s;

    &:focus {
      border-color: var(--color-primary);
      box-shadow: 0 0 0 2px var(--color-primary-soft);
    }
  }

  .codex-pet-manager-modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .codex-pet-manager-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 32px;
    padding: 0 14px;
    border: 1px solid var(--color-line);
    border-radius: 7px;
    background: var(--color-panel);
    color: var(--color-text);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;

    &:hover:not(:disabled) {
      border-color: var(--color-line-strong);
      background: var(--color-panel-soft);
    }

    &.primary {
      border-color: var(--color-primary);
      background: var(--color-primary-solid);
      color: #ffffff;

      &:hover:not(:disabled) {
        background: var(--color-primary);
      }
    }

    &:disabled {
      cursor: not-allowed;
      opacity: 0.45;
    }
  }

  .spinning {
    animation: codex-pet-manager-spin 0.9s linear infinite;
  }
}

/* 动效 */
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

/* 适配暗色模式特殊颜色 */
:global(:root[data-theme="dark"]),
.tools-view--dark {
  .codex-pet-manager {
    .codex-pet-manager-badge,
    .codex-pet-path-icon-box {
      background: rgba(59, 130, 246, 0.16);
      color: #60a5fa;
    }

    .codex-pet-card.is-enabled {
      border-color: rgba(96, 165, 250, 0.4);
      box-shadow: 0 2px 10px rgba(0, 0, 0, 0.25);

      &:hover {
        border-color: rgba(96, 165, 250, 0.6);
      }
    }

    .codex-pet-card .codex-pet-status-pill.is-enabled {
      background: rgba(16, 185, 129, 0.16);
      border-color: rgba(16, 185, 129, 0.35);
      color: #34d399;

      .status-dot {
        background: #34d399;
      }
    }

    .codex-pet-card .codex-pet-action-btn--toggle {
      color: #60a5fa;

      &:hover:not(:disabled) {
        border-color: rgba(96, 165, 250, 0.4);
        background: rgba(96, 165, 250, 0.12);
        color: #93c5fd;
      }
    }

    .codex-pet-card .codex-pet-action-btn--delete {
      color: #f87171;

      &:hover:not(:disabled) {
        border-color: rgba(248, 113, 113, 0.4);
        background: rgba(239, 68, 68, 0.15);
        color: #fca5a5;
      }
    }
  }
}

@media (max-width: 900px) {
  .codex-pet-manager {
    .codex-pet-manager-grid {
      grid-template-columns: 1fr;
    }

    .codex-pet-manager-paths {
      grid-template-columns: 1fr;
    }
  }
}
</style>
