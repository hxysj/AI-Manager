<template>
  <article
    class="quick-note-card"
    :class="[
      `note-color-${note.color || 'sand'}`,
      { 'note-selected': selected, 'note-summarizing': summaryCapture }
    ]"
  >
    <div class="note-card-head">
      <label class="note-card-select"
        ><input
          type="checkbox"
          :checked="selected"
          aria-label="选择便签"
          @change="$emit('select', note.id)"
        /><span class="note-type-badge"
          ><NotebookPen :size="12" />{{ typeName }}</span
        ></label
      ><span class="note-card-location"
        ><Folder :size="13" />{{ groupName
        }}<span v-if="categoryName && categoryName !== '未分类'"
          >· {{ categoryName }}</span
        ></span
      ><button
        class="note-card-icon"
        type="button"
        :title="note.pinned ? '取消置顶' : '置顶'"
        @click="$emit('pin', note)"
      >
        <Pin :size="13" :fill="note.pinned ? 'currentColor' : 'none'" />
      </button>
    </div>
    <div v-if="summaryCapture" class="note-card-summary">
      <span class="note-card-summary-status" role="status">
        <Sparkles :size="14" aria-hidden="true" />正在生成工作小结…
      </span>
      <button
        v-if="summaryCapture.id"
        class="note-card-summary-cancel"
        type="button"
        @click="$emit('cancel-summary', summaryCapture)"
      >
        取消生成
      </button>
    </div>
    <button class="note-card-open" type="button" @click="$emit('open', note)">
      <span class="note-card-title">{{ note.title }}</span
      ><span class="note-card-body">{{ note.content }}</span>
    </button>
    <div v-if="workStatus" class="note-card-tracking">
      <span class="note-card-status" :data-status="note.workTracking.status">
        {{ workStatus }}
      </span>
      <template v-if="note.workTracking.progress != null">
        <progress
          :value="note.workTracking.progress"
          max="100"
          aria-label="工作完成进度"
        />
        <span>{{ note.workTracking.progress }}%</span>
      </template>
    </div>
    <div
      v-if="note.type === 'worklog' && note.worklog?.result"
      class="note-card-result"
    >
      <span>结果</span>{{ note.worklog.result }}
    </div>
    <div v-if="note.tasks?.length" class="note-card-tasks">
      <label
        v-for="task in note.tasks.slice(0, 4)"
        :key="task.id"
        class="note-card-task"
        :class="{ 'note-task-done': task.status === 'completed' }"
        ><input
          type="checkbox"
          :checked="task.status === 'completed'"
          :disabled="note.lifecycle !== 'active' || task.status === 'cancelled'"
          @change="$emit('task', note, task)"
        /><span>{{ task.title }}</span
        ><span v-if="task.dueDate" class="note-task-date">{{
          task.dueDate
        }}</span></label
      ><span v-if="note.tasks.length > 4" class="note-card-more"
        >另有 {{ note.tasks.length - 4 }} 项待办</span
      >
    </div>
    <p v-if="note.pendingTime" class="note-card-pending">
      <Clock3 :size="12" />提醒时间待确认
    </p>
    <div v-if="note.tags?.length" class="note-card-tags">
      <span v-for="tag in [...new Set(note.tags)].slice(0, 4)" :key="tag"
        >#{{ tag }}</span
      >
    </div>
    <footer class="note-card-footer">
      <span
        class="note-card-date"
        :title="`记录于 ${formatDateTime(note.createdAt)}`"
        ><CalendarDays :size="13" />{{
          note.occurredOn ||
          new Date(note.createdAt).toLocaleDateString("sv-SE")
        }}</span
      ><button
        v-if="note.lifecycle === 'active'"
        class="note-card-reminder"
        type="button"
        :aria-label="nextReminder ? '管理提醒' : '添加提醒'"
        @click="$emit('reminder', note)"
      >
        <Bell :size="14" />{{
          nextReminder ? formatDateTime(nextReminder).slice(5, 16) : "添加提醒"
        }}</button
      ><button
        class="note-card-detail-button"
        type="button"
        title="编辑便签"
        @click="$emit('open', note)"
      >
        查看详情<ArrowUpRight :size="14" />
      </button>
    </footer>
  </article>
</template>

<script setup>
import { computed } from "vue"
import {
  ArrowUpRight,
  Bell,
  Clock3,
  Pin,
  NotebookPen,
  Folder,
  CalendarDays,
  Sparkles
} from "lucide-vue-next"
import { noteTypes, workStateLabel } from "@/features/quick-notes/constant"
import { formatDateTime } from "@/utils/formatters"
const props = defineProps({
  note: { type: Object, required: true },
  folders: { type: Array, default: () => [] },
  nextReminder: { type: Number, default: null },
  selected: Boolean,
  summaryCapture: { type: Object, default: null }
})
defineEmits(["open", "select", "pin", "task", "reminder", "cancel-summary"])
// 卡片只渲染公开记录，操作交给工作区统一保存和处理版本冲突。
const typeName = computed(
  () => noteTypes.find((type) => type.id === props.note.type)?.label || "便签"
)
const workStatus = computed(() =>
  props.note.type === "worklog" && props.note.workTracking
    ? workStateLabel(props.note.workTracking)
    : ""
)
const groupName = computed(
  () =>
    props.folders.find((folder) => folder.id === props.note.groupId)?.name ||
    "收件箱"
)
const categoryName = computed(
  () =>
    props.folders.find((folder) => folder.id === props.note.categoryId)?.name ||
    "未分类"
)
</script>

<style scoped lang="less">
.quick-note-card {
  --note-accent: #7394b3;
  display: flex;
  flex-direction: column;
  gap: 14px;
  width: 100%;
  flex: none;
  max-width: 100%;
  padding: 21px 23px 16px;
  border: 1px solid var(--color-line);
  border-radius: 12px;
  background: var(--color-panel);
  box-shadow: 0 2px 8px #18304703;
  transition: border-color 0.15s;
  &.note-color-mint {
    --note-accent: #63a187;
  }
  &.note-color-sky {
    --note-accent: #699ab7;
  }
  &.note-color-rose {
    --note-accent: #c78c91;
  }
  &.note-color-lavender {
    --note-accent: #9990b6;
  }
  &.note-selected {
    border-color: var(--color-primary);
  }
  &.note-summarizing {
    border-color: var(--color-primary);
    animation: note-summary-pulse 2s ease-in-out infinite;
  }
  .note-card-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 7px;
    color: var(--color-primary);
    background: var(--color-primary-soft);
    font-size: 12px;
    .note-card-summary-status {
      display: inline-flex;
      align-items: center;
      gap: 6px;
    }
    .note-card-summary-cancel {
      flex: none;
      padding: 2px 0 2px 5px;
      border: 0;
      color: inherit;
      background: transparent;
      font: inherit;
      cursor: pointer;
      &:hover {
        text-decoration: underline;
      }
    }
  }
  .note-card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    .note-card-select {
      display: inline-flex;
      align-items: center;
      gap: 9px;
      color: var(--color-text-muted);
      font-size: 12px;
    }
    .note-card-location {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      margin-left: auto;
      color: var(--color-text-muted);
      font-size: 11px;
    }
  }
  .note-type-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 0;
    border-radius: 0;
    background: transparent;
    color: var(--note-accent);
  }
  .note-card-icon {
    display: inline-flex;
    align-items: center;
    padding: 2px;
    border: 0;
    color: var(--color-text-muted);
    background: transparent;
    cursor: pointer;
    &:hover {
      color: var(--color-primary);
    }
  }
  .note-card-open {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-text);
    text-align: left;
    font: inherit;
    cursor: pointer;
    .note-card-title {
      font-size: 18px;
      line-height: 1.6;
      overflow-wrap: anywhere;
    }
    .note-card-body {
      display: -webkit-box;
      -webkit-line-clamp: 3;
      -webkit-box-orient: vertical;
      overflow: hidden;
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      line-height: 1.8;
      font-size: 14px;
      color: var(--color-text-muted);
    }
  }
  .note-card-result {
    display: flex;
    gap: 6px;
    font-size: 12px;
    line-height: 1.6;
    :deep(span) {
      color: var(--note-accent);
      flex: none;
    }
  }
  .note-card-tracking {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--color-text-muted);
    font-size: 12px;
    .note-card-status {
      padding: 3px 7px;
      border-radius: 5px;
      background: var(--color-panel-soft);
      &[data-status="in_progress"] {
        color: var(--color-primary);
      }
      &[data-status="blocked"] {
        color: var(--color-warning);
      }
    }
    progress {
      flex: 1;
      min-width: 0;
      height: 6px;
      accent-color: var(--color-primary);
    }
  }
  .note-card-tasks {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 11px 13px;
    border-radius: 8px;
    background: var(--color-panel-soft);
    .note-card-task {
      display: flex;
      align-items: center;
      gap: 7px;
      font-size: 13px;
      line-height: 1.5;
      &.note-task-done {
        text-decoration: line-through;
        color: var(--color-text-muted);
      }
      .note-task-date {
        margin-left: auto;
        font-size: 10px;
        color: var(--color-text-muted);
      }
    }
    .note-card-more {
      color: var(--color-text-muted);
      font-size: 11px;
    }
  }
  .note-card-pending {
    display: flex;
    gap: 5px;
    margin: 0;
    color: var(--color-warning);
    font-size: 12px;
  }
  .note-card-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    :deep(span) {
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      padding: 4px 9px;
      border-radius: 5px;
      font-size: 11px;
    }
  }
  .note-card-footer {
    display: flex;
    align-items: center;
    gap: 12px;
    border-top: 1px solid var(--color-line);
    padding-top: 10px;
    color: var(--color-text-muted);
    font-size: 12px;
    .note-card-date {
      display: inline-flex;
      align-items: center;
      gap: 7px;
    }
    .note-card-reminder {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      margin-left: auto;
      padding: 6px 10px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel);
      font: inherit;
      color: var(--color-primary);
      white-space: nowrap;
      cursor: pointer;
    }
    .note-card-detail-button {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 6px 0 6px 4px;
      border: 0;
      font: inherit;
      background: transparent;
      color: var(--color-text-muted);
      cursor: pointer;
      &:hover {
        color: var(--color-primary);
      }
    }
  }
}
@keyframes note-summary-pulse {
  0%,
  100% {
    border-color: var(--color-primary);
    box-shadow: 0 2px 8px #18304703;
  }
  50% {
    border-color: color-mix(
      in srgb,
      var(--color-primary) 45%,
      var(--color-line)
    );
    box-shadow: 0 0 0 3px var(--color-primary-soft);
  }
}
</style>
