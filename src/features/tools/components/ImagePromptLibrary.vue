<template>
  <section class="image-prompt-library">
    <BaseModal
      title="提示词库"
      :description="`按分类发现画面灵感 · ${catalog.count || 0} 条社区收录提示词`"
      @close="$emit('close')"
    >
      <div class="library-toolbar">
        <div class="search-box">
          <Search :size="15" class="search-icon" />
          <input
            v-model="search"
            class="search-input"
            type="search"
            placeholder="搜索标题、提示词、风格或标签..."
            aria-label="搜索提示词"
            :disabled="initializing || importing"
          />
          <button
            v-if="search"
            class="clear-search-btn"
            type="button"
            title="清空搜索"
            @click="clearSearch"
          >
            <X :size="13" />
          </button>
        </div>

        <div class="library-meta-chips">
          <span
            class="meta-chip database-chip"
            :title="`收录文件: ${catalog.filename}`"
          >
            <Database :size="12" />
            <span class="tech-dot"></span>
            <span>{{ catalog.count || 0 }} 灵感库</span>
          </span>
          <span
            v-if="catalog.filename"
            class="meta-chip source-file-chip"
            :title="catalog.filename"
          >
            <Terminal :size="12" />
            <span class="filename-text">{{ catalog.filename }}</span>
          </span>
        </div>

        <button
          class="library-button import-btn"
          type="button"
          :disabled="initializing || importing"
          @click="importLibrary"
        >
          <Upload :size="14" :class="{ 'spinning-icon': importing }" />
          <span>{{ importing ? "导入中…" : "导入 JSON" }}</span>
        </button>
      </div>

      <p v-if="error" class="library-error" role="alert">
        <span>{{ error }}</span>
        <button class="retry-button" type="button" @click="initialize">
          重试
        </button>
      </p>

      <div v-if="initializing" class="library-loading">
        <div class="tech-loader">
          <LoaderCircle class="spinning" :size="26" />
        </div>
        <span>正在加载本地提示词库，首次打开需要建立索引…</span>
      </div>

      <div v-else class="library-body">
        <nav class="category-sidebar" aria-label="提示词分类">
          <div class="sidebar-header">
            <span class="sidebar-title">// 分类导航</span>
            <span class="sidebar-count">{{
              catalog.categories?.length || 0
            }}</span>
          </div>

          <button
            class="category-button all-category-btn"
            :class="{ active: !category }"
            type="button"
            @click="selectCategory('')"
          >
            <span class="cat-left">
              <Layers :size="14" class="cat-icon" />
              <span class="category-name">全部提示词</span>
            </span>
            <span class="category-count">{{ catalog.count }}</span>
          </button>

          <div class="category-scroll-container">
            <section
              v-for="group in categoryGroups"
              :key="group.name"
              class="category-group"
            >
              <div class="category-heading">
                <span class="group-prefix">//</span>
                <span class="group-name">{{ group.name }}</span>
              </div>
              <button
                v-for="item in group.items"
                :key="item.id"
                class="category-button"
                :class="{ active: category === item.id }"
                type="button"
                :title="item.originalTitle"
                @click="selectCategory(item.id)"
              >
                <span class="cat-left">
                  <span class="active-indicator"></span>
                  <span class="category-name">{{ item.title }}</span>
                </span>
                <span class="category-count">{{ item.count }}</span>
              </button>
            </section>
          </div>
        </nav>

        <section class="library-main">
          <template v-if="!detail">
            <header class="gallery-heading">
              <div class="heading-left">
                <span class="heading-title">{{ activeCategory }}</span>
                <span class="result-count-badge">
                  <span class="live-dot"></span>
                  <span>{{ total }} 条匹配</span>
                </span>
              </div>
              <div class="heading-right">
                <span v-if="loading" class="loading-label">
                  <LoaderCircle :size="13" class="spinning" />
                  <span>加载中…</span>
                </span>
                <span class="page-quick-tag">
                  PAGE {{ String(page).padStart(2, "0") }} /
                  {{ String(totalPages).padStart(2, "0") }}
                </span>
              </div>
            </header>

            <div
              ref="galleryScrollRef"
              class="prompt-gallery-scroll"
              :aria-busy="loading"
              @scroll.passive="onGalleryScroll"
            >
              <div v-if="!items.length && !loading" class="library-empty">
                <div class="empty-icon-wrapper">
                  <Search :size="30" />
                </div>
                <span class="empty-title">没有找到匹配的提示词</span>
                <span class="empty-hint">试试其他分类或换个关键词搜索</span>
                <button
                  v-if="category || search"
                  class="empty-reset-button"
                  type="button"
                  @click="resetFilter"
                >
                  重置检索条件
                </button>
              </div>

              <div v-else class="prompt-gallery">
                <div
                  v-for="item in items"
                  :key="item.id"
                  class="prompt-card"
                  :class="{ 'is-edit-mode': isEditMode(item.inputMode) }"
                  role="button"
                  tabindex="0"
                  :aria-disabled="detailLoading || importing"
                  @click="openDetail(item.id)"
                  @keydown.enter="openDetail(item.id)"
                >
                  <div class="card-image">
                    <el-image
                      v-if="item.previewImageUrl || item.highQualityImageUrl"
                      class="preview-image"
                      :src="item.previewImageUrl || item.highQualityImageUrl"
                      fit="cover"
                      lazy
                      scroll-container=".prompt-gallery-scroll"
                      referrerpolicy="no-referrer"
                    >
                      <template #placeholder>
                        <div class="image-placeholder">
                          <Image :size="22" />
                        </div>
                      </template>
                      <template #error>
                        <div class="image-placeholder error-placeholder">
                          <ImageOff :size="22" />
                          <span>预览图不可用</span>
                        </div>
                      </template>
                    </el-image>
                    <div v-else class="image-placeholder">
                      <ImageOff :size="22" />
                      <span>暂无预览图</span>
                    </div>

                    <div class="image-overlay-gradient"></div>

                    <div class="badge-row">
                      <span
                        class="mode-badge"
                        :class="{ 'mode-edit': isEditMode(item.inputMode) }"
                      >
                        <span class="badge-dot"></span>
                        <span>{{ modeLabel(item.inputMode) }}</span>
                      </span>
                    </div>

                    <div class="card-hover-overlay">
                      <span class="hover-inspect-btn">
                        <Sparkles :size="12" />
                        <span>查看详情</span>
                      </span>
                    </div>
                  </div>

                  <div class="card-content">
                    <div class="card-title-row">
                      <span class="card-title" :title="item.title">
                        {{ item.title }}
                      </span>
                    </div>
                    <div class="card-footer-row">
                      <span class="card-category-pill">
                        # {{ categoryTitle(item.categoryIds?.[0]) }}
                      </span>
                      <span
                        v-if="item.categoryIds?.length > 1"
                        class="more-tags-indicator"
                      >
                        +{{ item.categoryIds.length - 1 }}
                      </span>
                    </div>
                  </div>
                </div>
              </div>

              <!-- 回到顶部浮动按钮 -->
              <button
                v-if="showBackToTop"
                class="floating-back-to-top"
                type="button"
                title="回到顶部"
                @click="scrollToTop('smooth')"
              >
                <ArrowUp :size="14" />
                <span>顶部</span>
              </button>
            </div>

            <footer class="gallery-footer">
              <span class="footer-hint">
                <ExternalLink :size="12" />
                <span>图片来自公开收录链接 · 点击卡片查看完整提示词与参数</span>
              </span>
              <div class="pagination-controls">
                <button
                  class="page-nav-button"
                  type="button"
                  title="上一页"
                  :disabled="page <= 1 || loading"
                  @click="changePage(-1)"
                >
                  <ChevronLeft :size="14" />
                </button>
                <div class="page-indicator">
                  <span class="current-page">{{
                    String(page).padStart(2, "0")
                  }}</span>
                  <span class="page-separator">/</span>
                  <span class="total-pages">{{
                    String(totalPages).padStart(2, "0")
                  }}</span>
                </div>
                <button
                  class="page-nav-button"
                  type="button"
                  title="下一页"
                  :disabled="page >= totalPages || loading"
                  @click="changePage(1)"
                >
                  <ChevronRight :size="14" />
                </button>
              </div>
            </footer>
          </template>

          <template v-else>
            <header class="detail-heading">
              <button class="back-button" type="button" @click="closeDetail">
                <ArrowLeft :size="14" />
                <span>返回词库列表</span>
              </button>
              <div class="detail-heading-info">
                <span class="detail-category-badge">
                  # {{ categoryTitle(detail.categoryIds?.[0]) }}
                </span>
                <span class="detail-title" :title="detail.title">{{
                  detail.title
                }}</span>
              </div>
            </header>

            <div class="prompt-detail">
              <div class="detail-visual">
                <div class="detail-image-wrapper">
                  <el-image
                    class="detail-image"
                    :src="detail.highQualityImageUrl || detail.previewImageUrl"
                    :preview-src-list="
                      [
                        detail.highQualityImageUrl || detail.previewImageUrl
                      ].filter(Boolean)
                    "
                    fit="contain"
                    preview-teleported
                    referrerpolicy="no-referrer"
                  >
                    <template #placeholder>
                      <div class="image-placeholder">
                        <LoaderCircle class="spinning" :size="24" />
                        <span>加载原图中…</span>
                      </div>
                    </template>
                    <template #error>
                      <div class="image-placeholder">
                        <ImageOff :size="32" />
                        <span>图片链接暂不可用，仍可使用提示词</span>
                      </div>
                    </template>
                  </el-image>
                  <div class="corner-bracket top-left"></div>
                  <div class="corner-bracket top-right"></div>
                  <div class="corner-bracket bottom-left"></div>
                  <div class="corner-bracket bottom-right"></div>
                </div>

                <div class="detail-tags-box">
                  <div class="tags-header">// 标签分类</div>
                  <div class="detail-tags">
                    <span
                      v-for="id in detail.categoryIds"
                      :key="id"
                      class="category-tag"
                    >
                      # {{ categoryTitle(id) }}
                    </span>
                  </div>
                </div>

                <button
                  v-if="sourceUrl"
                  class="source-link-button"
                  type="button"
                  @click="openSource(sourceUrl)"
                >
                  <ExternalLink :size="12" />
                  <span>查看原始收录来源</span>
                </button>
              </div>

              <div class="detail-editor">
                <div class="console-box">
                  <div class="editor-heading">
                    <div class="console-title">
                      <Terminal :size="14" class="console-icon" />
                      <span>PROMPT CONSOLE // 提示词</span>
                    </div>

                    <div class="editor-controls">
                      <div class="lang-switch-box">
                        <label class="lang-label">语言:</label>
                        <select
                          v-model="language"
                          class="language-select"
                          aria-label="提示词语言"
                        >
                          <option value="original">英文/原文</option>
                          <option v-if="detail.promptLocalized?.zh" value="zh">
                            中文翻译
                          </option>
                        </select>
                      </div>

                      <button
                        class="copy-prompt-btn"
                        type="button"
                        :class="{ copied }"
                        title="复制提示词"
                        @click="copyPrompt"
                      >
                        <Check v-if="copied" :size="13" />
                        <Copy v-else :size="13" />
                        <span>{{ copied ? "已复制" : "复制" }}</span>
                      </button>
                    </div>
                  </div>

                  <div class="textarea-wrapper">
                    <textarea
                      v-model="prompt"
                      class="prompt-text"
                      aria-label="可编辑的提示词"
                      spellcheck="false"
                      placeholder="提示词内容..."
                    ></textarea>
                    <div class="prompt-metrics-bar">
                      <span :class="{ 'warning-limit': promptTooLong }">
                        {{ prompt.length }} 字符 ·
                        {{ (promptByteLength / 1024).toFixed(1) }} KB / 32 KB
                      </span>
                    </div>
                  </div>
                </div>

                <div v-if="detail.variables?.length" class="tech-hint-card">
                  <Sparkles :size="14" class="hint-icon" />
                  <span
                    >此模板含可替换内容，可直接在上方控制台修改占位参数。</span
                  >
                </div>

                <div class="template-meta-strip">
                  <div class="meta-strip-item">
                    <Cpu :size="13" />
                    <span class="meta-strip-label">推荐模型:</span>
                    <span class="meta-strip-value">{{
                      detail.model || "沿用当前所选模型"
                    }}</span>
                  </div>
                  <div class="meta-strip-item">
                    <Layers :size="13" />
                    <span class="meta-strip-label">生成模式:</span>
                    <span class="meta-strip-value">{{
                      modeLabel(detail.inputMode)
                    }}</span>
                  </div>
                </div>

                <div
                  v-if="detail.inputMode !== 'text_to_image'"
                  class="tech-hint-card edit-hint-card"
                >
                  <Image :size="14" class="hint-icon" />
                  <span
                    >图片编辑模式：载入工作台后需提供参考图，示例预览图不会作为输入。</span
                  >
                </div>

                <p v-if="promptTooLong" class="detail-error">
                  提示词超过工作台的 32000 字节限制，请精简后使用。
                </p>

                <div class="detail-action-bar">
                  <button
                    class="use-button"
                    type="button"
                    :disabled="!prompt.trim() || promptTooLong || importing"
                    @click="usePrompt"
                  >
                    <Sparkles :size="15" />
                    <span>填入生图工作台</span>
                  </button>
                </div>
              </div>
            </div>
          </template>
        </section>
      </div>

      <footer class="library-attribution">
        <span class="attribution-label">// 提示词整理自社区项目：</span>
        <a
          class="attribution-link"
          :href="sourceRepositoryUrl"
          @click.prevent="openSource(sourceRepositoryUrl)"
        >
          Toolcentral-ai / Image Prompt Gallery
          <ExternalLink :size="11" />
        </a>
      </footer>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  ArrowLeft,
  ArrowUp,
  Check,
  ChevronLeft,
  ChevronRight,
  Copy,
  Cpu,
  Database,
  ExternalLink,
  Image,
  ImageOff,
  Layers,
  LoaderCircle,
  Search,
  Sparkles,
  Terminal,
  Upload,
  X
} from "lucide-vue-next"
import { ElImage, ElMessageBox } from "element-plus"
import "element-plus/es/components/image/style/css"
import "element-plus/es/components/message-box/style/css"
import BaseModal from "@/components/BaseModal.vue"
import { systemApi, toolboxApi } from "@/api"
import { createMessage } from "@/utils/message"

const emit = defineEmits(["close", "select"])
// 保留词库整理来源，桌面端统一通过系统浏览器打开外链。
const sourceRepositoryUrl =
  "https://github.com/Toolcentral-ai/awesome-gpt-image-2-prompts"
const catalog = ref({ count: 0, categories: [], filename: "" })
const category = ref("")
const search = ref("")
const page = ref(1)
const items = ref([])
const total = ref(0)
const detail = ref(null)
const prompt = ref("")
const language = ref("original")
const initializing = ref(true)
const loading = ref(false)
const detailLoading = ref(false)
const importing = ref(false)
const error = ref("")
const copied = ref(false)
const showBackToTop = ref(false)
const galleryScrollRef = ref(null)

let copyTimer
let searchTimer
let requestVersion = 0
let detailVersion = 0
let disposed = false

const categoryMap = computed(
  () => new Map(catalog.value.categories.map((item) => [item.id, item]))
)
const categoryGroups = computed(() => {
  const groups = new Map()
  for (const item of catalog.value.categories) {
    const name =
      { Category: "分类", Style: "风格", Scene: "场景", "Use Case": "用途" }[
        item.group
      ] ||
      item.group ||
      "分类"
    if (!groups.has(name)) groups.set(name, [])
    groups.get(name).push(item)
  }
  return [...groups].map(([name, entries]) => ({ name, items: entries }))
})
const activeCategory = computed(
  () => categoryMap.value.get(category.value)?.title || "全部提示词"
)
const totalPages = computed(() => Math.max(1, Math.ceil(total.value / 24)))
const promptByteLength = computed(
  () => new TextEncoder().encode(prompt.value).length
)
const promptTooLong = computed(() => promptByteLength.value > 32000)

const sourceUrl = computed(() => {
  const value =
    detail.value?.source?.originalSourceUrl ||
    detail.value?.source?.caseUrl ||
    ""
  try {
    return ["https:", "http:"].includes(new URL(value).protocol) ? value : ""
  } catch {
    return ""
  }
})

function categoryTitle(id) {
  return categoryMap.value.get(id)?.title || "未分类"
}

function modeLabel(mode) {
  return (
    {
      text_to_image: "文生图",
      single_image_to_image: "图片编辑",
      multi_image_to_image: "多图编辑"
    }[mode] || "文生图"
  )
}

function isEditMode(mode) {
  return ["single_image_to_image", "multi_image_to_image"].includes(mode)
}

function scrollToTop(behavior = "smooth") {
  nextTick(() => {
    if (galleryScrollRef.value) {
      if (typeof galleryScrollRef.value.scrollTo === "function") {
        galleryScrollRef.value.scrollTo({ top: 0, behavior })
      } else {
        galleryScrollRef.value.scrollTop = 0
      }
    }
  })
}

function onGalleryScroll(e) {
  showBackToTop.value = (e.target?.scrollTop || 0) > 220
}

async function loadList() {
  const version = ++requestVersion
  loading.value = true
  error.value = ""
  try {
    const result = await toolboxApi.listImagePrompts({
      category: category.value,
      query: search.value.trim(),
      page: page.value
    })
    // 搜索和分类切换只应用最新结果，避免旧请求覆盖新列表。
    if (disposed || version !== requestVersion) return
    items.value = result.items
    total.value = result.total
    page.value = result.page
    scrollToTop("instant")
  } catch (cause) {
    if (!disposed && version === requestVersion) error.value = String(cause)
  } finally {
    if (version === requestVersion) loading.value = false
  }
}

async function initialize() {
  initializing.value = true
  error.value = ""
  try {
    catalog.value = await toolboxApi.imagePromptCatalog()
    if (!disposed) await loadList()
  } catch (cause) {
    error.value = String(cause)
  } finally {
    initializing.value = false
  }
}

function selectCategory(id) {
  window.clearTimeout(searchTimer)
  category.value = id
  page.value = 1
  detail.value = null
  detailVersion++
  loadList()
  scrollToTop("instant")
}

function changePage(delta) {
  page.value += delta
  loadList()
  scrollToTop("smooth")
}

function clearSearch() {
  search.value = ""
  page.value = 1
  detail.value = null
  loadList()
  scrollToTop("instant")
}

function resetFilter() {
  category.value = ""
  search.value = ""
  page.value = 1
  detail.value = null
  loadList()
  scrollToTop("instant")
}

function closeDetail() {
  detail.value = null
  scrollToTop("instant")
}

async function openDetail(id) {
  const version = ++detailVersion
  detailLoading.value = true
  try {
    const result = await toolboxApi.imagePromptDetail({ id })
    if (disposed || version !== detailVersion) return
    detail.value = result
    language.value = "original"
    prompt.value = result.prompt
  } catch (cause) {
    error.value = String(cause)
  } finally {
    detailLoading.value = false
  }
}

function usePrompt() {
  // 只填入草稿，由用户确认后提交，预览图片不作为上传图片。
  emit("select", {
    prompt: prompt.value,
    model: detail.value.model,
    mode: isEditMode(detail.value.inputMode) ? "edit" : "generate"
  })
}

async function copyPrompt() {
  if (!prompt.value) return
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(prompt.value)
    } else {
      const textarea = document.createElement("textarea")
      textarea.value = prompt.value
      textarea.style.position = "fixed"
      textarea.style.opacity = "0"
      document.body.appendChild(textarea)
      textarea.select()
      document.execCommand("copy")
      document.body.removeChild(textarea)
    }
    copied.value = true
    createMessage.success("提示词已复制到剪贴板")
    window.clearTimeout(copyTimer)
    copyTimer = window.setTimeout(() => {
      copied.value = false
    }, 2000)
  } catch (cause) {
    createMessage.error("复制失败：" + String(cause))
  }
}

async function openSource(url) {
  try {
    await systemApi.openExternal({ url })
  } catch (cause) {
    createMessage.error(String(cause))
  }
}

async function importLibrary() {
  importing.value = true
  try {
    const path = await systemApi.selectFile({
      title: "导入图片提示词库",
      filters: [{ name: "JSON 提示词库", extensions: ["json"] }]
    })
    if (!path) return
    try {
      await ElMessageBox.confirm(
        "导入后将替换当前提示词库，已生成的图片任务不受影响。",
        "导入提示词库",
        { confirmButtonText: "导入", cancelButtonText: "取消", type: "warning" }
      )
    } catch {
      return
    }
    window.clearTimeout(searchTimer)
    requestVersion++
    detailVersion++
    catalog.value = await toolboxApi.importImagePrompts({ path })
    category.value = ""
    search.value = ""
    page.value = 1
    detail.value = null
    await loadList()
    createMessage.success(`已导入 ${catalog.value.count} 条提示词`)
  } catch (cause) {
    error.value = String(cause)
  } finally {
    importing.value = false
  }
}

watch(search, () => {
  window.clearTimeout(searchTimer)
  requestVersion++
  detailVersion++
  detail.value = null
  page.value = 1
  if (!importing.value) searchTimer = window.setTimeout(loadList, 300)
})

watch(language, () => {
  if (detail.value)
    prompt.value =
      language.value === "zh"
        ? detail.value.promptLocalized.zh
        : detail.value.prompt
})

onMounted(initialize)

onBeforeUnmount(() => {
  disposed = true
  requestVersion++
  detailVersion++
  window.clearTimeout(searchTimer)
  window.clearTimeout(copyTimer)
})
</script>

<style scoped lang="less">
.image-prompt-library {
  font-size: var(--font-size-base);

  :deep(.base-modal__panel) {
    width: calc(100vw - 44px);
    max-width: 1260px;
    height: calc(100vh - 44px);
    max-height: 900px;
    border-radius: 12px;
    border: 1px solid
      color-mix(in srgb, var(--color-line-strong) 80%, var(--color-primary));
    box-shadow:
      0 20px 50px rgba(0, 0, 0, 0.28),
      0 0 0 1px color-mix(in srgb, var(--color-primary) 20%, transparent);
  }

  :deep(.base-modal__header) {
    padding: 16px 22px 14px;
    border-bottom: 1px solid
      color-mix(in srgb, var(--color-line) 85%, var(--color-primary));
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--color-panel) 94%, var(--color-primary-soft)) 0%,
      var(--color-panel) 100%
    );
  }

  :deep(.base-modal__title) {
    font-weight: 700;
    letter-spacing: 0.5px;
  }

  :deep(.base-modal__description) {
    font-family: var(--font-family-mono, inherit);
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  :deep(.base-modal__content) {
    padding: 14px 20px 14px;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  // --- Toolbar Deck ---
  .library-toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-bottom: 14px;
    flex: none;

    .search-box {
      display: flex;
      flex: 1;
      align-items: center;
      gap: 8px;
      padding: 0 12px;
      height: 38px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      transition: all 0.2s ease;

      &:focus-within {
        border-color: var(--color-primary);
        box-shadow: 0 0 0 2px
          color-mix(in srgb, var(--color-primary) 25%, transparent);
        background: var(--color-panel);
      }

      .search-icon {
        color: var(--color-primary);
        flex: none;
      }

      .search-input {
        flex: 1;
        min-width: 0;
        border: 0;
        outline: none;
        color: var(--color-text);
        background: transparent;
        font-size: var(--font-size-sm);

        &::placeholder {
          color: var(--color-text-soft);
        }
      }

      .clear-search-btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 20px;
        height: 20px;
        padding: 0;
        border: 0;
        border-radius: 4px;
        background: var(--color-line);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.15s ease;

        &:hover {
          background: color-mix(
            in srgb,
            var(--color-primary) 30%,
            var(--color-line)
          );
          color: var(--color-text);
        }
      }
    }

    .library-meta-chips {
      display: flex;
      align-items: center;
      gap: 8px;
      flex: none;

      .meta-chip {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        height: 34px;
        padding: 0 10px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
        font-family: var(--font-family-mono, monospace);

        .tech-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          background: #10b981;
          box-shadow: 0 0 6px #10b981;
        }

        &.source-file-chip {
          max-width: 210px;

          .filename-text {
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }
        }
      }
    }

    .library-button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 7px;
      height: 38px;
      padding: 0 14px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-text);
      cursor: pointer;
      font-size: var(--font-size-sm);
      font-weight: 500;
      transition: all 0.18s ease;

      &:hover:not(:disabled) {
        border-color: var(--color-primary);
        color: var(--color-primary);
        box-shadow: 0 2px 8px
          color-mix(in srgb, var(--color-primary) 18%, transparent);
      }

      &:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      .spinning-icon {
        animation: tech-spin 1.4s linear infinite;
      }
    }
  }

  // --- Error & Loading ---
  .library-error {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 12px;
    padding: 10px 14px;
    border: 1px solid var(--color-danger-line);
    border-radius: 6px;
    background: var(--color-danger-soft);
    color: var(--color-danger);
    font-size: var(--font-size-sm);
    overflow-wrap: anywhere;

    .retry-button {
      border: 0;
      background: transparent;
      color: var(--color-primary);
      text-decoration: underline;
      cursor: pointer;
      font-weight: 600;
    }
  }

  .library-loading {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    color: var(--color-text-muted);
    font-size: var(--font-size-base);

    .tech-loader {
      display: flex;
      align-items: center;
      justify-content: center;
      width: 48px;
      height: 48px;
      border-radius: 50%;
      background: color-mix(in srgb, var(--color-primary) 12%, transparent);
      border: 1px solid
        color-mix(in srgb, var(--color-primary) 25%, transparent);

      .spinning {
        animation: tech-spin 1.2s linear infinite;
        color: var(--color-primary);
      }
    }
  }

  // --- Main Body Layout ---
  .library-body {
    display: flex;
    flex: 1;
    gap: 18px;
    min-height: 0;

    // --- Left Sidebar ---
    .category-sidebar {
      display: flex;
      flex-direction: column;
      flex: 0 0 210px;
      border-right: 1px solid var(--color-line);
      padding-right: 10px;
      min-height: 0;

      .sidebar-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 4px 6px 10px;
        border-bottom: 1px dashed var(--color-line);
        margin-bottom: 8px;

        .sidebar-title {
          font-size: var(--font-size-xs);
          font-family: var(--font-family-mono, monospace);
          font-weight: 600;
          color: var(--color-text-soft);
          letter-spacing: 0.8px;
        }

        .sidebar-count {
          padding: 1px 6px;
          border-radius: 10px;
          background: var(--color-panel-soft);
          border: 1px solid var(--color-line);
          font-size: 11px;
          font-family: var(--font-family-mono, monospace);
          color: var(--color-text-muted);
        }
      }

      .all-category-btn {
        margin-bottom: 6px;
      }

      .category-scroll-container {
        flex: 1;
        overflow-y: auto;
        min-height: 0;
        padding-right: 4px;

        &::-webkit-scrollbar {
          width: 4px;
        }
        &::-webkit-scrollbar-thumb {
          background: var(--color-line);
          border-radius: 2px;
        }
      }

      .category-button {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 6px;
        width: 100%;
        min-height: 36px;
        padding: 7px 10px;
        margin-bottom: 3px;
        border: 1px solid transparent;
        border-radius: 6px;
        text-align: left;
        background: transparent;
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
        cursor: pointer;
        transition: all 0.16s ease;
        position: relative;

        .cat-left {
          display: flex;
          align-items: center;
          gap: 7px;
          min-width: 0;

          .cat-icon {
            flex: none;
            opacity: 0.8;
          }

          .active-indicator {
            width: 4px;
            height: 12px;
            border-radius: 2px;
            background: transparent;
            transition: all 0.16s ease;
          }

          .category-name {
            overflow: hidden;
            white-space: nowrap;
            text-overflow: ellipsis;
          }
        }

        .category-count {
          font-family: var(--font-family-mono, monospace);
          font-size: 11px;
          padding: 1px 6px;
          border-radius: 4px;
          background: var(--color-panel-soft);
          color: var(--color-text-soft);
          transition: all 0.16s ease;
        }

        &:hover {
          background: var(--color-panel-soft);
          color: var(--color-text);
          transform: translateX(2px);
        }

        &.active {
          background: color-mix(
            in srgb,
            var(--color-primary-soft) 85%,
            var(--color-panel)
          );
          border-color: color-mix(
            in srgb,
            var(--color-primary) 35%,
            transparent
          );
          color: var(--color-primary);
          font-weight: 600;

          .cat-left .active-indicator {
            background: var(--color-primary);
            box-shadow: 0 0 6px var(--color-primary);
          }

          .category-count {
            background: color-mix(
              in srgb,
              var(--color-primary) 20%,
              transparent
            );
            color: var(--color-primary);
          }
        }
      }

      .category-group {
        margin-top: 14px;

        .category-heading {
          display: flex;
          align-items: center;
          gap: 4px;
          padding: 3px 6px 6px;
          color: var(--color-text-soft);
          font-size: 11px;
          font-family: var(--font-family-mono, monospace);
          text-transform: uppercase;
          letter-spacing: 0.8px;

          .group-prefix {
            color: var(--color-primary);
            opacity: 0.7;
          }
        }
      }
    }

    // --- Main Right Content Area ---
    .library-main {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-width: 0;
      min-height: 0;

      .gallery-heading {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0 0 12px;
        flex: none;

        .heading-left {
          display: flex;
          align-items: center;
          gap: 10px;

          .heading-title {
            font-size: var(--font-size-lg);
            font-weight: 700;
            color: var(--color-text);
          }

          .result-count-badge {
            display: inline-flex;
            align-items: center;
            gap: 6px;
            padding: 2px 8px;
            border-radius: 12px;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            font-size: var(--font-size-xs);
            font-family: var(--font-family-mono, monospace);
            color: var(--color-text-muted);

            .live-dot {
              width: 6px;
              height: 6px;
              border-radius: 50%;
              background: var(--color-primary);
              box-shadow: 0 0 5px var(--color-primary);
              animation: tech-pulse 2s infinite ease-in-out;
            }
          }
        }

        .heading-right {
          display: flex;
          align-items: center;
          gap: 10px;

          .loading-label {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            color: var(--color-primary);
            font-size: var(--font-size-xs);
            font-family: var(--font-family-mono, monospace);
          }

          .page-quick-tag {
            font-family: var(--font-family-mono, monospace);
            font-size: var(--font-size-xs);
            color: var(--color-text-soft);
            background: var(--color-panel-soft);
            padding: 2px 8px;
            border-radius: 4px;
            border: 1px solid var(--color-line);
          }
        }
      }

      // --- Prompt Cards Gallery (Scrollable) ---
      .prompt-gallery-scroll {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding-right: 6px;
        position: relative;

        &::-webkit-scrollbar {
          width: 6px;
        }
        &::-webkit-scrollbar-thumb {
          background: var(--color-line-strong);
          border-radius: 3px;
        }

        .library-empty {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: 12px;
          height: 100%;
          min-height: 320px;
          color: var(--color-text-muted);

          .empty-icon-wrapper {
            display: flex;
            align-items: center;
            justify-content: center;
            width: 64px;
            height: 64px;
            border-radius: 50%;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            color: var(--color-text-soft);
          }

          .empty-title {
            font-size: var(--font-size-base);
            font-weight: 600;
            color: var(--color-text);
          }

          .empty-hint {
            color: var(--color-text-soft);
            font-size: var(--font-size-sm);
          }

          .empty-reset-button {
            margin-top: 6px;
            padding: 6px 14px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel);
            color: var(--color-primary);
            font-size: var(--font-size-sm);
            cursor: pointer;
            transition: all 0.15s ease;

            &:hover {
              border-color: var(--color-primary);
              background: var(--color-primary-soft);
            }
          }
        }

        .prompt-gallery {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
          gap: 14px;
          padding-bottom: 8px;

          .prompt-card {
            display: flex;
            flex-direction: column;
            border: 1px solid var(--color-line);
            border-radius: 9px;
            background: var(--color-panel);
            color: var(--color-text);
            text-align: left;
            cursor: pointer;
            overflow: hidden;
            position: relative;
            transition:
              transform 0.2s ease,
              border-color 0.2s ease,
              box-shadow 0.2s ease;

            &::before {
              content: "";
              position: absolute;
              top: 0;
              left: 0;
              right: 0;
              height: 2px;
              background: linear-gradient(
                90deg,
                transparent,
                var(--color-primary),
                transparent
              );
              opacity: 0;
              transition: opacity 0.25s ease;
              z-index: 2;
            }

            &:hover {
              transform: translateY(-3px);
              border-color: color-mix(
                in srgb,
                var(--color-primary) 70%,
                var(--color-line)
              );
              box-shadow:
                0 10px 24px -4px rgba(0, 0, 0, 0.18),
                0 0 0 1px
                  color-mix(in srgb, var(--color-primary) 28%, transparent);

              &::before {
                opacity: 1;
              }

              .card-image .card-hover-overlay {
                opacity: 1;
              }
            }

            &:focus-visible {
              outline: 2px solid var(--color-primary);
              outline-offset: 1px;
            }

            .card-image {
              position: relative;
              width: 100%;
              height: 156px;
              background: var(--color-panel-soft);
              overflow: hidden;

              .preview-image {
                width: 100%;
                height: 100%;
                display: block;
                transition: transform 0.3s ease;
              }

              .image-overlay-gradient {
                position: absolute;
                inset: 0;
                background: linear-gradient(
                  180deg,
                  rgba(0, 0, 0, 0.05) 0%,
                  rgba(0, 0, 0, 0.45) 100%
                );
                pointer-events: none;
              }

              .badge-row {
                position: absolute;
                bottom: 8px;
                left: 8px;
                display: flex;
                align-items: center;
                gap: 6px;
                z-index: 1;

                .mode-badge {
                  display: inline-flex;
                  align-items: center;
                  gap: 5px;
                  padding: 3px 8px;
                  border-radius: 4px;
                  background: rgba(15, 23, 42, 0.78);
                  backdrop-filter: blur(8px);
                  border: 1px solid rgba(255, 255, 255, 0.16);
                  color: #ffffff;
                  font-size: 11px;
                  font-weight: 500;
                  line-height: 1;

                  .badge-dot {
                    width: 5px;
                    height: 5px;
                    border-radius: 50%;
                    background: #06b6d4;
                    box-shadow: 0 0 5px #06b6d4;
                  }

                  &.mode-edit .badge-dot {
                    background: #f59e0b;
                    box-shadow: 0 0 5px #f59e0b;
                  }
                }
              }

              .card-hover-overlay {
                position: absolute;
                inset: 0;
                background: color-mix(
                  in srgb,
                  var(--color-primary) 20%,
                  rgba(0, 0, 0, 0.38)
                );
                display: flex;
                align-items: center;
                justify-content: center;
                opacity: 0;
                transition: opacity 0.2s ease;
                z-index: 2;

                .hover-inspect-btn {
                  display: inline-flex;
                  align-items: center;
                  gap: 5px;
                  padding: 6px 12px;
                  border-radius: 20px;
                  background: rgba(15, 23, 42, 0.88);
                  border: 1px solid rgba(255, 255, 255, 0.3);
                  color: #ffffff;
                  font-size: var(--font-size-xs);
                  font-weight: 600;
                  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
                }
              }
            }

            .card-content {
              display: flex;
              flex-direction: column;
              gap: 8px;
              padding: 12px;
              background: var(--color-panel);

              .card-title-row {
                .card-title {
                  display: -webkit-box;
                  -webkit-line-clamp: 2;
                  line-clamp: 2;
                  -webkit-box-orient: vertical;
                  overflow: hidden;
                  min-height: 2.8em;
                  line-height: 1.45;
                  font-size: var(--font-size-sm);
                  font-weight: 600;
                  color: var(--color-text);
                }
              }

              .card-footer-row {
                display: flex;
                align-items: center;
                justify-content: space-between;
                gap: 6px;

                .card-category-pill {
                  color: var(--color-text-soft);
                  font-size: 11px;
                  overflow: hidden;
                  text-overflow: ellipsis;
                  white-space: nowrap;
                  font-family: var(--font-family-mono, monospace);
                }

                .more-tags-indicator {
                  color: var(--color-text-soft);
                  font-size: 10px;
                  font-family: var(--font-family-mono, monospace);
                  padding: 1px 4px;
                  border-radius: 3px;
                  background: var(--color-panel-soft);
                }
              }
            }
          }
        }

        // --- Back to Top Button ---
        .floating-back-to-top {
          position: sticky;
          bottom: 12px;
          left: 100%;
          margin-right: 12px;
          display: inline-flex;
          align-items: center;
          gap: 5px;
          padding: 6px 12px;
          border-radius: 20px;
          border: 1px solid
            color-mix(in srgb, var(--color-primary) 50%, var(--color-line));
          background: color-mix(
            in srgb,
            var(--color-panel) 90%,
            var(--color-primary-soft)
          );
          color: var(--color-primary);
          box-shadow: 0 6px 18px rgba(0, 0, 0, 0.22);
          font-size: var(--font-size-xs);
          font-weight: 600;
          cursor: pointer;
          transition: all 0.18s ease;
          z-index: 10;

          &:hover {
            transform: translateY(-2px);
            background: var(--color-primary);
            color: #ffffff;
            border-color: var(--color-primary);
          }
        }
      }

      // --- Pagination Footer ---
      .gallery-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        padding-top: 12px;
        border-top: 1px solid var(--color-line);
        flex: none;

        .footer-hint {
          display: inline-flex;
          align-items: center;
          gap: 5px;
          color: var(--color-text-soft);
          font-size: var(--font-size-xs);
        }

        .pagination-controls {
          display: flex;
          align-items: center;
          gap: 6px;

          .page-nav-button {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            width: 32px;
            height: 32px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel);
            color: var(--color-text);
            cursor: pointer;
            transition: all 0.15s ease;

            &:hover:not(:disabled) {
              border-color: var(--color-primary);
              color: var(--color-primary);
              background: var(--color-primary-soft);
            }

            &:disabled {
              opacity: 0.4;
              cursor: not-allowed;
            }
          }

          .page-indicator {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            padding: 0 10px;
            height: 32px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel-soft);
            font-family: var(--font-family-mono, monospace);
            font-size: var(--font-size-xs);

            .current-page {
              color: var(--color-primary);
              font-weight: 700;
            }

            .page-separator {
              color: var(--color-text-soft);
              opacity: 0.6;
            }

            .total-pages {
              color: var(--color-text-muted);
            }
          }
        }
      }

      // --- Detail View ---
      .detail-heading {
        display: flex;
        align-items: center;
        gap: 14px;
        padding: 0 0 14px;
        border-bottom: 1px solid var(--color-line);
        flex: none;

        .back-button {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          padding: 6px 12px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel-soft);
          color: var(--color-text);
          font-size: var(--font-size-xs);
          font-weight: 600;
          cursor: pointer;
          transition: all 0.15s ease;
          flex: none;

          &:hover {
            border-color: var(--color-primary);
            color: var(--color-primary);
            background: var(--color-primary-soft);
          }
        }

        .detail-heading-info {
          display: flex;
          align-items: center;
          gap: 8px;
          min-width: 0;

          .detail-category-badge {
            padding: 2px 8px;
            border-radius: 4px;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            color: var(--color-text-muted);
            font-size: 11px;
            font-family: var(--font-family-mono, monospace);
            flex: none;
          }

          .detail-title {
            overflow: hidden;
            white-space: nowrap;
            text-overflow: ellipsis;
            font-size: var(--font-size-base);
            font-weight: 700;
            color: var(--color-text);
          }
        }
      }

      .prompt-detail {
        display: flex;
        flex: 1;
        gap: 20px;
        min-height: 0;
        overflow-y: auto;
        padding-top: 14px;

        &::-webkit-scrollbar {
          width: 5px;
        }
        &::-webkit-scrollbar-thumb {
          background: var(--color-line);
          border-radius: 2px;
        }

        .detail-visual {
          display: flex;
          flex-direction: column;
          width: 42%;
          gap: 14px;
          flex: none;

          .detail-image-wrapper {
            position: relative;
            width: 100%;
            height: 300px;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            border-radius: 8px;
            overflow: hidden;

            .detail-image {
              width: 100%;
              height: 100%;
              display: block;
            }

            .corner-bracket {
              position: absolute;
              width: 8px;
              height: 8px;
              border-color: var(--color-primary);
              pointer-events: none;
              opacity: 0.8;

              &.top-left {
                top: 4px;
                left: 4px;
                border-top: 2px solid;
                border-left: 2px solid;
              }
              &.top-right {
                top: 4px;
                right: 4px;
                border-top: 2px solid;
                border-right: 2px solid;
              }
              &.bottom-left {
                bottom: 4px;
                left: 4px;
                border-bottom: 2px solid;
                border-left: 2px solid;
              }
              &.bottom-right {
                bottom: 4px;
                right: 4px;
                border-bottom: 2px solid;
                border-right: 2px solid;
              }
            }
          }

          .detail-tags-box {
            display: flex;
            flex-direction: column;
            gap: 6px;

            .tags-header {
              font-size: 11px;
              font-family: var(--font-family-mono, monospace);
              color: var(--color-text-soft);
            }

            .detail-tags {
              display: flex;
              flex-wrap: wrap;
              gap: 6px;

              .category-tag {
                padding: 3px 8px;
                background: var(--color-panel-soft);
                border: 1px solid var(--color-line);
                color: var(--color-text-muted);
                border-radius: 4px;
                font-size: var(--font-size-xs);
                font-family: var(--font-family-mono, monospace);
              }
            }
          }

          .source-link-button {
            display: inline-flex;
            align-items: center;
            gap: 6px;
            align-self: flex-start;
            padding: 4px 8px;
            border: 1px solid var(--color-line);
            border-radius: 5px;
            background: transparent;
            color: var(--color-primary);
            font-size: var(--font-size-xs);
            cursor: pointer;
            transition: all 0.15s ease;

            &:hover {
              background: var(--color-primary-soft);
              border-color: var(--color-primary);
            }
          }
        }

        .detail-editor {
          display: flex;
          flex: 1;
          min-width: 0;
          flex-direction: column;
          gap: 12px;

          .console-box {
            display: flex;
            flex-direction: column;
            flex: 1;
            min-height: 220px;
            border: 1px solid var(--color-line);
            border-radius: 8px;
            overflow: hidden;
            background: var(--color-panel-soft);

            .editor-heading {
              display: flex;
              justify-content: space-between;
              align-items: center;
              padding: 8px 12px;
              background: color-mix(
                in srgb,
                var(--color-panel) 85%,
                var(--color-line)
              );
              border-bottom: 1px solid var(--color-line);

              .console-title {
                display: flex;
                align-items: center;
                gap: 6px;
                color: var(--color-text-muted);
                font-family: var(--font-family-mono, monospace);
                font-size: var(--font-size-xs);
                font-weight: 600;

                .console-icon {
                  color: var(--color-primary);
                }
              }

              .editor-controls {
                display: flex;
                align-items: center;
                gap: 8px;

                .lang-switch-box {
                  display: flex;
                  align-items: center;
                  gap: 4px;

                  .lang-label {
                    font-size: 11px;
                    color: var(--color-text-soft);
                  }

                  .language-select {
                    padding: 2px 6px;
                    border: 1px solid var(--color-line);
                    border-radius: 4px;
                    background: var(--color-panel);
                    color: var(--color-text);
                    font-size: var(--font-size-xs);
                    outline: none;
                    cursor: pointer;

                    &:focus {
                      border-color: var(--color-primary);
                    }
                  }
                }

                .copy-prompt-btn {
                  display: inline-flex;
                  align-items: center;
                  gap: 4px;
                  padding: 3px 8px;
                  border: 1px solid var(--color-line);
                  border-radius: 4px;
                  background: var(--color-panel);
                  color: var(--color-text);
                  font-size: var(--font-size-xs);
                  cursor: pointer;
                  transition: all 0.15s ease;

                  &:hover {
                    border-color: var(--color-primary);
                    color: var(--color-primary);
                  }

                  &.copied {
                    border-color: #10b981;
                    color: #10b981;
                    background: color-mix(in srgb, #10b981 12%, transparent);
                  }
                }
              }
            }

            .textarea-wrapper {
              display: flex;
              flex-direction: column;
              flex: 1;
              position: relative;

              .prompt-text {
                flex: 1;
                min-height: 160px;
                resize: none;
                padding: 12px;
                border: 0;
                outline: none;
                background: transparent;
                color: var(--color-text);
                font-family: inherit;
                font-size: var(--font-size-sm);
                line-height: 1.65;
              }

              .prompt-metrics-bar {
                display: flex;
                justify-content: flex-end;
                padding: 4px 10px 8px;
                font-family: var(--font-family-mono, monospace);
                font-size: 11px;
                color: var(--color-text-soft);

                .warning-limit {
                  color: var(--color-danger);
                  font-weight: 600;
                }
              }
            }
          }

          .tech-hint-card {
            display: flex;
            align-items: flex-start;
            gap: 8px;
            padding: 8px 12px;
            border-radius: 6px;
            background: color-mix(
              in srgb,
              var(--color-primary-soft) 70%,
              transparent
            );
            border: 1px solid
              color-mix(in srgb, var(--color-primary) 30%, transparent);
            color: var(--color-text-muted);
            font-size: var(--font-size-xs);
            line-height: 1.5;

            .hint-icon {
              color: var(--color-primary);
              flex: none;
              margin-top: 1px;
            }

            &.edit-hint-card {
              background: color-mix(in srgb, #f59e0b 8%, var(--color-panel));
              border-color: color-mix(in srgb, #f59e0b 28%, transparent);

              .hint-icon {
                color: #f59e0b;
              }
            }
          }

          .template-meta-strip {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 8px;
            padding: 8px 12px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel);

            .meta-strip-item {
              display: flex;
              align-items: center;
              gap: 6px;
              font-size: var(--font-size-xs);
              color: var(--color-text-muted);

              .meta-strip-label {
                color: var(--color-text-soft);
              }

              .meta-strip-value {
                font-family: var(--font-family-mono, monospace);
                font-weight: 600;
                color: var(--color-text);
              }
            }
          }

          .detail-error {
            margin: 0;
            color: var(--color-danger);
            font-size: var(--font-size-xs);
          }

          .detail-action-bar {
            margin-top: 4px;

            .use-button {
              display: flex;
              align-items: center;
              justify-content: center;
              gap: 8px;
              width: 100%;
              height: 40px;
              border: 0;
              border-radius: 8px;
              background: linear-gradient(
                135deg,
                var(--color-primary) 0%,
                color-mix(in srgb, var(--color-primary) 80%, #000) 100%
              );
              box-shadow: 0 4px 14px
                color-mix(in srgb, var(--color-primary) 35%, transparent);
              color: #ffffff;
              font-weight: 600;
              font-size: var(--font-size-sm);
              cursor: pointer;
              transition: all 0.2s ease;

              &:hover:not(:disabled) {
                transform: translateY(-1px);
                box-shadow: 0 6px 18px
                  color-mix(in srgb, var(--color-primary) 45%, transparent);
              }

              &:disabled {
                opacity: 0.5;
                cursor: not-allowed;
                box-shadow: none;
              }
            }
          }
        }
      }
    }
  }

  // --- Image Placeholder ---
  .image-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    width: 100%;
    height: 100%;
    color: var(--color-text-soft);
    font-size: var(--font-size-xs);

    &.error-placeholder {
      opacity: 0.75;
    }
  }

  // --- Attribution Footer ---
  .library-attribution {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 6px;
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--color-line);
    color: var(--color-text-soft);
    font-size: var(--font-size-xs);

    .attribution-label {
      font-family: var(--font-family-mono, monospace);
    }

    .attribution-link {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      color: var(--color-primary);
      text-decoration: none;
      font-weight: 500;

      &:hover {
        text-decoration: underline;
      }
    }
  }
}

// --- Keyframes ---
@keyframes tech-spin {
  to {
    transform: rotate(360deg);
  }
}

@keyframes tech-pulse {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.4;
    transform: scale(0.85);
  }
}
</style>
