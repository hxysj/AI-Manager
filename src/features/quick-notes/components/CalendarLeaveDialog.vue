<template>
  <BaseModal
    class="calendar-leave-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="calendar-leave-title"
    @close="close"
    @keydown.esc.stop="close"
  >
    <template #header>
      <h2
        id="calendar-leave-title"
        ref="titleElement"
        class="leave-title"
        tabindex="-1"
      >
        <CalendarClock :size="19" />
        {{ rule ? "编辑请假" : "设置请假" }}
      </h2>
      <button
        class="leave-close"
        type="button"
        aria-label="关闭请假"
        :disabled="busy"
        @click="close"
      >
        <X :size="16" />
      </button>
    </template>
    <form class="leave-form" @submit.prevent="save">
      <div class="leave-content">
        <fieldset class="leave-fields" :disabled="busy">
          <div class="leave-options" role="group" aria-label="请假方式">
            <button
              class="leave-option"
              type="button"
              :aria-pressed="!timed"
              @click="timed = false"
            >
              全天
            </button>
            <button
              class="leave-option"
              type="button"
              :aria-pressed="timed"
              @click="timed = true"
            >
              时间范围
            </button>
          </div>
          <div class="leave-range">
            <div class="leave-row">
              <label class="leave-field">
                开始日期
                <input
                  v-model="startDate"
                  class="leave-input"
                  type="date"
                  required
                />
              </label>
              <label v-if="timed" class="leave-field leave-time">
                开始时间
                <input
                  v-model="startTime"
                  class="leave-input"
                  type="time"
                  required
                />
              </label>
            </div>
            <div class="leave-row">
              <label class="leave-field">
                结束日期
                <input
                  v-model="endDate"
                  class="leave-input"
                  type="date"
                  :min="startDate"
                  required
                />
              </label>
              <label v-if="timed" class="leave-field leave-time">
                结束时间
                <input
                  v-model="endTime"
                  class="leave-input"
                  type="time"
                  required
                />
              </label>
            </div>
            <p v-if="!timed && startDate !== endDate" class="leave-caption">
              首尾两天均计入请假。
            </p>
          </div>
        </fieldset>
        <p v-if="error" class="leave-error" role="alert">{{ error }}</p>
      </div>
      <footer class="leave-actions">
        <button
          v-if="rule"
          class="leave-button leave-remove"
          type="button"
          :disabled="busy"
          @click="remove"
        >
          <Trash2 :size="14" />删除请假
        </button>
        <button
          class="leave-button leave-cancel"
          type="button"
          :disabled="busy"
          @click="close"
        >
          取消
        </button>
        <button
          class="leave-button leave-primary"
          type="submit"
          :disabled="busy"
        >
          <Check :size="14" />{{ busy ? "处理中…" : "保存请假" }}
        </button>
      </footer>
    </form>
  </BaseModal>
</template>

<script setup>
import { onMounted, ref, watch } from "vue"
import { CalendarClock, Check, Trash2, X } from "lucide-vue-next"
import BaseModal from "@/components/BaseModal.vue"
import { quickNotesApi } from "@/api/modules/quick-notes"

const props = defineProps({
  date: { type: String, required: true },
  rule: { type: Object, default: null }
})
const emit = defineEmits(["close"])
const startDate = ref(props.rule?.startDate || props.date)
const endDate = ref(props.rule?.endDate || startDate.value)
const startTime = ref(props.rule?.startTime || "")
const endTime = ref(props.rule?.endTime || "")
const timed = ref(!!startTime.value)
const busy = ref(false)
const error = ref("")
const titleElement = ref(null)

onMounted(() => titleElement.value?.focus({ preventScroll: true }))
// 单日请假调整开始日期时，结束日期同步移动。
watch(startDate, (date, previous) => {
  if (endDate.value === previous || endDate.value < date) endDate.value = date
})

function close() {
  if (!busy.value) emit("close")
}

async function write(action, payload) {
  if (busy.value) return
  busy.value = true
  error.value = ""
  try {
    await quickNotesApi.invoke(action, payload)
    emit("close")
  } catch (failure) {
    error.value = failure.message || String(failure)
  } finally {
    busy.value = false
  }
}

function save() {
  if (
    timed.value &&
    startDate.value === endDate.value &&
    endTime.value <= startTime.value
  ) {
    error.value = "结束时间须晚于开始时间。"
    return
  }
  write("calendar-rule-save", {
    ...(props.rule ? { id: props.rule.id, revision: props.rule.revision } : {}),
    title: props.rule?.title || "请假",
    kind: "leave",
    startDate: startDate.value,
    endDate: endDate.value === startDate.value ? "" : endDate.value,
    startTime: timed.value ? startTime.value : "",
    endTime: timed.value ? endTime.value : ""
  })
}

function remove() {
  write("calendar-rule-delete", {
    id: props.rule.id,
    revision: props.rule.revision
  })
}
</script>

<style scoped lang="less">
.calendar-leave-modal {
  display: flex;
  justify-content: flex-end;
  padding: 0;

  :deep(> .base-modal__panel) {
    width: min(356px, calc(100vw - 24px));
    height: 100%;
    max-height: 100%;
    border-radius: 19px 0 0 19px;
    .base-modal__header {
      display: flex;
      flex: none;
      align-items: center;
      padding: 15px 17px;
      border-bottom: 1px solid var(--color-line);
    }
    .base-modal__content {
      padding: 0;
    }
  }
  .leave-title {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font: inherit;
    font-size: var(--font-size-lg);
    &:focus {
      outline: none;
    }
  }
  .leave-close,
  .leave-button {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 32px;
    padding: 6px 10px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text);
    font: inherit;
    font-size: 12px;
    white-space: nowrap;
    cursor: pointer;
    &:hover {
      border-color: var(--color-primary);
      color: var(--color-primary);
    }
    &:disabled {
      opacity: 0.5;
      cursor: default;
    }
  }
  .leave-close {
    width: 32px;
    padding: 0;
    color: var(--color-text-muted);
  }
  .leave-form {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    .leave-content {
      flex: 1;
      min-height: 0;
      overflow: auto;
      padding: 16px 17px;
      .leave-fields {
        display: flex;
        flex-direction: column;
        gap: 16px;
        min-width: 0;
        margin: 0;
        padding: 0;
        border: 0;
        .leave-options {
          display: flex;
          align-self: flex-start;
          gap: 2px;
          padding: 3px;
          border: 1px solid var(--color-line);
          border-radius: 10px;
          background: var(--color-panel-soft);
          .leave-option {
            padding: 6px 11px;
            border: 0;
            border-radius: 7px;
            background: transparent;
            color: var(--color-text-muted);
            font: inherit;
            font-size: 12px;
            cursor: pointer;
            &:hover,
            &[aria-pressed="true"] {
              color: var(--color-primary);
            }
            &[aria-pressed="true"] {
              background: var(--color-primary-soft);
            }
          }
        }
        .leave-range {
          display: grid;
          gap: 12px;
          .leave-row {
            display: flex;
            flex-wrap: wrap;
            gap: 8px 10px;
            .leave-field {
              display: flex;
              flex: 1 0 142px;
              flex-direction: column;
              min-width: 0;
              gap: 6px;
              color: var(--color-text-muted);
              font-size: var(--font-size-sm);
              &.leave-time {
                flex: 0 1 112px;
              }
              .leave-input {
                width: 100%;
                min-width: 0;
                height: 34px;
                padding: 5px 8px;
                border: 1px solid var(--color-line);
                border-radius: 8px;
                background: var(--color-panel);
                color: var(--color-text);
                font: inherit;
                &:focus {
                  border-color: var(--color-primary);
                  outline: none;
                  box-shadow: 0 0 0 2px var(--color-primary-soft);
                }
              }
            }
          }
          .leave-caption {
            margin: 0;
            color: var(--color-text-muted);
            font-size: var(--font-size-xs);
          }
        }
      }
      .leave-error {
        margin: 12px 0 0;
        color: var(--color-danger);
        font-size: var(--font-size-sm);
        overflow-wrap: anywhere;
      }
    }
    .leave-actions {
      display: flex;
      flex: none;
      flex-wrap: wrap;
      align-items: center;
      gap: 8px;
      padding: 12px 17px;
      border-top: 1px solid var(--color-line);
      background: var(--color-panel-soft);
      .leave-button {
        &.leave-cancel {
          margin-left: auto;
        }
        &.leave-primary {
          border-color: var(--color-primary-solid);
          background: var(--color-primary-solid);
          color: #fff;
          &:hover {
            background: var(--color-primary);
          }
        }
        &.leave-remove {
          padding-inline: 3px;
          border: 0;
          background: transparent;
          color: var(--color-text-muted);
        }
        &.leave-remove:hover {
          color: var(--color-danger);
        }
      }
    }
  }
}
</style>
