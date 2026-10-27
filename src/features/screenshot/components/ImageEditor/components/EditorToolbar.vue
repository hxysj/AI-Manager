<template>
  <Teleport to="body">
    <div
      class="screenshot-toolbar"
      :style="toolbarStyle"
      @pointerdown.stop
      @pointerenter="$emit('hover', true)"
      @pointerleave="$emit('hover', false)"
    >
      <div v-if="tool !== 'select'" class="toolbar-options">
        <div v-if="!['eraser', 'mosaic'].includes(tool)" class="toolbar-group">
          <span class="toolbar-label">颜色</span>
          <button
            v-for="preset in colorPresets"
            :key="preset"
            type="button"
            class="toolbar-color-preset"
            :class="{ active: color === preset }"
            :style="{ backgroundColor: preset }"
            :title="`颜色 ${preset}`"
            :aria-label="`颜色 ${preset}`"
            :disabled="busy || outputting"
            @click="$emit('update:color', preset)"
          />
          <input
            class="toolbar-color"
            type="color"
            title="自定义颜色"
            aria-label="自定义颜色"
            :value="color"
            :disabled="busy || outputting"
            @input="$emit('update:color', $event.target.value)"
          />
        </div>
        <div class="toolbar-group">
          <span class="toolbar-label">{{
            tool === "text" ? "字号" : "线宽"
          }}</span>
          <select
            class="toolbar-select"
            :value="width"
            :aria-label="tool === 'text' ? '字号' : '线宽'"
            :disabled="busy || outputting"
            @change="$emit('update:width', Number($event.target.value))"
          >
            <option v-if="!lineWidths.includes(width)" :value="width">
              {{ tool === "text" ? Math.max(14, width * 7) : width }} px
            </option>
            <option v-for="size in lineWidths" :key="size" :value="size">
              {{ tool === "text" ? Math.max(14, size * 7) : size }} px
            </option>
          </select>
        </div>
        <template v-if="['rectangle', 'ellipse', 'line'].includes(tool)">
          <div
            v-for="control in lineControls"
            v-show="control.id === 'style' || tool === 'line'"
            :key="control.id"
            class="toolbar-group"
          >
            <div
              class="toolbar-dropdown"
              @keydown.stop
              @keydown.esc.prevent="dropdown = ''"
              @focusout="closeDropdownOnBlur"
            >
              <button
                type="button"
                class="toolbar-dropdown-toggle"
                :title="control.label"
                :aria-label="control.label"
                :aria-expanded="dropdown === control.id"
                :disabled="busy || outputting"
                @click="dropdown = dropdown === control.id ? '' : control.id"
              >
                <component
                  :is="
                    lineOptions[control.id] === 'arrow' ? ArrowUpRight : Minus
                  "
                  :size="20"
                  :stroke-dasharray="
                    lineOptions[control.id] === 'dashed' ? '4 3' : undefined
                  "
                />
                <ChevronDown :size="12" />
              </button>
              <div v-if="dropdown === control.id" class="toolbar-dropdown-menu">
                <button
                  v-for="option in control.options"
                  :key="option.value"
                  type="button"
                  class="toolbar-dropdown-option"
                  :class="{
                    active: lineOptions[control.id] === option.value
                  }"
                  :title="option.label"
                  :aria-label="option.label"
                  :aria-pressed="lineOptions[control.id] === option.value"
                  :disabled="busy || outputting"
                  @click="selectLineOption(control.id, option.value)"
                >
                  <component
                    :is="option.value === 'arrow' ? ArrowUpRight : Minus"
                    :size="20"
                    :stroke-dasharray="
                      option.value === 'dashed' ? '4 3' : undefined
                    "
                  />
                </button>
              </div>
            </div>
          </div>
        </template>
      </div>
      <div class="toolbar-buttons">
        <button
          v-for="item in tools"
          :key="item.id"
          type="button"
          class="toolbar-button"
          :class="{ active: tool === item.id }"
          :title="item.label"
          :aria-label="item.label"
          :aria-pressed="tool === item.id"
          :disabled="busy || outputting"
          @click="selectTool(item.id)"
        >
          <component
            :is="
              item.id === 'line' && lineOptions.type === 'arrow'
                ? ArrowUpRight
                : item.icon
            "
            :size="17"
          />
        </button>
        <span class="toolbar-separator" />
        <button
          class="toolbar-button"
          type="button"
          title="取消 Esc"
          aria-label="取消"
          :disabled="busy || outputting"
          @click="$emit('cancel')"
        >
          <X :size="17" />
        </button>
        <button
          class="toolbar-button"
          type="button"
          title="撤销 Ctrl+Z"
          aria-label="撤销"
          :disabled="!canUndo || busy || outputting"
          @click="$emit('undo')"
        >
          <Undo2 :size="17" />
        </button>
        <button
          class="toolbar-button"
          type="button"
          title="重做 Ctrl+Shift+Z"
          aria-label="重做"
          :disabled="!canRedo || busy || outputting"
          @click="$emit('redo')"
        >
          <Redo2 :size="17" />
        </button>
        <button
          v-if="reselect"
          class="toolbar-button"
          type="button"
          title="重新选择区域"
          aria-label="重新选择区域"
          :disabled="busy || outputting"
          @click="$emit('reselect', hasAnnotations)"
        >
          <Crop :size="17" />
        </button>
        <span class="toolbar-separator" />
        <button
          v-for="action in outputs"
          :key="action.id"
          class="toolbar-button"
          type="button"
          :title="action.label"
          :aria-label="action.label"
          :disabled="busy || outputting"
          @click="$emit('output', action.id)"
        >
          <component :is="action.icon" :size="17" />
        </button>
      </div>
    </div>
  </Teleport>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from "vue"
import {
  ArrowUpRight,
  ChevronDown,
  Circle,
  Copy,
  Crop,
  Eraser,
  Grid2X2,
  Highlighter,
  Minus,
  MousePointer2,
  PenLine,
  Pin,
  Redo2,
  Save,
  ScanText,
  Square,
  Type,
  Undo2,
  X
} from "lucide-vue-next"

const props = defineProps({
  toolbarStyle: { type: Object, required: true },
  tool: { type: String, required: true },
  color: { type: String, required: true },
  width: { type: Number, required: true },
  lineOptions: { type: Object, required: true },
  busy: Boolean,
  outputting: Boolean,
  canUndo: Boolean,
  canRedo: Boolean,
  reselect: Boolean,
  hasAnnotations: Boolean
})
const emit = defineEmits([
  "update:color",
  "update:width",
  "tool",
  "line-option",
  "cancel",
  "undo",
  "redo",
  "reselect",
  "output",
  "hover"
])

const dropdown = ref("")
const tools = computed(() =>
  [
    { id: "select", label: "调整选区", icon: MousePointer2 },
    { id: "rectangle", label: "矩形", icon: Square },
    { id: "ellipse", label: "椭圆", icon: Circle },
    { id: "line", label: "直线 / 箭头", icon: Minus },
    { id: "pen", label: "画笔", icon: PenLine },
    { id: "highlighter", label: "荧光笔", icon: Highlighter },
    { id: "mosaic", label: "马赛克", icon: Grid2X2 },
    { id: "text", label: "文字", icon: Type },
    { id: "eraser", label: "橡皮擦", icon: Eraser }
  ].filter((item) => props.reselect || item.id !== "select")
)
const outputs = [
  { id: "pin", label: "贴到桌面", icon: Pin },
  { id: "ocr", label: "识别文字", icon: ScanText },
  { id: "export", label: "保存 PNG", icon: Save },
  { id: "copy", label: "复制图片 Enter", icon: Copy }
]
const colorPresets = [
  "#f43f5e",
  "#ef4444",
  "#f59e0b",
  "#facc15",
  "#22c55e",
  "#06b6d4",
  "#3b82f6",
  "#8b5cf6",
  "#ffffff",
  "#111827"
]
const lineWidths = [2, 4, 6, 10]
const lineControls = [
  {
    id: "style",
    label: "线条样式",
    options: [
      { value: "solid", label: "实线" },
      { value: "dashed", label: "虚线" }
    ]
  },
  {
    id: "type",
    label: "直线或箭头",
    options: [
      { value: "line", label: "直线" },
      { value: "arrow", label: "箭头" }
    ]
  }
]

function selectTool(value) {
  dropdown.value = ""
  emit("tool", value)
}

function selectLineOption(key, value) {
  dropdown.value = ""
  emit("line-option", key, value)
}

function closeDropdown(event) {
  if (!event.target.closest(".toolbar-dropdown")) dropdown.value = ""
}

function closeDropdownOnBlur(event) {
  if (!event.currentTarget.contains(event.relatedTarget)) dropdown.value = ""
}

onMounted(() => {
  window.addEventListener("pointerdown", closeDropdown, true)
})

onBeforeUnmount(() => {
  emit("hover", false)
  window.removeEventListener("pointerdown", closeDropdown, true)
})
</script>

<style scoped lang="less">
.screenshot-toolbar {
  position: fixed;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 0;
  padding: 2px;
  border: 1px solid #8f9aa7;
  border-radius: 1px;
  background: #f8fafc;
  color: #1f2933;
  box-shadow: 0 1px 3px #0005;
  overflow: visible;
  .toolbar-buttons {
    order: 1;
    display: flex;
    align-items: center;
    gap: 0;
    overflow-x: auto;
    max-width: 100%;
    min-height: 30px;
  }
  .toolbar-options {
    order: 2;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    min-height: 28px;
    padding: 2px 4px;
    border-top: 1px solid #b8c1cb;
    background: #eef2f6;
    .toolbar-group {
      .toolbar-select {
        flex-shrink: 0;
        height: 23px;
        min-width: 56px;
        padding: 0 3px;
        border: 1px solid #8f9aa7;
        border-radius: 0;
        background: #fff;
        color: inherit;
        color-scheme: light;
        font-size: var(--font-size-xs);
        cursor: pointer;
        &:disabled {
          opacity: 0.35;
          cursor: default;
        }
      }
      .toolbar-dropdown {
        position: relative;
        .toolbar-dropdown-toggle {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 2px;
          height: 23px;
          padding: 0 4px;
          border: 1px solid #8f9aa7;
          border-radius: 0;
          background: #fff;
          color: inherit;
          cursor: pointer;
          &:hover,
          &[aria-expanded="true"] {
            border-color: #1677d2;
            background: #d6e9fb;
          }
          &:disabled {
            opacity: 0.35;
            cursor: default;
          }
        }
        .toolbar-dropdown-menu {
          position: absolute;
          right: 0;
          bottom: calc(100% + 4px);
          z-index: 1;
          display: flex;
          padding: 3px;
          border: 1px solid #8f9aa7;
          background: #f8fafc;
          box-shadow: 0 1px 3px #0005;
          .toolbar-dropdown-option {
            display: flex;
            align-items: center;
            justify-content: center;
            width: 34px;
            height: 26px;
            padding: 0;
            border: 1px solid transparent;
            background: transparent;
            color: inherit;
            cursor: pointer;
            &:hover,
            &.active {
              border-color: #1677d2;
              background: #d6e9fb;
            }
            &:disabled {
              opacity: 0.35;
              cursor: default;
            }
          }
        }
      }
    }
  }
  .toolbar-group {
    display: flex;
    align-items: center;
    gap: 3px;
    padding-right: 5px;
    border-right: 1px solid #c8d0d8;
    &:last-child {
      padding-right: 0;
      border-right: 0;
    }
  }
  .toolbar-label {
    margin-right: 2px;
    color: #69717a;
    font-size: var(--font-size-xs);
  }
  .toolbar-button,
  .toolbar-color,
  .toolbar-color-preset,
  .toolbar-separator {
    flex-shrink: 0;
  }
  .toolbar-button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 29px;
    height: 29px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    &:hover,
    &.active {
      border-color: #1677d2;
      background: #1677d2;
      color: #fff;
    }
    &:disabled {
      opacity: 0.35;
      cursor: default;
    }
  }
  .toolbar-color {
    width: 23px;
    height: 23px;
    padding: 1px;
    border: 1px solid #8f9aa7;
    background: transparent;
  }
  .toolbar-color-preset {
    width: 15px;
    height: 15px;
    padding: 0;
    border: 1px solid #7f8994;
    border-radius: 0;
    cursor: pointer;
    &:hover,
    &.active {
      outline: 2px solid #38bdf8;
      outline-offset: 1px;
    }
    &:disabled {
      opacity: 0.35;
      cursor: default;
    }
  }
  .toolbar-separator {
    width: 1px;
    height: 23px;
    margin: 0 2px;
    background: #b8c1cb;
  }
}
</style>
