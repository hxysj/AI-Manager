<template>
  <div class="provider-icon-picker">
    <button
      class="icon-avatar"
      type="button"
      aria-label="选择服务商图标"
      :aria-expanded="expanded"
      @click="expanded = !expanded"
    >
      <AiIcon
        v-if="modelValue"
        class="avatar-image"
        :name="modelValue"
        :alt="`${name || 'Provider'} 图标`"
      />
      <template v-else>{{ name.slice(0, 1) || "AI" }}</template>
    </button>
    <span class="icon-name">{{
      modelValue ? iconLabel(modelValue) : "点击选择图标"
    }}</span>
    <section v-if="expanded" class="icon-panel">
      <div class="icon-actions">
        <label class="icon-upload">
          <Upload :size="16" />上传图标
          <input
            class="icon-file"
            type="file"
            accept="image/svg+xml,image/png,image/jpeg,image/webp"
            @change="uploadCustomIcon"
          />
        </label>
        <button
          v-if="modelValue"
          class="icon-clear"
          type="button"
          @click="selectIcon('')"
        >
          清除图标
        </button>
      </div>
      <input
        v-model.trim="keyword"
        class="input"
        aria-label="搜索图标"
        placeholder="输入图标名称..."
      />
      <div class="icon-grid">
        <button
          v-for="icon in filteredIcons"
          :key="icon"
          class="icon-option"
          :class="{ 'icon-option-active': modelValue === icon }"
          type="button"
          :aria-pressed="modelValue === icon"
          @click="selectIcon(icon)"
        >
          <AiIcon
            class="option-image"
            :name="icon"
            :alt="`${iconLabel(icon)} 图标`"
          />
          <span class="option-name">{{ iconLabel(icon) }}</span>
        </button>
      </div>
    </section>
  </div>
</template>

<script setup>
import { computed, ref } from "vue"
import { Upload } from "lucide-vue-next"
import AiIcon from "@/components/AiIcon.vue"
import { createMessage } from "@/utils/message"

defineProps({
  modelValue: { type: String, default: "" },
  name: { type: String, default: "" }
})
const emit = defineEmits(["update:modelValue"])
const expanded = ref(false)
const keyword = ref("")
const iconModules = import.meta.glob("/src/assets/ai-icons/*.svg", {
  query: "?url",
  import: "default"
})
const icons = Object.keys(iconModules)
  .map((item) => item.split("/").pop())
  .sort((left, right) => left.localeCompare(right))
const filteredIcons = computed(() =>
  icons.filter((icon) =>
    iconLabel(icon).toLowerCase().includes(keyword.value.toLowerCase())
  )
)

function iconLabel(icon) {
  return /^(data:|https?:\/\/|file:|blob:)/i.test(icon)
    ? "自定义图标"
    : icon.replace(/\.svg$/, "")
}

function selectIcon(icon) {
  emit("update:modelValue", icon)
  expanded.value = false
}

function uploadCustomIcon(event) {
  const input = event.target
  const file = input.files?.[0]
  if (!file) return
  if (
    !["image/svg+xml", "image/png", "image/jpeg", "image/webp"].includes(
      file.type
    )
  ) {
    createMessage.error("请选择 SVG、PNG、JPG 或 WebP 图标。")
    input.value = ""
    return
  }
  if (file.size > 1024 * 1024) {
    createMessage.error("图标文件不能超过 1MB。")
    input.value = ""
    return
  }
  const reader = new FileReader()
  reader.onload = () => {
    selectIcon(String(reader.result || ""))
    input.value = ""
  }
  reader.onerror = () => {
    createMessage.error("图标读取失败。")
    input.value = ""
  }
  reader.readAsDataURL(file)
}
</script>

<style scoped lang="less">
.provider-icon-picker {
  display: flex;
  align-items: center;
  flex-direction: column;
  gap: 10px;

  .icon-avatar {
    display: flex;
    width: 78px;
    height: 78px;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel-soft);
    color: var(--color-text);
    font-size: var(--font-size-xl);
    cursor: pointer;

    .avatar-image {
      width: 48px;
      height: 48px;
    }
  }

  .icon-name {
    color: var(--color-text-muted);
    font-size: var(--font-size-base);
  }

  .icon-panel {
    display: flex;
    width: 100%;
    box-sizing: border-box;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel-soft);

    .icon-actions {
      display: flex;
      align-items: center;
      gap: 10px;

      .icon-upload,
      .icon-clear {
        display: inline-flex;
        align-items: center;
        gap: 7px;
        padding: 8px 12px;
        border: 1px solid var(--color-info-line);
        border-radius: 8px;
        background: var(--color-primary-soft);
        color: var(--color-primary);
        font: inherit;
        cursor: pointer;

        .icon-file {
          display: none;
        }
      }
    }

    .icon-grid {
      display: flex;
      max-height: 280px;
      overflow: auto;
      flex-wrap: wrap;
      gap: 10px;

      .icon-option {
        display: flex;
        flex: 0 0 132px;
        min-width: 0;
        height: 86px;
        align-items: center;
        justify-content: center;
        flex-direction: column;
        gap: 8px;
        padding: 8px 6px;
        border: 1px solid transparent;
        border-radius: 8px;
        background: transparent;
        color: var(--color-text-muted);
        cursor: pointer;

        &.icon-option-active {
          border-color: var(--color-primary-solid);
          background: var(--color-primary-soft);
          color: var(--color-text);
        }

        .option-image {
          width: 30px;
          height: 30px;
          flex: none;
        }

        .option-name {
          overflow: hidden;
          width: 100%;
          font-size: var(--font-size-sm);
          text-align: center;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }
    }
  }
}
</style>
