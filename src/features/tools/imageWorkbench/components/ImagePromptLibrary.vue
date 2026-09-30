<template>
  <section class="image-prompt-library">
    <BaseModal @close="$emit('close')">
      <!-- 弹窗顶栏：左侧图标徽章、标题与副标题；右侧关闭按钮 -->
      <template #header>
        <div class="library-modal-header">
          <div class="header-left">
            <div class="header-icon-box">
              <Layers :size="20" />
            </div>
            <div class="header-info">
              <h2 class="header-title">提示词库</h2>
              <p class="header-desc">
                按分类发现画面灵感，{{ catalog.count || 0 }} 条社区提示词
              </p>
            </div>
          </div>
          <button
            class="header-close-btn"
            type="button"
            title="关闭"
            aria-label="关闭"
            @click="$emit('close')"
          >
            <X :size="15" />
          </button>
        </div>
      </template>

      <!-- 检索与工具栏：搜索框、状态胶囊、文件源与导入按钮 -->
      <div class="library-toolbar">
        <div class="search-box">
          <Search :size="14" class="search-icon" />
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
            <X :size="12" />
          </button>
        </div>

        <div class="library-meta-actions">
          <div
            class="meta-pill library-count-pill"
            :title="`收录文件: ${catalog.filename || '未知'}`"
          >
            <span class="status-dot"></span>
            <Database :size="13" class="pill-icon" />
            <span>{{ catalog.count || 0 }} 灵感库</span>
          </div>

          <div
            v-if="catalog.filename"
            class="meta-pill filename-pill"
            :title="catalog.filename"
          >
            <Code2 :size="13" class="pill-icon" />
            <span class="filename-text">{{ catalog.filename }}</span>
          </div>

          <button
            class="import-json-btn"
            type="button"
            :disabled="initializing || importing"
            title="导入自定义 JSON 提示词库"
            @click="importLibrary"
          >
            <Upload :size="14" :class="{ 'spinning-icon': importing }" />
            <span>{{ importing ? "导入中…" : "导入 JSON" }}</span>
          </button>
        </div>
      </div>

      <p v-if="error" class="library-error" role="alert">
        <span>{{ error }}</span>
        <button class="retry-button" type="button" @click="initialize">
          重试
        </button>
      </p>

      <!-- 加载提示 -->
      <div v-if="initializing" class="library-loading">
        <div class="loading-icon-box">
          <LoaderCircle class="spinning" :size="26" />
        </div>
        <span>正在加载本地提示词库，首次打开需要建立索引…</span>
      </div>

      <!-- 主体区域：左侧分类导航 + 右侧列表/详情 -->
      <div v-else class="library-body">
        <!-- 左侧分类侧边栏 -->
        <nav class="category-sidebar" aria-label="提示词分类导航">
          <header class="sidebar-header">
            <AlignLeft :size="14" class="sidebar-head-icon" />
            <span class="sidebar-title">分类导航</span>
          </header>

          <div class="category-scroll-container">
            <!-- 全部提示词 (始终置顶) -->
            <button
              class="category-item-btn"
              :class="{ active: !category }"
              type="button"
              @click="selectCategory('')"
            >
              <div class="cat-left">
                <LayoutGrid :size="16" class="cat-icon" />
                <span class="category-name">全部提示词</span>
              </div>
              <span class="category-badge">{{ catalog.count || 0 }}</span>
            </button>

            <!-- 分类项列表 -->
            <button
              v-for="item in catalog.categories"
              :key="item.id"
              class="category-item-btn"
              :class="{ active: category === item.id }"
              type="button"
              :title="item.originalTitle || item.title"
              @click="selectCategory(item.id)"
            >
              <div class="cat-left">
                <component
                  :is="getCategoryIcon(item.title)"
                  :size="16"
                  class="cat-icon"
                />
                <span class="category-name">{{ item.title }}</span>
              </div>
              <span class="category-badge">{{ item.count }}</span>
            </button>
          </div>
        </nav>

        <!-- 右侧主内容区 -->
        <section class="library-main">
          <!-- 模式一：图库卡片列表 -->
          <template v-if="!detail">
            <header class="gallery-heading">
              <div class="heading-left">
                <span class="heading-title">{{ activeCategory }}</span>
                <span class="match-count-pill">
                  <span class="dot-indicator"></span>
                  <span>{{ total }} 条匹配</span>
                </span>
              </div>
              <div class="heading-right">
                <span v-if="loading" class="loading-label">
                  <LoaderCircle :size="12" class="spinning" />
                  <span>加载中…</span>
                </span>
                <span class="page-quick-tag">
                  <FileText :size="12" class="tag-file-icon" />
                  <span>
                    PAGE {{ String(page).padStart(2, "0") }} /
                    {{ String(totalPages).padStart(2, "0") }}
                  </span>
                </span>
              </div>
            </header>

            <!-- 卡片网格滚动区 -->
            <div
              ref="galleryScrollRef"
              class="prompt-gallery-scroll"
              :aria-busy="loading"
              @scroll.passive="onGalleryScroll"
            >
              <div v-if="!items.length && !loading" class="library-empty">
                <div class="empty-icon-box">
                  <Search :size="28" />
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
                <article
                  v-for="item in items"
                  :key="item.id"
                  class="prompt-card"
                  role="button"
                  tabindex="0"
                  :aria-disabled="detailLoading || importing"
                  @click="openDetail(item.id)"
                  @keydown.enter="openDetail(item.id)"
                >
                  <!-- 顶部封面图与模式角标 -->
                  <div class="card-image-box">
                    <el-image
                      v-if="item.previewImageUrl || item.highQualityImageUrl"
                      class="preview-img"
                      :src="item.previewImageUrl || item.highQualityImageUrl"
                      fit="cover"
                      lazy
                      scroll-container=".prompt-gallery-scroll"
                      referrerpolicy="no-referrer"
                    >
                      <template #placeholder>
                        <div class="card-placeholder-box">
                          <LoaderCircle class="spinning" :size="20" />
                        </div>
                      </template>
                      <template #error>
                        <div class="card-placeholder-box error-box">
                          <ImageIcon :size="24" class="soft-icon" />
                          <span class="placeholder-text">预览图暂不可用</span>
                        </div>
                      </template>
                    </el-image>

                    <!-- 无图时的默认质感占位 -->
                    <div v-else class="card-placeholder-box default-box">
                      <ImageIcon :size="28" class="soft-icon" />
                    </div>

                    <!-- 模式标签胶囊 (如: 文生图 / 图片编辑) -->
                    <div class="card-badge-tag">
                      <span
                        class="mode-pill"
                        :class="{
                          'mode-pill-edit': isEditMode(item.inputMode)
                        }"
                      >
                        <span class="badge-dot"></span>
                        <span>{{ modeLabel(item.inputMode) }}</span>
                      </span>
                    </div>
                  </div>

                  <!-- 底部标题与标签信息 -->
                  <div class="card-content">
                    <h3 class="card-title" :title="item.title">
                      {{ item.title }}
                    </h3>
                    <div class="card-footer-row">
                      <Tag :size="12" class="tag-icon" />
                      <span class="card-tag-name">
                        ·
                        {{
                          item.tags?.[0] || categoryTitle(item.categoryIds?.[0])
                        }}
                      </span>
                    </div>
                  </div>
                </article>
              </div>

              <!-- 回到顶部悬浮按钮 -->
              <button
                v-if="showBackToTop"
                class="floating-back-to-top"
                type="button"
                title="回到顶部"
                @click="scrollToTop('smooth')"
              >
                <ArrowUp :size="13" />
                <span>顶部</span>
              </button>
            </div>

            <!-- 底部来源注明与翻页器 -->
            <footer class="gallery-footer">
              <div class="footer-left">
                <BookOpen :size="14" class="footer-book-icon" />
                <span class="footer-hint-text">提示词整理自社区项目 ·</span>
                <a
                  class="footer-source-link"
                  :href="sourceRepositoryUrl"
                  target="_blank"
                  rel="noopener noreferrer"
                  @click.prevent="openSource(sourceRepositoryUrl)"
                >
                  <span>Toolcentral-ai / Image Prompt Gallery</span>
                  <ExternalLink :size="11" />
                </a>
              </div>

              <div class="footer-right">
                <button
                  class="page-nav-btn"
                  type="button"
                  title="上一页"
                  :disabled="page <= 1 || loading"
                  @click="changePage(-1)"
                >
                  <ChevronLeft :size="14" />
                </button>
                <div class="page-indicator-box">
                  <span class="curr-page">{{
                    String(page).padStart(2, "0")
                  }}</span>
                  <span class="sep">/</span>
                  <span class="total-page">{{
                    String(totalPages).padStart(2, "0")
                  }}</span>
                </div>
                <button
                  class="page-nav-btn"
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

          <!-- 模式二：详情与快速填入视图 -->
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
                </div>

                <div class="detail-tags-box">
                  <div class="tags-header">标签分类</div>
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
                      <span>提示词内容</span>
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
                  <span>此模板含可替换内容，可直接在上方修改占位参数。</span>
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
                  <ImageIcon :size="14" class="hint-icon" />
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
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  AlignLeft,
  ArrowLeft,
  ArrowUp,
  BookOpen,
  Box,
  Briefcase,
  Building,
  Camera,
  Check,
  ChevronLeft,
  ChevronRight,
  Code2,
  Copy,
  Cpu,
  Database,
  ExternalLink,
  FileText,
  Flame,
  Gamepad2,
  Gem,
  Home,
  Image as ImageIcon,
  ImageOff,
  Layers,
  LayoutGrid,
  LoaderCircle,
  Mountain,
  Palette,
  Search,
  Smile,
  Sparkles,
  Tag,
  Terminal,
  Trees,
  Upload,
  User,
  Utensils,
  Wand2,
  Waves,
  X
} from "lucide-vue-next"
import { ElImage, ElMessageBox } from "element-plus"
import "element-plus/es/components/image/style/css"
import "element-plus/es/components/message-box/style/css"
import BaseModal from "@/components/BaseModal.vue"
import { systemApi, toolboxApi } from "@/api"
import { createMessage } from "@/utils/message"

const emit = defineEmits(["close", "select"])

// 提示词整理来源
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

// 根据分类名称语义动态返回适配的图标
function getCategoryIcon(title = "") {
  const t = String(title).toLowerCase()
  if (
    t.includes("动漫") ||
    t.includes("游戏") ||
    t.includes("anime") ||
    t.includes("game")
  )
    return Gamepad2
  if (
    t.includes("时尚") ||
    t.includes("美妆") ||
    t.includes("fashion") ||
    t.includes("beauty")
  )
    return Gem
  if (
    t.includes("食物") ||
    t.includes("饮品") ||
    t.includes("food") ||
    t.includes("drink") ||
    t.includes("beverage")
  )
    return Utensils
  if (
    t.includes("海报") ||
    t.includes("平面") ||
    t.includes("poster") ||
    t.includes("banner")
  )
    return ImageIcon
  if (
    t.includes("插画") ||
    t.includes("卡通") ||
    t.includes("绘本") ||
    t.includes("illustration") ||
    t.includes("cartoon")
  )
    return Smile
  if (
    t.includes("角色") ||
    t.includes("人物") ||
    t.includes("肖像") ||
    t.includes("character") ||
    t.includes("portrait") ||
    t.includes("person")
  )
    return User
  if (
    t.includes("摄影") ||
    t.includes("写实") ||
    t.includes("photo") ||
    t.includes("realistic")
  )
    return Camera
  if (t.includes("3d") || t.includes("渲染") || t.includes("render")) return Box
  if (
    t.includes("家居") ||
    t.includes("室内") ||
    t.includes("home") ||
    t.includes("interior") ||
    t.includes("room")
  )
    return Home
  if (
    t.includes("建筑") ||
    t.includes("空间") ||
    t.includes("城市") ||
    t.includes("architecture") ||
    t.includes("building")
  )
    return Building
  if (
    t.includes("商业") ||
    t.includes("电商") ||
    t.includes("产品") ||
    t.includes("product") ||
    t.includes("commerce") ||
    t.includes("business")
  )
    return Briefcase
  if (
    t.includes("森林") ||
    t.includes("植物") ||
    t.includes("树木") ||
    t.includes("forest") ||
    t.includes("tree") ||
    t.includes("plant")
  )
    return Trees
  if (
    t.includes("海洋") ||
    t.includes("沙滩") ||
    t.includes("水") ||
    t.includes("ocean") ||
    t.includes("sea") ||
    t.includes("beach") ||
    t.includes("wave") ||
    t.includes("water")
  )
    return Waves
  if (
    t.includes("奇幻") ||
    t.includes("魔法") ||
    t.includes("fantasy") ||
    t.includes("magic")
  )
    return Wand2
  if (
    t.includes("科幻") ||
    t.includes("赛博") ||
    t.includes("科技") ||
    t.includes("sci-fi") ||
    t.includes("cyberpunk") ||
    t.includes("tech")
  )
    return Flame
  if (
    t.includes("艺术") ||
    t.includes("绘画") ||
    t.includes("art") ||
    t.includes("painting")
  )
    return Palette
  if (
    t.includes("风景") ||
    t.includes("自然") ||
    t.includes("landscape") ||
    t.includes("nature")
  )
    return Mountain
  if (t.includes("general") || t.includes("通用")) return FileText
  return Tag
}

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
    width: min(990px, calc(100vw - 36px));
    height: min(720px, calc(100vh - 44px));
    max-height: 740px;
    border-radius: 16px;
    border: 1px solid var(--color-line);
    background: var(--color-panel);
    box-shadow:
      0 20px 48px rgba(0, 0, 0, 0.12),
      0 1px 3px rgba(0, 0, 0, 0.04);
  }

  :deep(.base-modal__header) {
    padding: 16px 20px 12px;
    border-bottom: 0;
    background: transparent;
    display: block;
  }

  :deep(.base-modal__content) {
    padding: 0 20px 18px;
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }

  /* 弹窗头部：精致图标、不夸张的标题 */
  .library-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;

    .header-left {
      display: flex;
      align-items: center;
      gap: 12px;

      .header-icon-box {
        display: grid;
        width: 40px;
        height: 40px;
        place-items: center;
        border-radius: 10px;
        background: #eff6ff;
        color: #2563eb;
        flex-shrink: 0;
      }

      .header-info {
        display: flex;
        flex-direction: column;
        gap: 2px;

        .header-title {
          margin: 0;
          font-size: 16px;
          font-weight: 700;
          color: var(--color-text);
          letter-spacing: 0.1px;
          line-height: 1.3;
        }

        .header-desc {
          margin: 0;
          font-size: 12px;
          color: var(--color-text-muted);
          line-height: 1.4;
        }
      }
    }

    .header-close-btn {
      display: grid;
      width: 32px;
      height: 32px;
      place-items: center;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-text-muted);
      cursor: pointer;
      transition: all 0.2s;

      &:hover {
        border-color: var(--color-line-strong);
        color: var(--color-text);
        background: var(--color-panel-soft);
      }
    }
  }

  /* 检索与操作工具栏 */
  .library-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 12px;
    flex: none;

    .search-box {
      display: flex;
      flex: 1;
      max-width: 380px;
      align-items: center;
      gap: 8px;
      padding: 0 12px;
      height: 34px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      transition: all 0.2s ease;

      &:focus-within {
        border-color: #2563eb;
        background: var(--color-panel);
        box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.12);
      }

      .search-icon {
        color: var(--color-text-muted);
        flex: none;
      }

      .search-input {
        flex: 1;
        min-width: 0;
        border: 0;
        outline: none;
        color: var(--color-text);
        background: transparent;
        font-size: 12.5px;

        &::placeholder {
          color: var(--color-text-soft);
        }
      }

      .clear-search-btn {
        display: grid;
        place-items: center;
        width: 18px;
        height: 18px;
        padding: 0;
        border: 0;
        border-radius: 4px;
        background: var(--color-line);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.15s;

        &:hover {
          color: var(--color-text);
          background: var(--color-line-strong);
        }
      }
    }

    .library-meta-actions {
      display: flex;
      align-items: center;
      gap: 8px;
      flex-shrink: 0;

      .meta-pill {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        height: 32px;
        padding: 0 12px;
        border: 1px solid var(--color-line);
        background: var(--color-panel);
        color: var(--color-text);
        font-size: 12px;
        font-family:
          ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        white-space: nowrap;
        flex-shrink: 0;

        &.library-count-pill {
          border-radius: 9999px;

          .status-dot {
            width: 7px;
            height: 7px;
            border-radius: 50%;
            background: #10b981;
            flex-shrink: 0;
          }

          .pill-icon {
            color: var(--color-text-muted);
          }
        }

        &.filename-pill {
          border-radius: 8px;
          color: var(--color-text);

          .pill-icon {
            color: #2563eb;
          }

          .filename-text {
            max-width: 180px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }
        }
      }

      .import-json-btn {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        height: 32px;
        padding: 0 14px;
        border-radius: 8px;
        border: 1px solid #bfdbfe;
        background: #eff6ff;
        color: #2563eb;
        font-size: 12.5px;
        font-weight: 600;
        cursor: pointer;
        white-space: nowrap;
        flex-shrink: 0;
        transition: all 0.2s ease;

        &:hover:not(:disabled) {
          background: #dbeafe;
          border-color: #93c5fd;
        }

        &:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        .spinning-icon {
          animation: library-spin 1.4s linear infinite;
        }
      }
    }
  }

  /* 错误与加载 */
  .library-error {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 10px;
    padding: 8px 12px;
    border: 1px solid #fecaca;
    border-radius: 8px;
    background: #fef2f2;
    color: #b91c1c;
    font-size: 12px;

    .retry-button {
      border: 0;
      background: transparent;
      color: #2563eb;
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
    gap: 12px;
    color: var(--color-text-muted);
    font-size: 13px;

    .loading-icon-box {
      color: #2563eb;
    }
  }

  /* 主体卡片边框容器 (带圆角和内外边框) */
  .library-body {
    display: flex;
    flex: 1;
    min-height: 0;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    overflow: hidden;

    /* 左侧分类导航 */
    .category-sidebar {
      display: flex;
      flex-direction: column;
      flex: 0 0 220px;
      border-right: 1px solid var(--color-line);
      min-height: 0;
      background: var(--color-panel);

      .sidebar-header {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 14px 16px 12px;
        color: var(--color-text);
        font-size: 13px;
        font-weight: 600;
        border-bottom: 1px solid var(--color-line);

        .sidebar-head-icon {
          color: var(--color-text-muted);
        }

        .sidebar-title {
          font-weight: 600;
        }
      }

      .category-scroll-container {
        flex: 1;
        overflow-y: auto;
        min-height: 0;
        padding: 10px;
        display: flex;
        flex-direction: column;
        gap: 6px;

        &::-webkit-scrollbar {
          width: 4px;
        }
        &::-webkit-scrollbar-thumb {
          background: var(--color-line);
          border-radius: 2px;
        }
      }

      .category-item-btn {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
        width: 100%;
        height: 40px;
        padding: 0 12px;
        border: 1px solid transparent;
        border-radius: 8px;
        text-align: left;
        background: transparent;
        color: var(--color-text);
        font-size: 13px;
        cursor: pointer;
        transition: all 0.16s ease;

        .cat-left {
          display: flex;
          align-items: center;
          gap: 9px;
          min-width: 0;

          .cat-icon {
            flex-shrink: 0;
            color: var(--color-text-muted);
            transition: color 0.16s;
          }

          .category-name {
            overflow: hidden;
            white-space: nowrap;
            text-overflow: ellipsis;
            font-weight: 500;
          }
        }

        .category-badge {
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 11.5px;
          font-weight: 500;
          padding: 1px 7px;
          border-radius: 9999px;
          background: var(--color-panel-soft);
          color: var(--color-text-muted);
          transition: all 0.16s;
          flex-shrink: 0;
        }

        &:hover {
          background: var(--color-panel-soft);
          border-color: var(--color-line);
          color: var(--color-text);

          .cat-left .cat-icon {
            color: var(--color-text);
          }
        }

        &.active {
          background: #eff6ff;
          border-color: #bfdbfe;
          color: #2563eb;
          font-weight: 600;

          .cat-left .cat-icon {
            color: #2563eb;
          }

          .category-badge {
            background: #dbeafe;
            color: #1d4ed8;
            font-weight: 600;
          }
        }
      }
    }

    /* 右侧展示区 */
    .library-main {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-width: 0;
      min-height: 0;
      background: var(--color-panel);

      .gallery-heading {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 12px 18px 10px;
        border-bottom: 1px solid var(--color-line);
        flex: none;

        .heading-left {
          display: flex;
          align-items: center;
          gap: 10px;

          .heading-title {
            font-size: 14.5px;
            font-weight: 700;
            color: var(--color-text);
          }

          .match-count-pill {
            display: inline-flex;
            align-items: center;
            gap: 6px;
            padding: 2px 8px;
            border-radius: 9999px;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            font-size: 11.5px;
            color: var(--color-text-muted);

            .dot-indicator {
              width: 6px;
              height: 6px;
              border-radius: 50%;
              background: #2563eb;
            }
          }
        }

        .heading-right {
          display: flex;
          align-items: center;
          gap: 8px;

          .loading-label {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            color: #2563eb;
            font-size: 11.5px;
          }

          .page-quick-tag {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 11px;
            color: var(--color-text-muted);
            background: var(--color-panel-soft);
            padding: 2px 8px;
            border-radius: 5px;
            border: 1px solid var(--color-line);

            .tag-file-icon {
              color: var(--color-text-soft);
            }
          }
        }
      }

      /* 图库卡片列表滚动容器 */
      .prompt-gallery-scroll {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: 14px 18px;
        position: relative;

        &::-webkit-scrollbar {
          width: 5px;
        }
        &::-webkit-scrollbar-thumb {
          background: var(--color-line);
          border-radius: 3px;
        }

        .library-empty {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: 10px;
          height: 100%;
          min-height: 240px;
          color: var(--color-text-muted);

          .empty-icon-box {
            display: grid;
            width: 52px;
            height: 52px;
            place-items: center;
            border-radius: 50%;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            color: var(--color-text-soft);
          }

          .empty-title {
            font-size: 14px;
            font-weight: 600;
            color: var(--color-text);
          }

          .empty-hint {
            color: var(--color-text-muted);
            font-size: 12px;
          }

          .empty-reset-button {
            margin-top: 4px;
            padding: 5px 12px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel);
            color: #2563eb;
            font-size: 12px;
            cursor: pointer;
            transition: all 0.15s ease;

            &:hover {
              border-color: #2563eb;
              background: #eff6ff;
            }
          }
        }

        /* 双列网格 (精准贴合截图比例) */
        .prompt-gallery {
          display: grid;
          grid-template-columns: repeat(2, minmax(0, 1fr));
          gap: 14px;

          @media (max-width: 680px) {
            grid-template-columns: 1fr;
          }

          .prompt-card {
            display: flex;
            flex-direction: column;
            border: 1px solid var(--color-line);
            border-radius: 12px;
            background: var(--color-panel);
            overflow: hidden;
            cursor: pointer;
            transition: all 0.2s ease;

            &:hover {
              border-color: #bfdbfe;
              box-shadow: 0 6px 18px rgba(37, 99, 235, 0.08);
              transform: translateY(-2px);
            }

            .card-image-box {
              position: relative;
              width: 100%;
              height: 160px;
              background: #f1f5f9;
              overflow: hidden;

              .preview-img {
                width: 100%;
                height: 100%;
                display: block;
              }

              .card-placeholder-box {
                width: 100%;
                height: 100%;
                display: flex;
                flex-direction: column;
                align-items: center;
                justify-content: center;
                gap: 6px;
                background: linear-gradient(135deg, #f8fafc 0%, #e2e8f0 100%);
                color: #94a3b8;

                &.error-box {
                  .placeholder-text {
                    font-size: 11px;
                    color: #94a3b8;
                  }
                }
              }

              .card-badge-tag {
                position: absolute;
                bottom: 8px;
                left: 8px;
                z-index: 1;

                .mode-pill {
                  display: inline-flex;
                  align-items: center;
                  gap: 5px;
                  padding: 2px 8px;
                  border-radius: 6px;
                  background: rgba(15, 23, 42, 0.72);
                  backdrop-filter: blur(4px);
                  color: #ffffff;
                  font-size: 11px;
                  font-weight: 500;
                  line-height: 1.3;

                  .badge-dot {
                    width: 5px;
                    height: 5px;
                    border-radius: 50%;
                    background: #3b82f6;
                  }

                  &.mode-pill-edit .badge-dot {
                    background: #f59e0b;
                  }
                }
              }
            }

            .card-content {
              display: flex;
              flex-direction: column;
              gap: 6px;
              padding: 12px 14px 14px;
              background: var(--color-panel);

              .card-title {
                margin: 0;
                font-size: 13.5px;
                font-weight: 700;
                color: var(--color-text);
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
                line-height: 1.4;
              }

              .card-footer-row {
                display: flex;
                align-items: center;
                gap: 5px;
                color: var(--color-text-muted);
                font-size: 11.5px;

                .tag-icon {
                  color: var(--color-text-soft);
                  flex-shrink: 0;
                }

                .card-tag-name {
                  overflow: hidden;
                  text-overflow: ellipsis;
                  white-space: nowrap;
                }
              }
            }
          }
        }

        .floating-back-to-top {
          position: sticky;
          bottom: 12px;
          left: 100%;
          margin-right: 12px;
          display: inline-flex;
          align-items: center;
          gap: 4px;
          padding: 5px 10px;
          border-radius: 20px;
          border: 1px solid #bfdbfe;
          background: #eff6ff;
          color: #2563eb;
          box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
          font-size: 11.5px;
          font-weight: 600;
          cursor: pointer;
          transition: all 0.18s ease;
          z-index: 10;

          &:hover {
            transform: translateY(-2px);
            background: #2563eb;
            color: #ffffff;
            border-color: #2563eb;
          }
        }
      }

      /* 底部页脚：开源标注 + 翻页控件 */
      .gallery-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        padding: 10px 18px;
        border-top: 1px solid var(--color-line);
        background: var(--color-panel);
        flex: none;

        .footer-left {
          display: flex;
          align-items: center;
          gap: 6px;
          font-size: 11.5px;
          color: var(--color-text-muted);

          .footer-book-icon {
            color: var(--color-text-muted);
            flex-shrink: 0;
          }

          .footer-source-link {
            display: inline-flex;
            align-items: center;
            gap: 3px;
            color: var(--color-text-muted);
            text-decoration: none;
            transition: color 0.15s;

            &:hover {
              color: #2563eb;
              text-decoration: underline;
            }
          }
        }

        .footer-right {
          display: flex;
          align-items: center;
          gap: 6px;

          .page-nav-btn {
            display: grid;
            place-items: center;
            width: 28px;
            height: 28px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel);
            color: var(--color-text);
            cursor: pointer;
            transition: all 0.15s ease;

            &:hover:not(:disabled) {
              border-color: #2563eb;
              color: #2563eb;
              background: #eff6ff;
            }

            &:disabled {
              opacity: 0.4;
              cursor: not-allowed;
            }
          }

          .page-indicator-box {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            padding: 0 10px;
            height: 28px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel-soft);
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 11.5px;

            .curr-page {
              color: #2563eb;
              font-weight: 700;
            }

            .sep {
              color: var(--color-text-soft);
            }

            .total-page {
              color: var(--color-text-muted);
            }
          }
        }
      }

      /* 详情页模式 */
      .detail-heading {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 12px 18px;
        border-bottom: 1px solid var(--color-line);
        flex: none;

        .back-button {
          display: inline-flex;
          align-items: center;
          gap: 5px;
          padding: 5px 11px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel-soft);
          color: var(--color-text);
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.15s ease;
          flex-shrink: 0;

          &:hover {
            border-color: #2563eb;
            color: #2563eb;
            background: #eff6ff;
          }
        }

        .detail-heading-info {
          display: flex;
          align-items: center;
          gap: 8px;
          min-width: 0;

          .detail-category-badge {
            padding: 2px 7px;
            border-radius: 4px;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            color: var(--color-text-muted);
            font-size: 11px;
            flex-shrink: 0;
          }

          .detail-title {
            overflow: hidden;
            white-space: nowrap;
            text-overflow: ellipsis;
            font-size: 14px;
            font-weight: 700;
            color: var(--color-text);
          }
        }
      }

      .prompt-detail {
        display: flex;
        flex: 1;
        gap: 18px;
        min-height: 0;
        overflow-y: auto;
        padding: 16px 18px;

        .detail-visual {
          display: flex;
          flex-direction: column;
          width: 40%;
          gap: 12px;
          flex-shrink: 0;

          .detail-image-wrapper {
            position: relative;
            width: 100%;
            height: 260px;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            border-radius: 8px;
            overflow: hidden;

            .detail-image {
              width: 100%;
              height: 100%;
              display: block;
            }
          }

          .detail-tags-box {
            display: flex;
            flex-direction: column;
            gap: 6px;

            .tags-header {
              font-size: 11px;
              color: var(--color-text-muted);
              font-weight: 500;
            }

            .detail-tags {
              display: flex;
              flex-wrap: wrap;
              gap: 6px;

              .category-tag {
                padding: 2px 7px;
                background: var(--color-panel-soft);
                border: 1px solid var(--color-line);
                color: var(--color-text-muted);
                border-radius: 4px;
                font-size: 11px;
              }
            }
          }

          .source-link-button {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            align-self: flex-start;
            padding: 4px 8px;
            border: 1px solid var(--color-line);
            border-radius: 5px;
            background: transparent;
            color: #2563eb;
            font-size: 11.5px;
            cursor: pointer;
            transition: all 0.15s ease;

            &:hover {
              background: #eff6ff;
              border-color: #bfdbfe;
            }
          }
        }

        .detail-editor {
          display: flex;
          flex: 1;
          min-width: 0;
          flex-direction: column;
          gap: 10px;

          .console-box {
            display: flex;
            flex-direction: column;
            flex: 1;
            min-height: 200px;
            border: 1px solid var(--color-line);
            border-radius: 8px;
            overflow: hidden;
            background: var(--color-panel-soft);

            .editor-heading {
              display: flex;
              justify-content: space-between;
              align-items: center;
              padding: 8px 12px;
              background: var(--color-panel);
              border-bottom: 1px solid var(--color-line);

              .console-title {
                display: flex;
                align-items: center;
                gap: 6px;
                color: var(--color-text);
                font-size: 12px;
                font-weight: 600;

                .console-icon {
                  color: #2563eb;
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
                    color: var(--color-text-muted);
                  }

                  .language-select {
                    padding: 2px 6px;
                    border: 1px solid var(--color-line);
                    border-radius: 4px;
                    background: var(--color-panel-soft);
                    color: var(--color-text);
                    font-size: 11px;
                    outline: none;
                    cursor: pointer;
                  }
                }

                .copy-prompt-btn {
                  display: inline-flex;
                  align-items: center;
                  gap: 4px;
                  padding: 3px 8px;
                  border: 1px solid var(--color-line);
                  border-radius: 4px;
                  background: var(--color-panel-soft);
                  color: var(--color-text);
                  font-size: 11px;
                  cursor: pointer;
                  transition: all 0.15s ease;

                  &:hover {
                    border-color: #2563eb;
                    color: #2563eb;
                  }

                  &.copied {
                    border-color: #10b981;
                    color: #10b981;
                    background: rgba(16, 185, 129, 0.1);
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
                min-height: 140px;
                resize: none;
                padding: 10px 12px;
                border: 0;
                outline: none;
                background: transparent;
                color: var(--color-text);
                font-family: inherit;
                font-size: 12.5px;
                line-height: 1.6;
              }

              .prompt-metrics-bar {
                display: flex;
                justify-content: flex-end;
                padding: 4px 10px 6px;
                font-family:
                  ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                  monospace;
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
            gap: 7px;
            padding: 7px 10px;
            border-radius: 6px;
            background: #eff6ff;
            border: 1px solid #bfdbfe;
            color: #1e40af;
            font-size: 11.5px;
            line-height: 1.4;

            .hint-icon {
              color: #2563eb;
              flex-shrink: 0;
              margin-top: 1px;
            }

            &.edit-hint-card {
              background: #fffbeb;
              border-color: #fde68a;
              color: #b45309;

              .hint-icon {
                color: #d97706;
              }
            }
          }

          .template-meta-strip {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 8px;
            padding: 7px 10px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel-soft);

            .meta-strip-item {
              display: flex;
              align-items: center;
              gap: 6px;
              font-size: 11.5px;
              color: var(--color-text-muted);

              .meta-strip-label {
                color: var(--color-text-soft);
              }

              .meta-strip-value {
                font-family:
                  ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                  monospace;
                font-weight: 600;
                color: var(--color-text);
              }
            }
          }

          .detail-error {
            margin: 0;
            color: var(--color-danger);
            font-size: 11.5px;
          }

          .detail-action-bar {
            margin-top: 2px;

            .use-button {
              display: flex;
              align-items: center;
              justify-content: center;
              gap: 7px;
              width: 100%;
              height: 38px;
              border: 0;
              border-radius: 8px;
              background: #2563eb;
              color: #ffffff;
              font-weight: 600;
              font-size: 13px;
              cursor: pointer;
              transition: all 0.2s ease;

              &:hover:not(:disabled) {
                background: #1d4ed8;
              }

              &:disabled {
                opacity: 0.5;
                cursor: not-allowed;
              }
            }
          }
        }
      }
    }
  }

  /* 图片加载占位 */
  .image-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    height: 100%;
    color: var(--color-text-soft);
    font-size: 11px;
  }

  /* 暗色模式适配 */
  :global(:root[data-theme="dark"]),
  .tools-view--dark & {
    :deep(.base-modal__panel) {
      box-shadow:
        0 20px 50px rgba(0, 0, 0, 0.5),
        0 0 0 1px rgba(255, 255, 255, 0.08);
    }

    .library-modal-header {
      .header-left .header-icon-box {
        background: rgba(37, 99, 235, 0.16);
        color: #60a5fa;
      }
    }

    .library-toolbar .library-meta-actions {
      .meta-pill {
        background: var(--color-panel-soft);
        border-color: var(--color-line);

        &.filename-pill .pill-icon {
          color: #60a5fa;
        }
      }

      .import-json-btn {
        background: rgba(37, 99, 235, 0.16);
        border-color: rgba(96, 165, 250, 0.35);
        color: #60a5fa;

        &:hover:not(:disabled) {
          background: rgba(37, 99, 235, 0.24);
          border-color: #60a5fa;
        }
      }
    }

    .library-body {
      .category-sidebar .category-item-btn {
        &:hover {
          background: rgba(255, 255, 255, 0.05);
          border-color: rgba(255, 255, 255, 0.1);
        }

        &.active {
          background: rgba(37, 99, 235, 0.16);
          border-color: rgba(96, 165, 250, 0.35);
          color: #60a5fa;

          .cat-left .cat-icon {
            color: #60a5fa;
          }

          .category-badge {
            background: rgba(37, 99, 235, 0.25);
            color: #93c5fd;
          }
        }
      }

      .library-main {
        .gallery-heading .heading-left .match-count-pill .dot-indicator {
          background: #60a5fa;
        }

        .prompt-gallery-scroll .prompt-gallery .prompt-card {
          &:hover {
            border-color: rgba(96, 165, 250, 0.4);
            box-shadow: 0 6px 18px rgba(0, 0, 0, 0.25);
          }

          .card-image-box {
            background: rgba(15, 23, 42, 0.6);

            .card-placeholder-box {
              background: rgba(30, 41, 59, 0.7);
              color: #64748b;
            }
          }
        }

        .gallery-footer {
          .footer-right .page-indicator-box .curr-page {
            color: #60a5fa;
          }

          .footer-left .footer-source-link:hover {
            color: #60a5fa;
          }
        }

        .prompt-detail .detail-editor {
          .tech-hint-card {
            background: rgba(37, 99, 235, 0.14);
            border-color: rgba(96, 165, 250, 0.3);
            color: #93c5fd;

            .hint-icon {
              color: #60a5fa;
            }

            &.edit-hint-card {
              background: rgba(245, 158, 11, 0.14);
              border-color: rgba(245, 158, 11, 0.3);
              color: #fbbf24;

              .hint-icon {
                color: #f59e0b;
              }
            }
          }
        }
      }
    }
  }
}

@keyframes library-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
