<template>
  <section class="image-prompt-library">
    <BaseModal
      title="提示词库"
      :description="`按分类发现画面灵感 · ${catalog.count || 0} 条提示词`"
      @close="$emit('close')"
    >
      <div class="library-toolbar">
        <label class="search-box">
          <Search :size="15" />
          <input
            v-model="search"
            class="search-input"
            type="search"
            placeholder="搜索标题、提示词或标签"
            aria-label="搜索提示词"
            :disabled="initializing || importing"
          />
        </label>
        <span class="library-source" :title="catalog.filename">{{
          catalog.filename
        }}</span>
        <button
          class="library-button"
          type="button"
          :disabled="initializing || importing"
          @click="importLibrary"
        >
          <Upload :size="14" />{{ importing ? "导入中…" : "导入 JSON" }}
        </button>
      </div>
      <p v-if="error" class="library-error" role="alert">
        {{ error
        }}<button class="retry-button" type="button" @click="initialize">
          重试
        </button>
      </p>
      <div v-if="initializing" class="library-loading">
        <LoaderCircle class="spinning" :size="24" /><span
          >正在加载本地提示词库，首次打开需要建立索引…</span
        >
      </div>
      <div v-else class="library-body">
        <nav class="category-sidebar" aria-label="提示词分类">
          <button
            class="category-button"
            :class="{ active: !category }"
            type="button"
            @click="selectCategory('')"
          >
            <span>全部提示词</span
            ><span class="category-count">{{ catalog.count }}</span>
          </button>
          <section
            v-for="group in categoryGroups"
            :key="group.name"
            class="category-group"
          >
            <div class="category-heading">{{ group.name }}</div>
            <button
              v-for="item in group.items"
              :key="item.id"
              class="category-button"
              :class="{ active: category === item.id }"
              type="button"
              :title="item.originalTitle"
              @click="selectCategory(item.id)"
            >
              <span class="category-name">{{ item.title }}</span
              ><span class="category-count">{{ item.count }}</span>
            </button>
          </section>
        </nav>
        <section class="library-main">
          <template v-if="!detail">
            <header class="gallery-heading">
              <span
                >{{ activeCategory }}
                <span class="result-count">{{ total }} 条</span></span
              ><span v-if="loading" class="loading-label">加载中…</span>
            </header>
            <div class="prompt-gallery-scroll" :aria-busy="loading">
              <div v-if="!items.length && !loading" class="library-empty">
                <Search :size="28" /><span>没有找到匹配的提示词</span
                ><span class="empty-hint">试试其他分类或关键词</span>
              </div>
              <div v-else class="prompt-gallery">
                <button
                  v-for="item in items"
                  :key="item.id"
                  class="prompt-card"
                  type="button"
                  :disabled="detailLoading || importing"
                  @click="openDetail(item.id)"
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
                      <template #placeholder
                        ><div class="image-placeholder">
                          <Image :size="24" /></div
                      ></template>
                      <template #error
                        ><div class="image-placeholder">
                          <ImageOff :size="24" /><span>预览图暂不可用</span>
                        </div></template
                      >
                    </el-image>
                    <div v-else class="image-placeholder">
                      <ImageOff :size="24" /><span>暂无预览图</span>
                    </div>
                    <span class="mode-badge">{{
                      modeLabel(item.inputMode)
                    }}</span>
                  </div>
                  <div class="card-content">
                    <span class="card-title" :title="item.title">{{
                      item.title
                    }}</span
                    ><span class="card-category">{{
                      categoryTitle(item.categoryIds?.[0])
                    }}</span>
                  </div>
                </button>
              </div>
            </div>
            <footer class="gallery-footer">
              <span class="footer-hint">图片来自词库中的链接</span>
              <button
                class="library-button"
                type="button"
                :disabled="page <= 1 || loading"
                @click="changePage(-1)"
              >
                <ChevronLeft :size="14" />
              </button>
              <span>{{ page }} / {{ Math.max(1, Math.ceil(total / 24)) }}</span>
              <button
                class="library-button"
                type="button"
                :disabled="page * 24 >= total || loading"
                @click="changePage(1)"
              >
                <ChevronRight :size="14" />
              </button>
            </footer>
          </template>
          <template v-else>
            <header class="detail-heading">
              <button class="back-button" type="button" @click="detail = null">
                <ArrowLeft :size="14" />返回词库</button
              ><span class="detail-title">{{ detail.title }}</span>
            </header>
            <div class="prompt-detail">
              <div class="detail-visual">
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
                  <template #error
                    ><div class="image-placeholder">
                      <ImageOff :size="32" /><span
                        >图片链接暂不可用，仍可使用提示词</span
                      >
                    </div></template
                  >
                </el-image>
                <div class="detail-tags">
                  <span
                    v-for="id in detail.categoryIds"
                    :key="id"
                    class="category-tag"
                    >{{ categoryTitle(id) }}</span
                  >
                </div>
                <button
                  v-if="sourceUrl"
                  class="source-link"
                  type="button"
                  @click="openSource(sourceUrl)"
                >
                  <ExternalLink :size="12" />查看原始来源
                </button>
              </div>
              <div class="detail-editor">
                <div class="editor-heading">
                  <span>提示词</span
                  ><select
                    v-model="language"
                    class="language-select"
                    aria-label="提示词语言"
                  >
                    <option value="original">原文</option>
                    <option v-if="detail.promptLocalized?.zh" value="zh">
                      中文
                    </option>
                  </select>
                </div>
                <textarea
                  v-model="prompt"
                  class="prompt-text"
                  aria-label="可编辑的提示词"
                  spellcheck="false"
                ></textarea>
                <p v-if="detail.variables?.length" class="detail-hint">
                  此模板含可替换内容，可直接在上方修改占位参数。
                </p>
                <div class="template-meta">
                  <span>{{ detail.model || "沿用当前模型" }}</span
                  ><span>{{ modeLabel(detail.inputMode) }}</span>
                </div>
                <p
                  v-if="detail.inputMode !== 'text_to_image'"
                  class="detail-hint"
                >
                  此模板需要你提供参考图，示例预览图不会作为编辑输入。
                </p>
                <p v-if="promptTooLong" class="detail-error">
                  提示词超过工作台的 32000 字节限制，请精简后使用。
                </p>
                <button
                  class="use-button"
                  type="button"
                  :disabled="!prompt.trim() || promptTooLong || importing"
                  @click="usePrompt"
                >
                  <Sparkles :size="15" />使用此提示词
                </button>
              </div>
            </div>
          </template>
        </section>
      </div>
      <footer class="library-attribution">
        <span>提示词整理自</span>
        <a
          class="attribution-link"
          :href="sourceRepositoryUrl"
          @click.prevent="openSource(sourceRepositoryUrl)"
          >Toolcentral-ai / Image Prompt Gallery<ExternalLink :size="11"
        /></a>
      </footer>
    </BaseModal>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  Image,
  ImageOff,
  LoaderCircle,
  Search,
  Sparkles,
  Upload
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
const promptTooLong = computed(
  () => new TextEncoder().encode(prompt.value).length > 32000
)
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
}

function changePage(delta) {
  page.value += delta
  loadList()
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
    mode: ["single_image_to_image", "multi_image_to_image"].includes(
      detail.value.inputMode
    )
      ? "edit"
      : "generate"
  })
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
})
</script>

<style scoped lang="less">
.image-prompt-library {
  font-size: var(--font-size-base);
  .library-attribution {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 5px;
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--color-line);
    color: var(--color-text-soft);
    font-size: var(--font-size-sm);
    .attribution-link {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      color: var(--color-primary);
      text-decoration: none;
      &:hover {
        text-decoration: underline;
      }
    }
  }
  :deep(.base-modal__panel) {
    width: calc(100vw - 48px);
    max-width: 1200px;
    height: calc(100vh - 48px);
  }
  :deep(.base-modal__content) {
    padding: 8px 20px 18px;
  }
  .library-toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-bottom: 14px;
    .search-box {
      display: flex;
      flex: 1;
      align-items: center;
      gap: 9px;
      padding: 0 11px;
      height: 36px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      .search-input {
        flex: 1;
        min-width: 0;
        border: 0;
        outline: none;
        color: var(--color-text);
        background: transparent;
        font-size: var(--font-size-base);
      }
    }
    .library-source {
      max-width: 200px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      color: var(--color-text-soft);
      font-size: var(--font-size-sm);
    }
  }
  .library-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 36px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    background: var(--color-panel);
    color: var(--color-text);
    cursor: pointer;
    font-size: var(--font-size-sm);
    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }
  .library-error {
    margin: 0 0 12px;
    color: var(--color-danger);
    font-size: var(--font-size-base);
    overflow-wrap: anywhere;
    .retry-button {
      margin-left: 8px;
      border: 0;
      background: transparent;
      color: var(--color-primary);
      cursor: pointer;
    }
  }
  .library-loading {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--color-text-muted);
    font-size: var(--font-size-base);
    .spinning {
      animation: prompt-library-spin 1.4s linear infinite;
    }
  }
  .library-body {
    display: flex;
    flex: 1;
    gap: 20px;
    min-height: 0;
    .category-sidebar {
      flex: 0 0 190px;
      overflow: auto;
      padding-right: 8px;
      border-right: 1px solid var(--color-line);
      .category-button {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 6px;
        width: 100%;
        min-height: 36px;
        padding: 6px 8px;
        margin-bottom: 3px;
        border: 0;
        border-radius: 5px;
        text-align: left;
        background: transparent;
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
        cursor: pointer;
        &:hover {
          background: var(--color-panel-soft);
        }
        &.active {
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }
        .category-name {
          overflow: hidden;
          white-space: nowrap;
          text-overflow: ellipsis;
        }
        .category-count {
          opacity: 0.7;
          font-size: var(--font-size-sm);
        }
      }
      .category-group {
        margin-top: 16px;
        .category-heading {
          padding: 0 8px 7px;
          color: var(--color-text-soft);
          font-size: var(--font-size-sm);
          letter-spacing: 1px;
        }
      }
    }
    .library-main {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-width: 0;
      min-height: 0;
      .gallery-heading {
        display: flex;
        justify-content: space-between;
        padding: 2px 0 13px;
        font-size: var(--font-size-lg);
        .result-count,
        .loading-label {
          margin-left: 8px;
          color: var(--color-text-soft);
          font-size: var(--font-size-sm);
        }
      }
      .prompt-gallery-scroll {
        flex: 1;
        min-height: 0;
        overflow: auto;
        padding-right: 6px;
        .library-empty {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: 12px;
          height: 100%;
          color: var(--color-text-muted);
          font-size: var(--font-size-base);
          .empty-hint {
            color: var(--color-text-soft);
            font-size: var(--font-size-sm);
          }
        }
        .prompt-gallery {
          display: flex;
          flex-wrap: wrap;
          gap: 14px;
          .prompt-card {
            display: flex;
            flex-direction: column;
            width: calc((100% - 28px) / 3);
            padding: 0;
            overflow: hidden;
            border: 1px solid var(--color-line);
            border-radius: 8px;
            background: var(--color-panel);
            color: var(--color-text);
            text-align: left;
            cursor: pointer;
            transition: border-color 0.15s;
            &:hover {
              border-color: var(--color-primary);
            }
            &:disabled {
              cursor: progress;
            }
            .card-image {
              position: relative;
              width: 100%;
              height: 152px;
              background: var(--color-panel-soft);
              .preview-image {
                width: 100%;
                height: 100%;
              }
              .mode-badge {
                position: absolute;
                left: 8px;
                bottom: 8px;
                padding: 3px 7px;
                border-radius: 4px;
                background: #101820b8;
                color: #fff;
                font-size: var(--font-size-sm);
              }
            }
            .card-content {
              display: flex;
              flex-direction: column;
              gap: 5px;
              padding: 11px;
              .card-title {
                display: -webkit-box;
                -webkit-line-clamp: 2;
                -webkit-box-orient: vertical;
                overflow: hidden;
                min-height: 3em;
                line-height: 1.5;
                font-size: var(--font-size-base);
              }
              .card-category {
                color: var(--color-text-soft);
                font-size: var(--font-size-sm);
              }
            }
          }
        }
      }
      .gallery-footer {
        display: flex;
        align-items: center;
        gap: 10px;
        padding-top: 14px;
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
        .footer-hint {
          flex: 1;
          color: var(--color-text-soft);
          font-size: var(--font-size-sm);
        }
      }
      .detail-heading {
        display: flex;
        align-items: center;
        gap: 16px;
        padding-bottom: 12px;
        .back-button {
          display: flex;
          flex: none;
          align-items: center;
          gap: 5px;
          padding: 0;
          border: 0;
          background: transparent;
          color: var(--color-primary);
          font-size: var(--font-size-sm);
          cursor: pointer;
        }
        .detail-title {
          overflow: hidden;
          white-space: nowrap;
          text-overflow: ellipsis;
          font-size: var(--font-size-lg);
        }
      }
      .prompt-detail {
        display: flex;
        flex: 1;
        gap: 18px;
        min-height: 0;
        overflow: auto;
        .detail-visual {
          display: flex;
          flex-direction: column;
          width: 43%;
          gap: 12px;
          .detail-image {
            flex: none;
            width: 100%;
            height: 280px;
            background: var(--color-panel-soft);
            border: 1px solid var(--color-line);
            border-radius: 7px;
          }
          .detail-tags {
            display: flex;
            flex-wrap: wrap;
            gap: 6px;
            .category-tag {
              padding: 3px 6px;
              background: var(--color-panel-soft);
              color: var(--color-text-muted);
              border-radius: 4px;
              font-size: var(--font-size-sm);
            }
          }
          .source-link {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            align-self: flex-start;
            padding: 0;
            border: 0;
            background: transparent;
            color: var(--color-primary);
            font-size: var(--font-size-sm);
            cursor: pointer;
          }
        }
        .detail-editor {
          display: flex;
          flex: 1;
          min-width: 0;
          min-height: 340px;
          flex-direction: column;
          gap: 10px;
          .editor-heading {
            display: flex;
            justify-content: space-between;
            align-items: center;
            color: var(--color-text-muted);
            font-size: var(--font-size-base);
            .language-select {
              padding: 4px 7px;
              border: 1px solid var(--color-line);
              border-radius: 4px;
              background: var(--color-panel);
              color: var(--color-text);
              font-size: var(--font-size-sm);
            }
          }
          .prompt-text {
            flex: 1;
            min-height: 150px;
            resize: none;
            padding: 12px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            outline: none;
            background: var(--color-panel-soft);
            color: var(--color-text);
            font-family: inherit;
            font-size: var(--font-size-base);
            line-height: 1.7;
            &:focus {
              border-color: var(--color-primary);
            }
          }
          .detail-hint {
            margin: 0;
            color: var(--color-text-soft);
            font-size: var(--font-size-sm);
            line-height: 1.6;
          }
          .detail-error {
            margin: 0;
            color: var(--color-danger);
            font-size: var(--font-size-sm);
          }
          .template-meta {
            display: flex;
            justify-content: space-between;
            gap: 8px;
            color: var(--color-text-muted);
            font-size: var(--font-size-sm);
          }
          .use-button {
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 8px;
            flex: none;
            height: 36px;
            border: 0;
            border-radius: 6px;
            background: var(--color-primary-solid);
            color: #fff;
            cursor: pointer;
            &:disabled {
              opacity: 0.5;
              cursor: not-allowed;
            }
          }
        }
      }
    }
  }
  .image-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 9px;
    width: 100%;
    height: 100%;
    color: var(--color-text-soft);
    font-size: var(--font-size-sm);
  }
}
@keyframes prompt-library-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
