<template>
  <section class="quick-notes">
    <aside class="notes-sidebar">
      <section
        v-for="section in navSections"
        :key="section.title"
        class="notes-nav-section"
      >
        <div class="notes-sidebar-heading">
          <span>{{ section.title }}</span
          ><ListFilter :size="15" />
        </div>
        <nav class="notes-nav" :aria-label="section.title">
          <button
            v-for="item in views.filter((item) =>
              section.ids.includes(item.id)
            )"
            :key="item.id"
            type="button"
            class="notes-nav-item"
            :class="{ 'notes-nav-active': view === item.id && !groupId }"
            @click="selectView(item.id)"
          >
            <component :is="item.icon" :size="19" /><span>{{ item.label }}</span
            ><span
              v-if="item.id === 'reminders' && unread"
              class="notes-count"
              >{{ unread }}</span
            >
          </button>
        </nav>
      </section>
      <div
        v-if="groups.some((folder) => folder.id !== 'inbox')"
        class="notes-sidebar-heading"
      >
        <span>项目分组</span
        ><button
          class="notes-icon"
          type="button"
          title="管理分类与分组"
          @click="organizer = true"
        >
          <Settings2 :size="14" />
        </button>
      </div>
      <nav
        v-if="groups.some((folder) => folder.id !== 'inbox')"
        class="notes-nav"
        aria-label="项目分组"
      >
        <button
          v-for="folder in groups"
          :key="folder.id"
          class="notes-nav-item"
          :class="{ 'notes-nav-active': groupId === folder.id }"
          type="button"
          @click="selectGroup(folder.id)"
        >
          <Folder :size="14" /><span>{{ folder.name }}</span>
        </button>
      </nav>
      <p class="notes-sidebar-foot">
        <HardDrive :size="13" />本机保存 · 托盘运行可提醒
      </p>
    </aside>
    <main
      class="notes-main"
      :class="{ 'notes-main-calendar': view === 'calendar' || !!editing?.id }"
    >
      <QuickNoteEditor
        v-if="editing?.id"
        ref="noteEditor"
        :key="editing.id"
        inline
        :note="editing"
        :folders="state.folders"
        @close="editing = null"
        @saved="saved"
      />
      <template v-else>
        <header v-if="view !== 'calendar'" class="notes-header">
          <div>
            <span class="notes-eyebrow">QUICK NOTES / 工作与生活</span>
            <h2 class="notes-title">{{ heading }}</h2>
            <p class="notes-subtitle">
              {{
                view === "worklog"
                  ? "沿着日期回看过程、结果和下一步。"
                  : "想法、待办与工作进展，都有自己的位置。"
              }}
            </p>
          </div>
          <div class="notes-header-actions">
            <button
              class="notes-button"
              type="button"
              @click="organizer = true"
            >
              <Layers :size="14" />分类与分组</button
            ><button
              class="notes-button"
              type="button"
              @click="openSmartCapture"
            >
              <Sparkles :size="14" />智能记录</button
            ><button
              class="notes-button notes-primary"
              type="button"
              @click="newNote"
            >
              <Plus :size="15" />新建便签
            </button>
          </div>
        </header>
        <p v-if="error" class="notes-error" role="alert">
          {{ error
          }}<button class="notes-text" type="button" @click="error = ''">
            收起
          </button>
        </p>
        <section v-if="view === 'reminders'" class="notes-reminders">
          <div class="notes-reminder-intro">
            <Bell :size="18" /><span
              >系统通知与应用内记录同步保存。应用完全退出时暂停通知，下次启动补发。</span
            >
          </div>
          <article
            v-for="event in state.events"
            :key="event.id"
            class="notes-event"
            :class="{ 'notes-event-unread': !event.readAt }"
          >
            <Bell :size="17" />
            <div class="notes-event-body">
              <span>{{ event.title }}</span
              ><span class="notes-caption"
                >{{ formatDateTime(event.scheduledAt) }} ·
                {{ eventLabels[event.status] || event.status }}</span
              ><span v-if="event.error" class="notes-error">{{
                event.error
              }}</span>
            </div>
            <button
              class="notes-button"
              type="button"
              @click="edit({ id: event.noteId })"
            >
              查看</button
            ><button
              v-if="!event.readAt"
              class="notes-text"
              type="button"
              @click="reminderAction(event, 'read')"
            >
              已读</button
            ><select
              v-if="event.status !== 'cancelled'"
              class="notes-select"
              aria-label="稍后提醒"
              @change="snooze(event, $event)"
            >
              <option value="">稍后提醒</option>
              <option
                v-for="minutes in [5, 10, 30, 60]"
                :key="minutes"
                :value="minutes"
              >
                {{ minutes }} 分钟后
              </option></select
            ><button
              v-if="eventTask(event)"
              class="notes-text"
              type="button"
              @click="completeEvent(event)"
            >
              完成任务
            </button>
          </article>
          <div v-if="!state.events.length" class="notes-empty">
            <Bell :size="30" /><span>还没有提醒记录</span
            ><span class="notes-caption"
              >为便签或待办设置时间，到点后会在这里留下记录。</span
            >
          </div>
        </section>
        <template v-else>
          <div
            v-if="view !== 'calendar' && filtersVisible"
            class="notes-filters"
          >
            <label class="notes-search"
              ><Search :size="15" /><input
                v-model="search"
                ref="searchInput"
                placeholder="搜索内容、标签、工作记录"
                @input="scheduleLoad" /></label
            ><select
              v-model="categoryId"
              class="notes-select"
              aria-label="分类筛选"
              @change="load"
            >
              <option value="">所有分类</option>
              <option
                v-for="folder in categories"
                :key="folder.id"
                :value="folder.id"
              >
                {{ folder.name }}
              </option></select
            ><select
              v-if="view !== 'calendar'"
              v-model="groupBy"
              class="notes-select"
              aria-label="分组显示"
            >
              <option value="none">不分组</option>
              <option value="groupId">按项目分组</option>
              <option value="categoryId">按分类</option>
              <option value="date">按日期</option>
              <option value="status">按任务状态</option></select
            ><select
              v-if="view !== 'calendar'"
              v-model="display"
              class="notes-select"
              aria-label="显示方式"
            >
              <option value="cards">便签卡片</option>
              <option value="list">紧凑列表</option>
              <option value="timeline">工作时间线</option></select
            ><button
              class="notes-icon"
              type="button"
              title="刷新便签"
              @click="load"
            >
              <RefreshCw :size="15" />
            </button>
          </div>
          <div
            v-if="view !== 'calendar' && filtersVisible"
            class="notes-filter-dates"
          >
            <label>从 <input v-model="fromDate" type="date" /></label
            ><label>至 <input v-model="toDate" type="date" /></label
            ><select
              v-model="typeFilter"
              class="notes-select"
              aria-label="记录类型"
            >
              <option value="">全部类型</option>
              <option v-for="type in noteTypes" :key="type.id" :value="type.id">
                {{ type.label }}
              </option></select
            ><span class="notes-caption">{{ filtered.length }} 条记录</span>
          </div>
          <div v-if="selected.size" class="notes-bulk">
            <span>已选 {{ selected.size }} 条</span
            ><button class="notes-text" type="button" @click="selectAll">
              选择当前视图</button
            ><select
              class="notes-select"
              aria-label="批量移动分组"
              @change="moveSelected($event)"
            >
              <option value="">移动到…</option>
              <option
                v-for="folder in groups"
                :key="folder.id"
                :value="folder.id"
              >
                {{ folder.name }}
              </option></select
            ><button
              class="notes-text"
              type="button"
              @click="exportNotes('markdown')"
            >
              <Download :size="14" />导出 Markdown</button
            ><button
              class="notes-text"
              type="button"
              @click="exportNotes('json')"
            >
              JSON</button
            ><button class="notes-text" type="button" @click="summary">
              生成工作小结</button
            ><button class="notes-text" type="button" @click="classifySelected">
              AI 补分类</button
            ><button
              class="notes-text"
              type="button"
              @click="
                batchLifecycle(
                  view === 'trash' || view === 'archived'
                    ? 'restore'
                    : 'archive'
                )
              "
            >
              {{
                view === "trash" || view === "archived" ? "恢复记录" : "归档"
              }}</button
            ><button
              class="notes-text notes-danger"
              type="button"
              @click="batchLifecycle(view === 'trash' ? 'purge' : 'trash')"
            >
              {{ view === "trash" ? "彻底删除" : "删除" }}</button
            ><button
              class="notes-text"
              type="button"
              @click="selected = new Set()"
            >
              取消选择
            </button>
          </div>
          <section
            v-if="visibleCaptures.length && view === 'all' && !groupId"
            class="notes-pending"
          >
            <div class="notes-section-title">
              <Sparkles :size="14" />待整理 · 原文已保留
            </div>
            <article
              v-for="capture in visibleCaptures"
              :key="capture.id"
              class="notes-pending-item"
            >
              <div>
                <span>{{ capture.sourceText.slice(0, 70) }}</span
                ><span class="notes-caption">{{
                  capture.status === "running"
                    ? "AI 正在整理…"
                    : capture.status === "preview"
                      ? "AI 已整理，待确认保存"
                      : capture.error
                }}</span>
              </div>
              <button
                v-if="capture.status === 'running'"
                class="notes-text"
                type="button"
                @click="cancelCapture(capture)"
              >
                取消</button
              ><template v-else
                ><button
                  class="notes-text"
                  type="button"
                  @click="retry(capture)"
                >
                  重试</button
                ><button
                  class="notes-text"
                  type="button"
                  @click="recover(capture)"
                >
                  保存原文</button
                ><button
                  class="notes-text"
                  type="button"
                  @click="dismiss(capture)"
                >
                  忽略
                </button></template
              >
            </article>
          </section>
          <div
            class="notes-workspace"
            :class="
              view === 'calendar'
                ? 'notes-display-calendar'
                : `notes-display-${display}`
            "
            :aria-busy="loading"
          >
            <QuickNoteCalendar
              v-if="view === 'calendar'"
              :notes="filtered"
              :reminders="state.reminders"
              :rules="state.calendarRules || []"
              @open="edit"
              @create="newNote"
              @smart-record="openSmartCapture"
            />
            <template v-else>
              <section
                v-for="section in sections"
                :key="section.id"
                class="notes-group"
              >
                <h3 v-if="section.title" class="notes-group-title">
                  {{ section.title }}<span>{{ section.items.length }}</span>
                </h3>
                <div class="notes-cards">
                  <QuickNoteCard
                    v-for="note in section.items"
                    :key="note.id"
                    :note="note"
                    :folders="state.folders"
                    :next-reminder="nextReminder(note.id)"
                    :selected="selected.has(note.id)"
                    @open="edit"
                    @reminder="edit($event, true)"
                    @select="toggleSelect"
                    @pin="pin"
                    @task="toggleTask"
                  />
                </div>
              </section>
              <div v-if="!filtered.length && !loading" class="notes-empty">
                <NotebookPen :size="34" /><span>{{
                  search ? "没有找到匹配的记录" : "从一条便签开始"
                }}</span
                ><span class="notes-caption"
                  >记住要做的事，也留下已经完成的事。</span
                ><button class="notes-button" type="button" @click="newNote">
                  写一条便签
                </button>
              </div>
            </template>
            <p
              v-if="view === 'calendar' && state.hasMore"
              class="notes-caption"
            >
              日历显示已加载的记录，加载更多可查看其余事项。
            </p>
            <button
              v-if="state.hasMore"
              class="notes-button notes-load"
              type="button"
              @click="loadMore"
            >
              加载更多
            </button>
          </div>
        </template>
      </template>
    </main>
    <QuickNoteSmartCapture
      :open="smartCaptureOpen"
      :folders="state.folders"
      :providers="providers"
      :codex-accounts="codexAccounts"
      :runtime-models="runtimeModels"
      @close="smartCaptureOpen = false"
      @saved="load()"
    />
    <QuickNoteEditor
      v-if="editing && !editing.id"
      ref="noteEditor"
      :key="editing.id || 'new'"
      :note="editing"
      :folders="state.folders"
      @close="editing = null"
      @saved="saved"
    />
    <BaseModal
      v-if="organizer"
      title="分类与项目分组"
      @close="organizer = false"
      ><div class="notes-organizer">
        <div class="notes-caption">
          分类描述事情的性质，分组归属项目。别名帮助 AI 匹配已有目录。
        </div>
        <div class="notes-folder-list">
          <button
            v-for="folder in state.folders.filter(
              (item) => !['inbox', 'uncategorized'].includes(item.id)
            )"
            :key="folder.id"
            class="notes-folder-row"
            type="button"
            @click="
              folderDraft = {
                ...folder,
                aliasesText: folder.aliases.join('，')
              }
            "
          >
            <Folder :size="14" /><span>{{ folder.name }}</span
            ><span class="notes-caption"
              >{{ folder.kind === "group" ? "项目分组" : "分类"
              }}{{ folder.archived ? " · 已归档" : "" }}</span
            >
          </button>
        </div>
        <form class="notes-folder-form" @submit.prevent="saveFolder">
          <select
            v-model="folderDraft.kind"
            class="notes-select"
            :disabled="!!folderDraft.id"
          >
            <option value="group">项目分组</option>
            <option value="category">分类</option></select
          ><input
            v-model="folderDraft.name"
            placeholder="目录名称"
            required
          /><input
            v-model="folderDraft.aliasesText"
            placeholder="识别别名，逗号分隔"
          /><label
            ><input
              v-model="folderDraft.archived"
              type="checkbox"
            />归档目录</label
          ><select
            v-if="folderDraft.id"
            v-model="folderDraft.mergeInto"
            class="notes-select"
          >
            <option value="">保留当前目录</option>
            <option
              v-for="folder in state.folders.filter(
                (item) =>
                  item.kind === folderDraft.kind && item.id !== folderDraft.id
              )"
              :key="folder.id"
              :value="folder.id"
            >
              合并到 {{ folder.name }}
            </option></select
          ><button class="notes-button notes-primary">
            {{ folderDraft.id ? "保存修改" : "创建目录" }}</button
          ><button
            class="notes-text"
            type="button"
            @click="
              folderDraft = {
                kind: 'group',
                name: '',
                aliasesText: '',
                archived: false
              }
            "
          >
            新增
          </button>
        </form>
        <p v-if="folderError" class="notes-error">{{ folderError }}</p>
      </div></BaseModal
    >
    <BaseModal
      v-if="purgeConfirm"
      title="彻底删除便签"
      @close="purgeConfirm = false"
      ><p>将永久删除所选便签、原文、历史及提醒记录，此操作无法恢复。</p>
      <button class="notes-button" type="button" @click="purgeConfirm = false">
        取消</button
      ><button
        class="notes-button notes-danger"
        type="button"
        @click="batchLifecycle('purge', true)"
      >
        确认彻底删除
      </button></BaseModal
    >
  </section>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue"
import {
  Archive,
  Bell,
  CalendarDays,
  CheckCheck,
  Clock3,
  Download,
  Folder,
  HardDrive,
  Mail,
  ListFilter,
  Layers,
  ListTodo,
  NotebookPen,
  Plus,
  RefreshCw,
  Search,
  Settings2,
  Sparkles,
  StickyNote,
  Trash2
} from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import QuickNoteSmartCapture from "./components/QuickNoteSmartCapture.vue"
import QuickNoteCard from "./components/QuickNoteCard.vue"
import QuickNoteEditor from "./components/QuickNoteEditor.vue"
import QuickNoteCalendar from "./components/QuickNoteCalendar.vue"
import { quickNotesApi } from "@/api/modules/quick-notes"
import { noteTypes } from "./constant"
import { formatDateTime } from "@/utils/formatters"
const props = defineProps({
  providers: { type: Array, default: () => [] },
  codexAccounts: { type: Array, default: () => [] },
  runtimeModels: { type: Array, default: () => [] }
})
const state = ref({
  notes: [],
  folders: [],
  reminders: [],
  events: [],
  captures: []
})
const view = ref("calendar")
const groupId = ref("")
const categoryId = ref("")
const groupBy = ref(localStorage.getItem("quick-notes:group-by") || "none")
const display = ref(localStorage.getItem("quick-notes:display") || "cards")
const typeFilter = ref("")
const fromDate = ref("")
const toDate = ref("")
const search = ref("")
const searchInput = ref(null)
const filtersVisible = ref(true)
const smartCaptureOpen = ref(false)
const selected = ref(new Set())
const loading = ref(false)
const error = ref("")
const folderError = ref("")
const editing = ref(null)
const noteEditor = ref(null)
const organizer = ref(false)
const purgeConfirm = ref(false)
const folderDraft = ref({
  kind: "group",
  name: "",
  aliasesText: "",
  archived: false
})
const views = [
  { id: "all", label: "全部记录", icon: StickyNote },
  { id: "calendar", label: "日历日程", icon: CalendarDays },
  { id: "inbox", label: "收件箱", icon: Mail },
  { id: "todo", label: "待办事项", icon: ListTodo },
  { id: "today", label: "今日提醒", icon: Bell },
  { id: "overdue", label: "已逾期", icon: Clock3 },
  { id: "worklog", label: "工作留痕", icon: NotebookPen },
  { id: "completed", label: "已完成", icon: CheckCheck },
  { id: "reminders", label: "提醒中心", icon: Bell },
  { id: "archived", label: "已归档", icon: Archive },
  { id: "trash", label: "回收站", icon: Trash2 }
]
const navSections = [
  { title: "工作区", ids: ["all", "calendar", "inbox", "todo"] },
  { title: "提醒", ids: ["today", "overdue"] },
  {
    title: "归档",
    ids: ["worklog", "completed", "reminders", "archived", "trash"]
  }
]
const eventLabels = {
  pending: "等待通知",
  delivering: "正在投递",
  sent: "已通知",
  failed: "系统通知失败",
  snoozed: "稍后提醒",
  cancelled: "已停止"
}
const groups = computed(() =>
  state.value.folders.filter(
    (folder) => folder.kind === "group" && !folder.archived
  )
)
const categories = computed(() =>
  state.value.folders.filter(
    (folder) => folder.kind === "category" && !folder.archived
  )
)
const heading = computed(
  () =>
    state.value.folders.find((folder) => folder.id === groupId.value)?.name ||
    views.find((item) => item.id === view.value)?.label
)
const unread = computed(
  () =>
    state.value.events.filter(
      (event) => !event.readAt && ["sent", "failed"].includes(event.status)
    ).length
)
const visibleCaptures = computed(() =>
  state.value.captures.filter(
    (capture) => !["dismissed"].includes(capture.status)
  )
)
const filtered = computed(() =>
  state.value.notes.filter((note) => {
    // 日历显示完整事项，列表视图的筛选条件不影响日历。
    if (view.value === "calendar") return true
    const date =
      note.occurredOn || new Date(note.createdAt).toLocaleDateString("sv-SE")
    if (
      (fromDate.value && date < fromDate.value) ||
      (toDate.value && date > toDate.value)
    )
      return false
    if (typeFilter.value && note.type !== typeFilter.value) return false
    const incomplete =
      note.tasks?.filter(
        (task) => !["completed", "cancelled"].includes(task.status)
      ) || []
    if (view.value === "inbox") return note.groupId === "inbox"
    if (view.value === "todo") return incomplete.length > 0
    if (view.value === "completed")
      return note.tasks?.length && !incomplete.length
    if (view.value === "worklog") return note.type === "worklog"
    if (view.value === "today")
      return state.value.reminders.some(
        (plan) =>
          plan.noteId === note.id &&
          plan.enabled &&
          new Date(plan.nextFireAt).toDateString() === new Date().toDateString()
      )
    if (view.value === "overdue")
      return incomplete.some((task) =>
        task.dueAt
          ? task.dueAt < Date.now()
          : task.dueDate &&
            task.dueDate < new Date().toLocaleDateString("sv-SE")
      )
    return true
  })
)
const sections = computed(() => {
  const grouping = display.value === "timeline" ? "date" : groupBy.value
  const buckets = new Map()
  for (const note of filtered.value) {
    let key =
      grouping === "none"
        ? "all"
        : grouping === "date"
          ? note.occurredOn ||
            new Date(note.createdAt).toLocaleDateString("sv-SE")
          : grouping === "status"
            ? note.tasks?.length
              ? note.tasks.every((task) =>
                  ["completed", "cancelled"].includes(task.status)
                )
                ? "已完成"
                : "待完成 / 进行中"
              : "无待办"
            : note[grouping]
    if (!buckets.has(key))
      buckets.set(key, {
        id: key,
        title:
          grouping === "none"
            ? ""
            : ["groupId", "categoryId"].includes(grouping)
              ? state.value.folders.find((folder) => folder.id === key)?.name ||
                "未分类"
              : key,
        items: []
      })
    buckets.get(key).items.push(note)
  }
  return grouping === "date"
    ? [...buckets.values()].sort((a, b) => b.id.localeCompare(a.id))
    : [...buckets.values()]
})
let sequence = 0
let disposed = false
let unsubscribe
let unsubscribeOpen
let searchTimer
let recoverCapture = null
// 模块事件只读取便签数据；序号保护筛选切换和多窗口更新不会被旧请求覆盖。
async function load(more = false) {
  const current = ++sequence
  loading.value = true
  try {
    const result = await quickNotesApi.invoke("list", {
      lifecycle: ["trash", "archived"].includes(view.value)
        ? view.value
        : "active",
      search: view.value === "calendar" ? "" : search.value,
      groupId: groupId.value,
      categoryId: view.value === "calendar" ? "" : categoryId.value,
      offset: more === true ? state.value.notes.length : 0,
      limit: 300
    })
    if (!disposed && current === sequence)
      state.value = {
        ...result,
        notes:
          more === true ? [...state.value.notes, ...result.notes] : result.notes
      }
  } catch (failure) {
    if (!disposed) error.value = failure.message || String(failure)
  } finally {
    if (current === sequence) loading.value = false
  }
}
function scheduleLoad() {
  clearTimeout(searchTimer)
  searchTimer = setTimeout(load, 200)
}
function loadMore() {
  load(true)
}
// 顶部工具栏只控制当前便签工作区，其他工具保留各自的交互。
async function focusSearch() {
  selectView("all")
  filtersVisible.value = true
  await nextTick()
  searchInput.value.focus()
}
function toggleFilters() {
  if (view.value === "calendar" || editing.value) {
    selectView("all")
    filtersVisible.value = true
  } else filtersVisible.value = !filtersVisible.value
}
function openOrganizer() {
  organizer.value = true
}
defineExpose({ refresh: load, focusSearch, toggleFilters, openOrganizer })
function selectView(id) {
  editing.value = null
  view.value = id
  groupId.value = ""
  selected.value = new Set()
  load()
}
function selectGroup(id) {
  editing.value = null
  view.value = "all"
  groupId.value = id
  selected.value = new Set()
  load()
}
function newNote(occurredOn) {
  recoverCapture = null
  editing.value = {
    groupId: groupId.value || "inbox",
    type: view.value === "worklog" ? "worklog" : "note",
    // 日历入口使用所选日期，普通新建按钮传入的点击事件不作为日期保存。
    occurredOn: typeof occurredOn === "string" ? occurredOn : ""
  }
}
// 智能记录由本模块的预览组件处理，窗口关闭不取消后台分析。
function openSmartCapture() {
  smartCaptureOpen.value = true
}
async function edit(note, reminder = false) {
  recoverCapture = null
  try {
    editing.value = await quickNotesApi.invoke("detail", { id: note.id })
    if (reminder) {
      await nextTick()
      await noteEditor.value.openReminders()
    }
  } catch (failure) {
    error.value = String(failure)
  }
}
async function saved() {
  editing.value = null
  if (recoverCapture) {
    await dismiss(recoverCapture)
    recoverCapture = null
  }
  await load()
}
function toggleSelect(id) {
  const items = new Set(selected.value)
  items.has(id) ? items.delete(id) : items.add(id)
  selected.value = items
}
function selectAll() {
  selected.value = new Set(filtered.value.map((note) => note.id))
}
function nextReminder(id) {
  const times = state.value.reminders
    .filter((plan) => plan.noteId === id && plan.enabled)
    .map((plan) => plan.nextFireAt)
  return times.length ? Math.min(...times) : null
}
async function pin(note) {
  try {
    await quickNotesApi.invoke("save", { ...note, pinned: !note.pinned })
    await load()
  } catch (failure) {
    error.value = String(failure)
  }
}
async function toggleTask(note, task) {
  try {
    await quickNotesApi.invoke("task-update", {
      id: note.id,
      revision: note.revision,
      taskId: task.id,
      status: task.status === "completed" ? "pending" : "completed"
    })
    await load()
  } catch (failure) {
    error.value = String(failure)
  }
}
async function batchLifecycle(action, confirmed = false) {
  if (action === "purge" && !confirmed) {
    purgeConfirm.value = true
    return
  }
  try {
    for (const note of state.value.notes.filter((item) =>
      selected.value.has(item.id)
    ))
      await quickNotesApi.invoke(action, {
        id: note.id,
        revision: note.revision
      })
    selected.value = new Set()
    purgeConfirm.value = false
    await load()
  } catch (failure) {
    error.value = String(failure)
    await load()
  }
}
async function moveSelected(event) {
  const id = event.target.value
  if (!id) return
  try {
    for (const note of state.value.notes.filter((item) =>
      selected.value.has(item.id)
    ))
      await quickNotesApi.invoke("save", { ...note, groupId: id })
    selected.value = new Set()
    await load()
  } catch (failure) {
    error.value = String(failure)
  }
  event.target.value = ""
}
async function exportNotes(format) {
  try {
    const result = await quickNotesApi.invoke("export", {
      ids: [...selected.value],
      format
    })
    const url = URL.createObjectURL(
      new Blob([result.text], {
        type: format === "json" ? "application/json" : "text/markdown"
      })
    )
    const link = document.createElement("a")
    link.href = url
    link.download = `速记-${new Date().toLocaleDateString("sv-SE")}.${format === "json" ? "json" : "md"}`
    link.click()
    setTimeout(() => URL.revokeObjectURL(url), 1000)
  } catch (failure) {
    error.value = String(failure)
  }
}
async function summary() {
  try {
    await quickNotesApi.invoke("summary", {
      ids: [...selected.value],
      requestId: crypto.randomUUID()
    })
    view.value = "all"
    groupId.value = ""
    await load()
  } catch (failure) {
    error.value = String(failure)
  }
}
// AI 只补默认目录和空标签，已人工整理的字段保持原值。
async function classifySelected() {
  try {
    for (const id of selected.value)
      await quickNotesApi.invoke("classify", {
        noteId: id,
        requestId: crypto.randomUUID()
      })
    await load()
  } catch (failure) {
    error.value = String(failure)
  }
}
async function retry(capture) {
  try {
    await quickNotesApi.invoke("capture", {
      text: capture.sourceText,
      requestId: crypto.randomUUID(),
      source: {
        type: "retry",
        captureId: capture.id,
        timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone
      }
    })
    await dismiss(capture)
  } catch (failure) {
    error.value = String(failure)
  }
}
function recover(capture) {
  recoverCapture = capture
  editing.value = { content: capture.sourceText }
}
async function dismiss(capture) {
  try {
    await quickNotesApi.invoke("capture-discard", { id: capture.id })
    await load()
  } catch (failure) {
    error.value = String(failure)
  }
}
async function cancelCapture(capture) {
  await quickNotesApi.invoke("capture-cancel", { requestId: capture.id })
  await load()
}
async function saveFolder() {
  try {
    await quickNotesApi.invoke("organize", {
      ...folderDraft.value,
      aliases: folderDraft.value.aliasesText
        .split(/[,，]/)
        .map((value) => value.trim())
        .filter(Boolean)
    })
    folderDraft.value = {
      kind: "group",
      name: "",
      aliasesText: "",
      archived: false
    }
    folderError.value = ""
    await load()
  } catch (failure) {
    folderError.value = String(failure)
  }
}
async function reminderAction(event, action, until) {
  try {
    await quickNotesApi.invoke("reminder-action", {
      id: event.id,
      action,
      until
    })
    await load()
  } catch (failure) {
    error.value = String(failure)
  }
}
function snooze(event, change) {
  const minutes = Number(change.target.value)
  change.target.value = ""
  if (minutes) reminderAction(event, "snooze", Date.now() + minutes * 60000)
}
function eventTask(event) {
  const plan = state.value.reminders.find(
    (plan) => plan.id === event.reminderId
  )
  return plan?.taskId || null
}
async function completeEvent(event) {
  try {
    const note = await quickNotesApi.invoke("detail", { id: event.noteId })
    await quickNotesApi.invoke("task-update", {
      id: note.id,
      revision: note.revision,
      taskId: eventTask(event),
      status: "completed"
    })
    await reminderAction(event, "read")
  } catch (failure) {
    error.value = String(failure)
  }
}
watch([groupBy, display], () => {
  localStorage.setItem("quick-notes:group-by", groupBy.value)
  localStorage.setItem("quick-notes:display", display.value)
})
onMounted(async () => {
  try {
    const stop = await quickNotesApi.onChanged(load)
    if (disposed) return stop()
    unsubscribe = stop
    unsubscribeOpen = await quickNotesApi.onOpen((note) => {
      if (note.id) edit(note)
      else editing.value = { content: note.content || "" }
    })
    if (disposed) {
      unsubscribeOpen()
      return
    }
    await load()
    const options = await quickNotesApi.invoke("options")
    if (options.openNoteId) await edit({ id: options.openNoteId })
    else if (typeof options.draft === "string")
      editing.value = { content: options.draft }
  } catch (failure) {
    if (!disposed) error.value = String(failure)
  }
})
onBeforeUnmount(() => {
  disposed = true
  clearTimeout(searchTimer)
  unsubscribe?.()
  unsubscribeOpen?.()
})
</script>

<style scoped lang="less">
.quick-notes {
  display: flex;
  flex: 1;
  gap: 16px;
  min-height: 0;
  height: 100%;
  overflow: hidden;
  color: var(--color-text);
  font-size: 13px;
  background: var(--color-panel-soft);
  padding: 0 10px 12px 0;
  .notes-button,
  .notes-icon,
  .notes-text {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    flex: none;
    padding: 9px 13px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
    transition:
      border-color 150ms,
      background 150ms;
    &:hover {
      border-color: var(--color-primary);
      color: var(--color-primary);
    }
    &:disabled {
      opacity: 0.5;
      cursor: default;
    }
  }
  .notes-primary {
    background: linear-gradient(140deg, #3d7dbc, #245b96);
    border-color: #3471ae;
    color: white;
    box-shadow: 0 3px 8px #22548a12;
    &:hover {
      color: white;
    }
  }
  .notes-icon {
    padding: 8px;
  }
  .notes-text {
    padding: 4px;
    border: 0;
    background: transparent;
    color: var(--color-primary);
    font-size: 12px;
  }
  .notes-danger {
    color: var(--color-danger);
  }
  .notes-caption {
    color: var(--color-text-muted);
    font-size: 11px;
    line-height: 1.7;
  }
  .notes-select {
    min-width: 100px;
    max-width: 200px;
    border: 1px solid var(--color-line);
    border-radius: 10px;
    padding: 9px 11px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    font-size: 12px;
  }
  .notes-sidebar {
    display: flex;
    flex-direction: column;
    flex: 0 0 202px;
    min-height: 0;
    gap: 14px;
    padding: 20px 13px 10px;
    border-radius: 0 20px 20px 0;
    overflow: auto;
    background: linear-gradient(
      135deg,
      var(--color-panel-soft),
      var(--color-primary-soft)
    );
    .notes-nav-section {
      padding-bottom: 14px;
      border-bottom: 1px solid var(--color-line);
      &:last-of-type {
        border-bottom: 0;
      }
    }
    .notes-sidebar-heading {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      padding: 6px 13px 12px;
      font-size: 13px;
    }
    .notes-nav {
      display: flex;
      flex-direction: column;
      gap: 5px;
      .notes-nav-item {
        display: flex;
        align-items: center;
        gap: 12px;
        min-height: 39px;
        padding: 9px 13px;
        border: 1px solid transparent;
        border-radius: 10px;
        background: transparent;
        color: var(--color-text);
        text-align: left;
        font: inherit;
        font-size: 13px;
        cursor: pointer;
        &:hover {
          background: var(--color-panel);
        }
        &.notes-nav-active {
          color: var(--color-primary);
          background: color-mix(
            in srgb,
            var(--color-primary) 13%,
            var(--color-panel)
          );
        }
        .notes-count {
          margin-left: auto;
          font-size: 10px;
          border-radius: 20px;
          background: var(--color-primary-soft);
          padding: 2px 6px;
        }
      }
    }
    .notes-sidebar-foot {
      display: flex;
      align-items: center;
      gap: 5px;
      padding: 10px 5px;
      margin: auto 0 0;
      color: var(--color-text-muted);
      font-size: 9px;
    }
  }
  .notes-main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    gap: 18px;
    padding: 23px 25px;
    border: 1px solid var(--color-line);
    border-radius: 20px;
    background: linear-gradient(
      125deg,
      var(--color-panel),
      var(--color-panel-soft)
    );
    overflow: auto;
    &.notes-main-calendar {
      padding: 0;
      border: 0;
      border-radius: 0;
      overflow: hidden;
      background: transparent;
    }
    .notes-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      flex: none;
      gap: 15px;
      padding: 0 0 2px;
      .notes-eyebrow {
        font-size: 10px;
        letter-spacing: 1.6px;
        color: var(--color-text-muted);
      }
      .notes-title {
        margin: 10px 0 8px;
        font-size: 27px;
        line-height: 1.2;
      }
      .notes-subtitle {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 12px;
      }
      .notes-header-actions {
        display: flex;
        gap: 10px;
      }
    }
    .notes-error {
      display: flex;
      align-items: center;
      justify-content: space-between;
      margin: 0;
      color: var(--color-danger);
      font-size: 12px;
    }
    .notes-filters {
      display: flex;
      align-items: center;
      flex: none;
      gap: 10px;
      .notes-search {
        display: flex;
        align-items: center;
        flex: 1;
        min-width: 100px;
        gap: 9px;
        padding: 9px 12px;
        border: 1px solid var(--color-line);
        border-radius: 11px;
        color: var(--color-text-muted);
        background: var(--color-panel);
        :deep(input) {
          flex: 1;
          min-width: 0;
          width: 100%;
          border: 0;
          outline: 0;
          font: inherit;
          font-size: 12px;
          background: transparent;
          color: var(--color-text);
        }
      }
    }
    .notes-filter-dates {
      display: flex;
      align-items: center;
      flex: none;
      gap: 13px;
      font-size: 12px;
      color: var(--color-text-muted);
      :deep(input) {
        padding: 8px;
        border: 1px solid var(--color-line);
        border-radius: 9px;
        font: inherit;
        color: var(--color-text-muted);
        background: var(--color-panel);
      }
      .notes-caption {
        margin-left: auto;
      }
    }
    .notes-bulk {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 12px;
      padding: 8px 12px;
      background: var(--color-primary-soft);
      border-radius: 9px;
    }
    .notes-pending {
      flex: none;
      padding: 15px 17px;
      border: 1px solid var(--color-warning-line);
      border-radius: 12px;
      background: linear-gradient(
        115deg,
        var(--color-warning-soft),
        var(--color-panel)
      );
      .notes-section-title {
        display: flex;
        gap: 8px;
        align-items: center;
        color: var(--color-warning);
        font-size: 13px;
      }
      .notes-pending-item {
        display: flex;
        align-items: center;
        gap: 12px;
        padding-top: 12px;
        :deep(div) {
          flex: 1;
          min-width: 0;
          display: flex;
          flex-direction: column;
          gap: 4px;
          font-size: 12px;
          overflow-wrap: anywhere;
        }
      }
    }
    .notes-workspace {
      display: flex;
      flex-direction: column;
      gap: 20px;
      &.notes-display-calendar {
        flex: 1;
        min-height: 0;
      }
      .notes-group {
        .notes-group-title {
          display: flex;
          align-items: center;
          gap: 9px;
          margin: 0 0 13px;
          font-size: 13px;
          :deep(span) {
            font-size: 10px;
            color: var(--color-text-muted);
          }
        }
        .notes-cards {
          display: flex;
          flex-wrap: wrap;
          align-items: flex-start;
          gap: 16px;
        }
      }
      &.notes-display-list {
        .notes-group {
          .notes-cards {
            flex-direction: column;
            :deep(.quick-note-card) {
              width: 100%;
              max-width: none;
              flex: auto;
            }
            :deep(.note-card-body) {
              -webkit-line-clamp: 2;
            }
          }
        }
      }
      &.notes-display-timeline {
        .notes-group {
          padding-left: 20px;
          border-left: 1px solid var(--color-line-strong);
        }
      }
      .notes-load {
        align-self: center;
        margin: 8px;
      }
    }
    .notes-empty {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      min-height: 220px;
      gap: 13px;
      color: var(--color-text-muted);
    }
    .notes-reminders {
      .notes-reminder-intro {
        display: flex;
        align-items: center;
        gap: 10px;
        padding: 15px;
        border-radius: 10px;
        background: var(--color-primary-soft);
        color: var(--color-primary);
        font-size: 12px;
      }
      .notes-event {
        display: flex;
        align-items: center;
        gap: 13px;
        padding: 18px 10px;
        border-bottom: 1px solid var(--color-line);
        &.notes-event-unread {
          border-left: 2px solid var(--color-primary);
        }
        .notes-event-body {
          display: flex;
          flex-direction: column;
          flex: 1;
          gap: 5px;
        }
      }
    }
  }
  .notes-organizer {
    display: flex;
    flex-direction: column;
    gap: 15px;
    .notes-folder-list {
      max-height: 260px;
      overflow: auto;
      .notes-folder-row {
        display: flex;
        align-items: center;
        gap: 9px;
        width: 100%;
        padding: 11px;
        border: 0;
        border-bottom: 1px solid var(--color-line);
        background: transparent;
        color: var(--color-text);
        font: inherit;
        text-align: left;
        cursor: pointer;
        .notes-caption {
          margin-left: auto;
        }
      }
    }
    .notes-folder-form {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 10px;
      :deep(input:not([type="checkbox"])) {
        flex: 1;
        min-width: 0;
        padding: 9px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        color: var(--color-text);
        background: var(--color-panel);
        font: inherit;
      }
    }
    .notes-error {
      color: var(--color-danger);
    }
  }
}
</style>
