<template>
  <article
    class="quick-note-card"
    :class="[
      `note-color-${note.color || 'sand'}`,
      { 'note-selected': selected }
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
      ><button
        class="note-card-icon"
        type="button"
        :title="note.pinned ? '取消置顶' : '置顶'"
        @click="$emit('pin', note)"
      >
        <Pin :size="13" :fill="note.pinned ? 'currentColor' : 'none'" />
      </button>
    </div>
    <button class="note-card-open" type="button" @click="$emit('open', note)">
      <span class="note-card-title">{{ note.title }}</span
      ><span class="note-card-body">{{ note.content }}</span>
    </button>
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
    <div class="note-card-tags">
      <span>#{{ groupName }}</span
      ><span v-if="categoryName && categoryName !== '未分类'">{{
        categoryName
      }}</span
      ><span v-for="tag in (note.tags || []).slice(0, 3)" :key="tag"
        >#{{ tag }}</span
      >
    </div>
    <footer class="note-card-footer">
      <span :title="`记录于 ${formatDateTime(note.createdAt)}`">{{
        note.occurredOn || formatDateTime(note.createdAt).slice(0, 10)
      }}</span
      ><button
        v-if="note.lifecycle === 'active'"
        class="note-card-reminder"
        type="button"
        :aria-label="nextReminder ? '管理提醒' : '添加提醒'"
        @click="$emit('reminder', note)"
      >
        <Bell :size="12" />{{
          nextReminder ? formatDateTime(nextReminder).slice(5, 16) : "添加提醒"
        }}</button
      ><button
        class="note-card-icon"
        type="button"
        title="编辑便签"
        @click="$emit('open', note)"
      >
        <ArrowUpRight :size="15" />
      </button>
    </footer>
  </article>
</template>

<script setup>
import { computed } from "vue"
import { ArrowUpRight, Bell, Clock3, Pin, NotebookPen } from "lucide-vue-next"
import { noteTypes } from "@/features/quick-notes/constant"
import { formatDateTime } from "@/utils/formatters"
const props = defineProps({
  note: { type: Object, required: true },
  folders: { type: Array, default: () => [] },
  nextReminder: { type: Number, default: null },
  selected: Boolean
})
defineEmits(["open", "select", "pin", "task", "reminder"])
// 卡片只渲染公开记录，操作交给工作区统一保存和处理版本冲突。
const typeName = computed(
  () => noteTypes.find((type) => type.id === props.note.type)?.label || "便签"
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
  --note-accent: #b68c3b;
  display: flex;
  flex-direction: column;
  gap: 13px;
  width: 520px;
  flex: 0 1 560px;
  max-width: 100%;
  padding: 20px 23px 17px;
  border: 1px solid var(--color-line);
  border-top: 3px solid var(--note-accent);
  border-radius: 15px;
  background: linear-gradient(
    135deg,
    var(--color-panel),
    var(--color-panel-soft)
  );
  box-shadow: 0 3px 12px #18304706;
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
  .note-card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    .note-card-select {
      display: inline-flex;
      align-items: center;
      gap: 7px;
      color: var(--color-text-muted);
      font-size: 11px;
    }
  }
  .note-type-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border-radius: 20px;
    background: var(--color-primary-soft);
    color: var(--color-primary);
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
    gap: 10px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-text);
    text-align: left;
    font: inherit;
    cursor: pointer;
    .note-card-title {
      font-size: 17px;
      line-height: 1.6;
      overflow-wrap: anywhere;
    }
    .note-card-body {
      display: -webkit-box;
      -webkit-line-clamp: 4;
      -webkit-box-orient: vertical;
      overflow: hidden;
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      line-height: 1.8;
      font-size: 12px;
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
  .note-card-tasks {
    display: flex;
    flex-direction: column;
    gap: 8px;
    .note-card-task {
      display: flex;
      align-items: center;
      gap: 7px;
      font-size: 12px;
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
    font-size: 11px;
  }
  .note-card-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: auto;
    :deep(span) {
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      padding: 4px 9px;
      border-radius: 20px;
      font-size: 10px;
    }
  }
  .note-card-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    border-top: 1px solid var(--color-line);
    padding-top: 10px;
    color: var(--color-text-muted);
    font-size: 10px;
    .note-card-reminder {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      margin-left: auto;
      padding: 0;
      border: 0;
      background: transparent;
      font: inherit;
      color: var(--color-primary);
      white-space: nowrap;
      cursor: pointer;
    }
    .note-card-icon {
      margin-left: auto;
    }
  }
}
</style>
