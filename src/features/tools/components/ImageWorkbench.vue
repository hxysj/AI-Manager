<template>
  <section class="image-workbench">
    <header class="conversation-toolbar">
      <button
        class="conversation-button"
        type="button"
        @click="historyOpen = !historyOpen"
      >
        会话历史
      </button>
      <span class="conversation-title" :title="currentConversation?.title">{{
        currentConversation?.title || "新对话"
      }}</span>
      <button
        class="conversation-button"
        type="button"
        :disabled="!currentConversation"
        @click="renameConversation(currentConversation)"
      >
        重命名
      </button>
      <button
        class="conversation-button"
        type="button"
        @click="newConversation"
      >
        新建对话
      </button>
    </header>
    <aside v-if="historyOpen" class="conversation-sidebar">
      <header class="history-head">
        <span>历史会话</span
        ><button
          class="history-action"
          type="button"
          @click="historyOpen = false"
        >
          收起
        </button>
      </header>
      <div class="history-list">
        <article
          v-for="conversation in conversations"
          :key="conversation.id"
          class="history-item"
          :class="{ active: conversation.id === activeConversationId }"
        >
          <button
            class="history-select"
            type="button"
            @click="selectConversation(conversation.id)"
          >
            <span class="history-title">{{ conversation.title }}</span>
            <span class="history-meta"
              >{{ conversationStats(conversation.id).rounds }} 轮 ·
              {{ formatDateTime(conversation.updatedAt) }}</span
            >
            <span
              v-if="conversationStats(conversation.id).active"
              class="history-meta"
              >{{ conversationStats(conversation.id).queued }} 排队 ·
              {{ conversationStats(conversation.id).processing }} 处理中</span
            >
          </button>
          <div class="history-actions">
            <button
              class="history-action"
              type="button"
              @click="renameConversation(conversation)"
            >
              重命名</button
            ><button
              class="history-action"
              type="button"
              @click="deleteConversation(conversation.id)"
            >
              删除
            </button>
          </div>
        </article>
      </div>
      <button class="history-clear" type="button" @click="deleteConversation()">
        清空全部历史
      </button>
    </aside>
    <div
      ref="workbenchLayout"
      class="workbench-layout"
      :class="{ 'is-resizing': resizing }"
    >
      <form
        id="image-create-panel"
        class="create-panel"
        :style="{
          flexBasis: `calc(${leftWidth}% - ${(18 * leftWidth) / 100}px)`
        }"
        @submit.prevent="submitTask"
        @dragover.prevent
        @drop.prevent="dropImages"
      >
        <header class="panel-head">
          <span data-emphasis>创建任务</span>
          <button
            class="prompt-library-button"
            type="button"
            @click="promptLibraryOpen = true"
          >
            <BookOpen :size="13" />提示词库
          </button>
        </header>
        <label class="form-field">
          <span class="field-label">官方账号</span>
          <select
            v-model="form.accountId"
            class="field-input"
            :disabled="loadingAccounts || submitting"
          >
            <option value="">
              {{ loadingAccounts ? "正在读取账号…" : "请选择官方登录账号" }}
            </option>
            <option
              v-for="account in accounts"
              :key="account.id"
              :value="account.id"
              :disabled="account.disabled || account.requiresReauth"
            >
              {{ account.email || account.id }} ·
              {{
                account.disabled
                  ? "已停用"
                  : account.requiresReauth
                    ? "需要重新登录"
                    : account.plan || "官方账号"
              }}
            </option>
          </select>
        </label>
        <p
          v-if="!loadingAccounts && !availableAccounts.length"
          class="account-hint"
        >
          请先在 Provider 页面登录或恢复 Codex 官方账号。其他 Provider
          暂不支持图片生成。
        </p>
        <section
          v-if="form.generationMode === 'web'"
          class="web-quota"
          aria-label="Web 生图额度"
          aria-live="polite"
          :aria-busy="loadingQuota"
        >
          <div class="web-quota-head">
            <span>Web 生图额度</span>
            <button
              class="quota-refresh"
              type="button"
              :disabled="loadingQuota || !form.accountId || submitting"
              @click="loadQuota"
            >
              {{ loadingQuota ? "查询中…" : "刷新额度" }}
            </button>
          </div>
          <span v-if="!form.accountId" class="web-quota-info"
            >选择账号后查询</span
          >
          <span
            v-else-if="webQuota"
            :class="{ 'quota-empty': webQuota.remaining === 0 }"
          >
            {{ quotaError ? "上次查询：" : "" }}剩余 {{ webQuota.remaining }} 次
            {{ webQuota.remaining === 0 ? "（已用尽）" : "" }}
          </span>
          <span v-else class="web-quota-info">{{
            loadingQuota ? "正在查询网页额度…" : "额度未知"
          }}</span>
          <span v-if="quotaResetLabel" class="web-quota-info">
            额度恢复：{{ quotaResetLabel }}
          </span>
          <span v-if="webQuota" class="web-quota-info">
            更新于 {{ formatDateTime(webQuota.updatedAt) }}
          </span>
          <p v-if="quotaError" class="field-error">{{ quotaError }}</p>
        </section>
        <label class="form-field prompt-field">
          <span class="field-label"
            >提示词 <span class="field-key">prompt</span></span
          >
          <textarea
            v-model="form.prompt"
            class="field-input prompt-input"
            :placeholder="
              references.length
                ? '描述你希望如何修改参考图'
                : '输入你想要生成的画面，也可直接粘贴图片'
            "
            @keydown="promptKeydown"
            @paste="pasteImages"
            maxlength="16000"
            required
          ></textarea>
        </label>
        <div class="composer-tools">
          <button
            class="composer-button"
            type="button"
            @click="settingsOpen = !settingsOpen"
          >
            {{ width }} × {{ height }} · {{ form.quality }} · {{ form.n }} 张
            <span>{{ settingsOpen ? "收起设置" : "图像设置" }}</span>
          </button>
          <button
            class="composer-button"
            type="button"
            @click="drawing = { source: '' }"
          >
            <Paintbrush :size="14" />草图
          </button>
        </div>
        <section v-if="settingsOpen" class="image-settings">
          <div class="parameter-fields">
            <label class="form-field model-field">
              <span class="field-label"
                >模型<button
                  class="model-refresh"
                  type="button"
                  :disabled="loadingModels"
                  @click="loadModels"
                >
                  {{ loadingModels ? "获取中…" : "刷新模型" }}
                </button></span
              >
              <input
                v-model.trim="form.model"
                class="field-input"
                list="image-workbench-models"
                maxlength="128"
                required
              />
              <datalist id="image-workbench-models">
                <option
                  v-for="model in imageModels"
                  :key="model.id"
                  :value="model.id"
                />
              </datalist>
            </label>
            <label class="form-field"
              ><span class="field-label">质量</span
              ><select v-model="form.quality" class="field-input">
                <option
                  v-for="option in qualityOptions"
                  :key="option.value"
                  :value="option.value"
                >
                  {{ option.label }}
                </option>
              </select></label
            >
            <label class="form-field"
              ><span class="field-label">宽度 W</span
              ><input
                v-model.number="width"
                class="field-input"
                type="number"
                min="1"
                step="1"
                required
            /></label>
            <label class="form-field"
              ><span class="field-label">高度 H</span
              ><input
                v-model.number="height"
                class="field-input"
                type="number"
                min="1"
                step="1"
                required
            /></label>
          </div>
          <div class="preset-list">
            <button
              v-for="preset in sizePresets"
              :key="preset.label"
              class="preset-button"
              :class="{
                active: ratio === preset.ratio && tier === preset.tier
              }"
              type="button"
              :disabled="
                preset.tier !== '1k' &&
                preset.tier !== 'auto' &&
                !form.model.includes('codex')
              "
              :title="
                preset.tier !== '1k' && preset.tier !== 'auto'
                  ? '仅名称包含 codex 的模型可用'
                  : `${preset.width} × ${preset.height}`
              "
              @click="applySizePreset(preset)"
            >
              {{ preset.label }}
            </button>
          </div>
          <label class="form-field"
            ><span class="field-label">生成数量（每张一个任务）</span
            ><input
              v-model.number="form.n"
              class="field-input"
              type="number"
              min="1"
              max="100"
              step="1"
              required
          /></label>
          <div class="preset-list">
            <button
              v-for="count in 10"
              :key="count"
              class="preset-button"
              :class="{ active: form.n === count }"
              type="button"
              @click="form.n = count"
            >
              {{ count }} 张
            </button>
          </div>
          <details class="advanced-settings">
            <summary>输出选项</summary>
            <div class="parameter-fields">
              <label v-for="field in fields" :key="field.key" class="form-field"
                ><span class="field-label">{{ field.label }}</span
                ><select v-model="form[field.key]" class="field-input">
                  <option
                    v-for="option in field.options"
                    :key="option.value"
                    :value="option.value"
                  >
                    {{ option.label }}
                  </option>
                </select></label
              >
            </div>
          </details>
        </section>
        <p v-if="width * height > 40000000" class="field-error">
          尺寸总像素不能超过 4000 万，请调整宽度和高度。
        </p>
        <p v-if="modelsError" class="field-error">{{ modelsError }}</p>
        <p
          v-if="
            form.background === 'transparent' && form.outputFormat === 'jpeg'
          "
          class="field-error"
        >
          透明背景需要选择 PNG 或 WebP。
        </p>
        <section class="reference-section">
          <div class="reference-head">
            <span>参考图</span
            ><span class="field-key">{{ references.length }} / 10</span>
          </div>
          <div class="reference-list">
            <div
              v-for="(reference, index) in references"
              :key="reference.id"
              class="reference-item"
            >
              <el-image
                class="reference-image"
                :src="reference.url"
                :alt="reference.name"
                :preview-src-list="references.map((item) => item.url)"
                :initial-index="index"
                fit="cover"
                preview-teleported
              />
              <button
                class="remove-reference"
                type="button"
                :aria-label="`移除 ${reference.name}`"
                @click="removeReference(index)"
              >
                <X :size="12" />
              </button>
              <span class="reference-name" :title="reference.name">{{
                reference.name
              }}</span>
            </div>
            <label v-if="references.length < 10" class="upload-control">
              <Plus :size="21" /><span>添加图片</span>
              <input
                class="upload-input"
                type="file"
                accept="image/png,image/jpeg,image/webp"
                multiple
                :disabled="uploading"
                @change="addImages($event, false)"
              />
            </label>
          </div>
          <p class="reference-hint">PNG / JPEG / WebP，单张最多 20 MB。</p>
          <div class="mask-row">
            <label class="mask-upload">
              <Plus :size="13" />{{ mask ? "更换蒙版" : "添加蒙版（可选）" }}
              <input
                class="upload-input"
                type="file"
                accept="image/png"
                :disabled="uploading"
                @change="addImages($event, true)"
              />
            </label>
            <button
              v-if="mask"
              class="text-button"
              type="button"
              @click="mask = null"
            >
              移除
            </button>
          </div>
          <p v-if="mask" class="reference-hint">
            {{ mask.name }} · 透明区域为编辑范围；尺寸须与首张参考图一致。
          </p>
        </section>
        <button class="submit-button" type="submit" :disabled="!canSubmit">
          <LoaderCircle
            v-if="submitting || uploading"
            class="spinning"
            :size="16"
          /><Sparkles v-else :size="16" />
          {{
            submitting
              ? "正在提交…"
              : form.mode === "edit"
                ? "提交图片编辑任务"
                : "提交文生图任务"
          }}
        </button>
        <p class="submit-hint">
          生成使用所选账号的图片额度，任务在后台继续执行。
        </p>
        <p v-if="submitError" class="field-error" role="alert">
          {{ submitError }}
        </p>
      </form>
      <div
        ref="panelDivider"
        class="panel-divider"
        role="separator"
        tabindex="0"
        aria-label="调整创建任务与任务列表宽度"
        aria-orientation="vertical"
        aria-controls="image-create-panel image-tasks-panel"
        :aria-valuenow="Math.round(leftWidth)"
        :aria-valuemin="20"
        :aria-valuemax="80"
        title="拖动调整宽度，双击恢复 45%"
        @pointerdown="startPanelResize"
        @pointermove="movePanelResize"
        @pointerup="stopPanelResize"
        @pointercancel="stopPanelResize"
        @lostpointercapture="stopPanelResize"
        @keydown="resizePanelByKeyboard"
        @dblclick="leftWidth = 45"
      ></div>
      <section id="image-tasks-panel" class="tasks-panel">
        <header class="tasks-head">
          <div class="tasks-title">
            <span data-emphasis>当前会话</span
            ><span class="task-count">{{ total }} 轮</span>
          </div>
          <div class="tasks-controls">
            <select
              v-model="status"
              class="status-select"
              aria-label="筛选任务状态"
            >
              <option value="">全部状态</option>
              <option
                v-for="(label, key) in statusLabels"
                :key="key"
                :value="key"
              >
                {{ label }}
              </option>
            </select>
            <label class="auto-refresh"
              ><input v-model="autoRefresh" type="checkbox" />自动刷新</label
            >
            <button
              class="icon-button"
              type="button"
              aria-label="刷新账号与任务"
              title="刷新账号与任务"
              :disabled="refreshing"
              @click="refreshAll"
            >
              <RefreshCw :size="15" :class="{ spinning: refreshing }" />
            </button>
            <button
              class="action-button"
              type="button"
              @click="toggleSelection"
            >
              {{ selecting ? "取消" : "选择" }}
            </button>
          </div>
        </header>
        <div v-if="selecting" class="selection-bar">
          <label class="auto-refresh"
            ><input
              type="checkbox"
              :checked="allSelected"
              :disabled="!selectableTasks.length"
              @change="toggleAll"
            />本页全选</label
          >
          <span class="selection-count">已选 {{ selected.length }} 项</span>
          <button
            class="action-button"
            type="button"
            :disabled="!selected.length || exporting"
            @click="exportTasks(selected)"
          >
            <Download :size="14" />导出
          </button>
          <button
            class="action-button danger-button"
            type="button"
            :disabled="!selected.length || deleting"
            @click="deleteTasks"
          >
            <Trash2 :size="14" />删除
          </button>
        </div>
        <p v-if="listError" class="list-error" role="alert">{{ listError }}</p>
        <div v-if="!tasks.length" class="empty-state">
          <div class="empty-icon">
            <ImagePlus :size="30" :stroke-width="1.3" />
          </div>
          <span class="empty-title">{{
            refreshing
              ? "正在读取任务…"
              : status
                ? "暂无此状态的任务"
                : "第一张作品，从这里开始"
          }}</span>
          <span class="empty-description">{{
            status ? "切换状态查看其他任务" : "填写提示词，生成或编辑你的图片"
          }}</span>
        </div>
        <div v-else class="task-list">
          <section v-for="round in rounds" :key="round.id" class="round-card">
            <header class="round-header">
              <p v-if="!roundState[round.id]?.hidePrompt" class="round-prompt">
                {{ round.tasks[0].request.prompt }}
              </p>
              <div class="round-actions">
                <button
                  class="text-button"
                  type="button"
                  @click="reuseTask(round.tasks[0], true)"
                >
                  复用配置
                </button>
                <button
                  class="text-button"
                  type="button"
                  :disabled="submitting"
                  @click="regenerateRound(round)"
                >
                  全部重新生成
                </button>
                <button
                  class="text-button"
                  type="button"
                  @click="removeRoundPrompt(round)"
                >
                  删除提示词记录
                </button>
                <button
                  class="text-button"
                  type="button"
                  @click="removeRoundResults(round)"
                >
                  删除本轮生成结果
                </button>
              </div>
            </header>
            <article
              v-for="task in round.tasks"
              :key="task.id"
              class="task-card"
              :class="{ 'task-selected': selected.includes(task.id) }"
            >
              <header class="task-meta">
                <label v-if="selecting" class="task-check">
                  <input
                    v-model="selected"
                    type="checkbox"
                    :value="task.id"
                    :disabled="task.status === 'processing'"
                    :aria-label="`选择任务 ${formatDateTime(task.createdAt)}`"
                  />
                </label>
                <span class="task-status" :class="task.status">{{
                  statusLabels[task.status]
                }}</span>
                <span class="task-generation-mode" aria-label="调用模式">{{
                  task.request.generationMode === "web" ? "Web" : "Codex"
                }}</span>
                <span class="task-time">{{
                  formatDateTime(task.createdAt)
                }}</span>
                <span class="task-count"
                  >{{ task.imageCount }} / {{ task.request.n }} 张</span
                >
              </header>
              <button
                v-if="task.imageCount"
                class="task-preview"
                type="button"
                :disabled="detailLoading"
                aria-label="查看生成图片"
                @click="showDetail(task, 'images')"
              >
                <el-image
                  class="task-thumbnail"
                  :src="task.thumbnail"
                  fit="contain"
                  lazy
                >
                  <template #placeholder>
                    <span class="preview-placeholder"
                      ><LoaderCircle
                        class="spinning"
                        :size="28"
                      />正在加载图片…</span
                    >
                  </template>
                  <template #error>
                    <span class="preview-placeholder"
                      ><ImageOff :size="28" />预览暂不可用，点击查看原图</span
                    >
                  </template>
                </el-image>
                <span class="preview-hint">{{
                  task.imageCount > 1
                    ? `查看全部 ${task.imageCount} 张图片`
                    : "点击查看原图"
                }}</span>
              </button>
              <div v-else class="task-placeholder" role="status">
                <template v-if="['processing', 'queued'].includes(task.status)">
                  <LoaderCircle class="spinning" :size="36" />
                  <span class="placeholder-title">{{
                    task.status === "queued" ? "排队中…" : "正在生成图片…"
                  }}</span>
                  <span class="placeholder-hint">{{
                    autoRefresh
                      ? "完成后会自动显示在这里"
                      : "完成后点击刷新查看结果"
                  }}</span>
                </template>
                <template v-else>
                  <ImageOff :size="36" :stroke-width="1.3" />
                  <span class="placeholder-title"
                    >{{ statusLabels[task.status] }}，暂无生成图片</span
                  >
                  <span class="placeholder-hint">可以复用参数重新生成</span>
                </template>
              </div>
              <p
                v-if="
                  task.error?.message &&
                  !roundState[round.id]?.ignored?.includes(task.id)
                "
                class="task-error"
              >
                {{ task.error.message }}
              </p>
              <footer class="task-actions">
                <button
                  class="text-button"
                  type="button"
                  :disabled="
                    submitting || ['queued', 'processing'].includes(task.status)
                  "
                  @click="regenerateTask(task)"
                >
                  重新生成
                </button>
                <button
                  v-if="
                    task.canResume &&
                    ['failed', 'interrupted'].includes(task.status)
                  "
                  class="text-button"
                  type="button"
                  :disabled="resuming.includes(task.id)"
                  @click="resumeTask(task)"
                >
                  继续等待
                </button>
                <button
                  v-if="
                    task.error?.message &&
                    !roundState[round.id]?.ignored?.includes(task.id)
                  "
                  class="text-button"
                  type="button"
                  @click="ignoreTaskError(round, task)"
                >
                  忽略错误
                </button>
                <button
                  class="text-button"
                  type="button"
                  :disabled="detailLoading"
                  @click="showDetail(task, 'parameters')"
                >
                  查看参数
                </button>
                <button
                  class="text-button"
                  type="button"
                  @click="reuseTask(task)"
                >
                  复用参数
                </button>
                <button
                  v-if="task.imageCount"
                  class="text-button"
                  type="button"
                  :disabled="exporting"
                  @click="exportTasks([task.id])"
                >
                  导出图片
                </button>
              </footer>
            </article>
          </section>
        </div>
        <footer class="pagination">
          <label class="page-size-control">
            <span>每页</span>
            <select
              v-model.number="pageSize"
              class="page-size-select"
              aria-label="每页显示条数"
            >
              <option v-for="size in pageSizeOptions" :key="size" :value="size">
                {{ size }}
              </option>
            </select>
            <span>条</span>
          </label>
          <button
            class="action-button"
            type="button"
            :disabled="page <= 1"
            @click="page--"
          >
            上一页
          </button>
          <span
            >{{ page }} / {{ Math.max(1, Math.ceil(total / pageSize)) }}</span
          >
          <button
            class="action-button"
            type="button"
            :disabled="page * pageSize >= total"
            @click="page++"
          >
            下一页
          </button>
        </footer>
      </section>
    </div>

    <ImageDrawingDialog
      v-if="drawing"
      :source="drawing.source"
      @close="drawing = null"
      @apply="applyDrawing"
    />
    <ImagePromptLibrary
      v-if="promptLibraryOpen"
      @close="promptLibraryOpen = false"
      @select="applyLibraryPrompt"
    />
    <BaseModal
      v-if="detail"
      :class="{ 'image-detail-modal': detailView === 'images' }"
      :title="detailView === 'parameters' ? '任务参数' : '生成图片'"
      :description="`${detail.accountName} · ${formatDateTime(detail.createdAt)} · ${statusLabels[detail.status]}`"
      @close="detail = null"
    >
      <section class="detail-content">
        <p v-if="detailView === 'parameters'" class="detail-prompt">
          {{ detail.request.prompt }}
        </p>
        <p v-if="detail.error?.message" class="detail-error">
          {{ detail.error.message }}
        </p>
        <div v-if="detailView === 'images'" class="detail-images">
          <figure
            v-for="(item, index) in detail.data"
            :key="index"
            class="detail-image-card"
          >
            <el-image
              class="detail-image"
              :src="imageUrl(item)"
              :preview-src-list="detail.data.map(imageUrl)"
              :initial-index="index"
              fit="contain"
              preview-teleported
            />
            <figcaption class="detail-caption">
              <div class="detail-image-info">
                <p class="detail-info-title">图片信息</p>
                <dl class="detail-info-list">
                  <div class="detail-info-field">
                    <dt class="detail-info-label">尺寸</dt>
                    <dd class="detail-info-value">
                      {{ detail.images[index]?.size || "未知" }}
                    </dd>
                  </div>
                  <div class="detail-info-field">
                    <dt class="detail-info-label">格式</dt>
                    <dd class="detail-info-value">
                      {{ item.output_format || "未知" }}
                    </dd>
                  </div>
                  <div class="detail-info-field">
                    <dt class="detail-info-label">调用模式</dt>
                    <dd class="detail-info-value">
                      {{
                        detail.request.generationMode === "web"
                          ? "Web"
                          : "Codex"
                      }}
                    </dd>
                  </div>
                  <div class="detail-info-field">
                    <dt class="detail-info-label">模型</dt>
                    <dd class="detail-info-value">
                      {{ detail.request.model }}
                    </dd>
                  </div>
                </dl>
              </div>
              <div class="detail-image-actions">
                <button
                  class="action-button"
                  type="button"
                  @click="editResult(item)"
                >
                  <Paintbrush :size="14" />编辑
                </button>
                <button
                  class="action-button"
                  type="button"
                  @click="referenceResult(item)"
                >
                  <ImagePlus :size="14" />引用
                </button>
                <button
                  class="action-button"
                  type="button"
                  :disabled="exporting"
                  @click="downloadImage(index)"
                >
                  <Download :size="14" />{{ exporting ? "下载中…" : "下载" }}
                </button>
              </div>
            </figcaption>
          </figure>
        </div>
        <div v-if="detailView === 'parameters'" class="detail-meta">
          <span
            >调用模式：{{
              detail.request.generationMode === "web" ? "Web" : "Codex"
            }}</span
          >
          <span>模型：{{ detail.request.model }}</span
          ><span>质量：{{ detail.request.quality }}</span
          ><span>请求尺寸：{{ detail.request.size }}</span>
          <span
            >任务类型：{{
              detail.request.mode === "edit" ? "图片编辑" : "文生图"
            }}</span
          >
          <span>请求数量：{{ detail.request.n }} 张</span>
          <span>图片格式：{{ detail.request.outputFormat }}</span>
          <span>响应格式：{{ detail.request.responseFormat }}</span>
          <span>蒙版：{{ detail.hasMask ? "有" : "无" }}</span>
          <span>背景：{{ detail.request.background || "默认" }}</span
          ><span>实际图片：{{ detail.imageCount }} 张</span
          ><span>参考图：{{ detail.inputCount }} 张</span>
        </div>
        <details v-if="detailView === 'parameters'" class="technical-detail">
          <summary>请求与用量记录</summary>
          <pre class="usage-content">{{
            JSON.stringify(
              {
                taskId: detail.id,
                requestId: detail.requestId,
                endpoint: detail.endpoint,
                usage: detail.usage
              },
              null,
              2
            )
          }}</pre>
        </details>
        <p
          v-if="
            detailView === 'parameters' &&
            detail.request.responseFormat === 'url' &&
            detail.imageCount
          "
          class="detail-hint"
        >
          图片保存在本地；URL 为预览用 data URL，不是可分享的公网链接。
        </p>
      </section>
    </BaseModal>
  </section>
</template>

<script setup>
import {
  computed,
  defineAsyncComponent,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  watch
} from "vue"
import {
  Download,
  BookOpen,
  ImageOff,
  ImagePlus,
  LoaderCircle,
  Paintbrush,
  Plus,
  RefreshCw,
  Sparkles,
  Trash2,
  X
} from "lucide-vue-next"
import { ElImage, ElMessageBox } from "element-plus"
import "element-plus/es/components/image/style/css"
import "element-plus/es/components/message-box/style/css"
import BaseModal from "@/components/BaseModal.vue"
import ImageDrawingDialog from "./ImageDrawingDialog.vue"
import { systemApi, toolboxApi } from "@/api"
import { subscribe } from "@/api/request"
import { formatDateTime } from "@/utils/formatters"
import { createMessage } from "@/utils/message"

// reactive 会解包模式引用，表单回填直接同步到页面头部。
const generationMode = defineModel("generationMode", {
  type: String,
  default: "web"
})

// 词库按需加载，打开时才读取分类和分页内容。
const ImagePromptLibrary = defineAsyncComponent(
  () => import("@/features/tools/components/ImagePromptLibrary.vue")
)
const promptLibraryOpen = ref(false)
const workbenchLayout = ref(null)
const panelDivider = ref(null)
const leftWidth = ref(45)
const resizing = ref(false)
let panelDrag = null

function startPanelResize(event) {
  if (event.button !== 0) return
  event.preventDefault()
  panelDrag = {
    pointerId: event.pointerId,
    x: event.clientX,
    width: leftWidth.value
  }
  resizing.value = true
  event.currentTarget.setPointerCapture(event.pointerId)
}

function movePanelResize(event) {
  if (!panelDrag || panelDrag.pointerId !== event.pointerId) return
  // 应用整体有 transform 缩放，使用可视宽度计算比例，让分隔线跟随鼠标。
  const width =
    workbenchLayout.value.getBoundingClientRect().width -
    panelDivider.value.getBoundingClientRect().width
  if (width <= 0) return
  leftWidth.value = Math.min(
    80,
    Math.max(
      20,
      panelDrag.width + ((event.clientX - panelDrag.x) / width) * 100
    )
  )
}

function stopPanelResize(event) {
  if (!panelDrag || panelDrag.pointerId !== event.pointerId) return
  panelDrag = null
  resizing.value = false
  if (event.currentTarget.hasPointerCapture(event.pointerId))
    event.currentTarget.releasePointerCapture(event.pointerId)
}

function resizePanelByKeyboard(event) {
  const widths = {
    ArrowLeft: leftWidth.value - 2,
    ArrowRight: leftWidth.value + 2,
    Home: 20,
    End: 80,
    Enter: 45
  }
  if (!(event.key in widths)) return
  event.preventDefault()
  leftWidth.value = Math.min(80, Math.max(20, widths[event.key]))
}

function applyLibraryPrompt(template) {
  form.prompt = template.prompt
  if (typeof template.model === "string" && template.model.trim())
    form.model = template.model.trim()
  form.mode = references.value.length ? "edit" : "generate"
  submitError.value = ""
  promptLibraryOpen.value = false
  createMessage.success(
    template.mode === "edit"
      ? "已填入提示词，请确认参考图后提交"
      : "已填入提示词"
  )
}

const statusLabels = {
  queued: "排队中",
  processing: "生成中",
  completed: "已完成",
  partial: "部分完成",
  failed: "失败",
  interrupted: "已中断"
}
const fields = [
  {
    key: "outputFormat",
    label: "图片格式",
    apiName: "output_format",
    options: ["png", "jpeg", "webp"].map((value) => ({ value, label: value }))
  },
  {
    key: "background",
    label: "背景",
    apiName: "background",
    options: [
      { value: "", label: "默认（不传）" },
      { value: "auto", label: "自动" },
      { value: "opaque", label: "不透明" },
      { value: "transparent", label: "透明" }
    ]
  },
  {
    key: "responseFormat",
    label: "响应格式",
    apiName: "response_format",
    options: [
      { value: "url", label: "url（本地图片）" },
      { value: "b64_json", label: "b64_json" }
    ]
  }
]
const form = reactive({
  accountId: "",
  generationMode,
  mode: "generate",
  prompt: "",
  model: "gpt-image-2",
  size: "1024x1024",
  quality: "auto",
  n: 1,
  outputFormat: "png",
  background: "",
  responseFormat: "url"
})
// 浏览器只保存会话目录与偏好，图片、参考图、蒙版继续由 SQLite 按需读取。
function readLocal(key, fallback) {
  try {
    return JSON.parse(localStorage.getItem(key) || "null") ?? fallback
  } catch {
    return fallback
  }
}

function writeLocal(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    createMessage.error("本地存储空间不足，当前更改未持久化")
  }
}

const preferences = readLocal("image-workbench-preferences", {})
const pageSizeOptions = [10, 20, 50, 100]
const pageSize = ref(
  pageSizeOptions.includes(preferences.pageSize) ? preferences.pageSize : 50
)
for (const key of ["model", "quality", "n"]) {
  if (preferences[key] !== undefined) form[key] = preferences[key]
}
const width = ref(preferences.width || 1024)
const height = ref(preferences.height || 1024)
const ratio = ref(preferences.ratio || "1:1")
const tier = ref(preferences.tier || "1k")
const settingsOpen = ref(false)
const drawing = ref(null)
const historyOpen = ref(false)
const savedHistory = readLocal("image-workbench-conversations", {})
const conversations = ref(
  Array.isArray(savedHistory.conversations) ? savedHistory.conversations : []
)
const roundState = ref(savedHistory.rounds || {})
const activeConversationId = ref(savedHistory.active || "")
const historyIndex = ref([])
const resuming = ref([])
const currentConversation = computed(() =>
  conversations.value.find((item) => item.id === activeConversationId.value)
)
const sizePresets = [
  ["1:1", "1k", 1024, 1024],
  ["2:3", "1k", 1024, 1536],
  ["3:2", "1k", 1536, 1024],
  ["3:4", "1k", 1024, 1365],
  ["4:3", "1k", 1365, 1024],
  ["9:16", "1k", 1088, 1920],
  ["16:9", "1k", 1920, 1088],
  ["1:1", "2k", 2048, 2048],
  ["16:9", "2k", 2560, 1440],
  ["9:16", "2k", 1440, 2560],
  ["16:9", "4k", 3840, 2160],
  ["9:16", "4k", 2160, 3840],
  ["auto", "auto", 1024, 1024]
].map(([ratio, tier, width, height]) => ({
  ratio,
  tier,
  width,
  height,
  label: `${ratio}${["2k", "4k"].includes(tier) ? `(${tier})` : ""}`
}))
const qualityOptions = computed(() => [
  { value: "auto", label: "自动" },
  { value: "low", label: "低" },
  { value: "medium", label: "中" },
  { value: "high", label: "高" },
  ...(form.model.includes("image-2.5")
    ? [
        { value: "xhigh", label: "超高" },
        { value: "max", label: "最高" }
      ]
    : [])
])

function applySizePreset(preset) {
  width.value = preset.width
  height.value = preset.height
  ratio.value = preset.ratio
  tier.value = preset.tier
}

watch(
  () => form.model,
  () => {
    if (
      !form.model.includes("image-2.5") &&
      ["xhigh", "max"].includes(form.quality)
    )
      form.quality = "auto"
  },
  { immediate: true }
)
watch(
  [
    () => form.model,
    () => form.quality,
    () => form.n,
    width,
    height,
    ratio,
    tier,
    pageSize
  ],
  () => {
    form.size = `${width.value}x${height.value}`
    writeLocal("image-workbench-preferences", {
      model: form.model,
      quality: form.quality,
      n: form.n,
      width: width.value,
      height: height.value,
      ratio: ratio.value,
      tier: tier.value,
      pageSize: pageSize.value
    })
  }
)

function saveHistory() {
  writeLocal("image-workbench-conversations", {
    conversations: conversations.value,
    active: activeConversationId.value,
    rounds: roundState.value
  })
}

function patchRound(id, changes) {
  roundState.value[id] = { ...roundState.value[id], ...changes }
  saveHistory()
}

function ensureConversation() {
  if (!currentConversation.value) {
    const item = {
      id: crypto.randomUUID(),
      title: "新对话",
      updatedAt: Date.now()
    }
    conversations.value.unshift(item)
    activeConversationId.value = item.id
    saveHistory()
  }
  return currentConversation.value
}
ensureConversation()

function conversationStats(id) {
  const items = historyIndex.value.filter(
    (item) =>
      (item.conversationId || "legacy") === id &&
      !roundState.value[item.roundId || item.id]?.replaced?.includes(item.id)
  )
  const queued = items.filter((item) => item.status === "queued").length
  const processing = items.filter((item) => item.status === "processing").length
  return {
    rounds: new Set(items.map((item) => item.roundId || item.id)).size,
    queued,
    processing,
    active: queued + processing
  }
}

let historyVersion = 0
async function loadHistory() {
  const version = ++historyVersion
  const items = await toolboxApi.imageHistory()
  if (disposed || version !== historyVersion) return
  historyIndex.value = items
  for (const item of items) {
    const id = item.conversationId || "legacy"
    let conversation = conversations.value.find((item) => item.id === id)
    if (!conversation) {
      conversation = {
        id,
        title:
          id === "legacy"
            ? "早期任务"
            : (item.prompt || "生图对话").slice(0, 28),
        updatedAt: item.createdAt
      }
      conversations.value.push(conversation)
    }
    conversation.updatedAt = Math.max(conversation.updatedAt, item.createdAt)
  }
  conversations.value.sort((a, b) => b.updatedAt - a.updatedAt)
  saveHistory()
}

function selectConversation(id) {
  activeConversationId.value = id
  historyOpen.value = false
  selected.value = []
  status.value = ""
  page.value = 1
  saveHistory()
  loadTasks()
}

function newConversation() {
  activeConversationId.value = ""
  ensureConversation()
  form.prompt = ""
  references.value = []
  mask.value = null
  form.mode = "generate"
  selectConversation(activeConversationId.value)
}

async function renameConversation(conversation) {
  try {
    const result = await ElMessageBox.prompt("输入会话名称", "重命名会话", {
      inputValue: conversation.title,
      inputPattern: /\S/,
      inputErrorMessage: "名称不能为空",
      confirmButtonText: "保存",
      cancelButtonText: "取消"
    })
    conversation.title = result.value.trim().slice(0, 100)
    // 用户主动命名后，即便名称是“新对话”，提交任务也不再自动覆盖。
    conversation.renamed = true
    saveHistory()
  } catch {
    /* 取消时保留原名。 */
  }
}

async function deleteConversation(id) {
  try {
    await loadHistory()
  } catch (error) {
    createMessage.error(String(error))
    return
  }
  const items = historyIndex.value.filter(
    (item) => !id || (item.conversationId || "legacy") === id
  )
  if (items.some((item) => item.status === "processing"))
    return createMessage.warning("会话中仍有任务生成中，请完成后再删除")
  try {
    await ElMessageBox.confirm(
      id
        ? "删除此会话及其本地图片，并取消排队任务？"
        : "清空全部历史及本地图片，并取消排队任务？",
      "删除历史",
      { confirmButtonText: "删除", cancelButtonText: "取消", type: "warning" }
    )
  } catch {
    return
  }
  try {
    if (items.length)
      await toolboxApi.deleteImageTasks({ ids: items.map((item) => item.id) })
    historyVersion++
    for (const item of items) delete roundState.value[item.roundId || item.id]
    conversations.value = id
      ? conversations.value.filter((item) => item.id !== id)
      : []
    historyIndex.value = historyIndex.value.filter(
      (item) => id && (item.conversationId || "legacy") !== id
    )
    if (!currentConversation.value) {
      activeConversationId.value = ""
      ensureConversation()
    }
    saveHistory()
    await loadTasks()
  } catch (error) {
    createMessage.error(String(error))
  }
}

function promptKeydown(event) {
  if (
    event.key === "Enter" &&
    !event.shiftKey &&
    !event.isComposing &&
    event.keyCode !== 229
  ) {
    event.preventDefault()
    submitTask()
  }
}

function pasteImages(event) {
  const files = Array.from(event.clipboardData?.files || []).filter((file) =>
    file.type.startsWith("image/")
  )
  if (files.length) {
    event.preventDefault()
    addImages({ target: { files } }, false)
  }
}

function dropImages(event) {
  const files = Array.from(event.dataTransfer?.files || [])
  if (files.length) addImages({ target: { files } }, false)
}

function removeReference(index) {
  references.value.splice(index, 1)
  if (index === 0) mask.value = null
  form.mode = references.value.length ? "edit" : "generate"
}

function appendReference(item) {
  if (references.value.length >= 10) {
    createMessage.error("最多添加 10 张参考图")
    return false
  }
  if (
    references.value.reduce((size, item) => size + item.url.length, 0) +
      item.url.length +
      (mask.value?.url.length || 0) >
    32 * 1024 * 1024
  ) {
    createMessage.error("图片编码后总大小不能超过 32 MB")
    return false
  }
  references.value.push(item)
  form.mode = "edit"
  return true
}

function applyDrawing(result) {
  if (result.source) {
    if (result.source.length + result.url.length > 32 * 1024 * 1024)
      return createMessage.error("图片与蒙版总大小超过 32 MB")
    references.value = [
      { id: crypto.randomUUID(), name: "重绘原图.png", url: result.source }
    ]
    mask.value = { name: result.name, url: result.url }
    form.mode = "edit"
  } else if (!appendReference({ ...result, id: crypto.randomUUID() })) return
  drawing.value = null
}

async function regenerateTask(task, count = 1, wholeRound = false) {
  if (submitting.value) return
  submitting.value = true
  try {
    const inputs = await toolboxApi.imageTaskInputs({ id: task.id })
    // 重试使用原账号、通道和输入，不受当前草稿的设置影响。
    const roundId = wholeRound
      ? crypto.randomUUID()
      : task.request.roundId || task.id
    await toolboxApi.submitImageTask({
      ...task.request,
      generationMode: task.request.generationMode || "codex",
      conversationId: task.request.conversationId || "legacy",
      roundId,
      n: count,
      images: inputs.images,
      mask: inputs.mask
    })
    status.value = ""
    if (wholeRound) {
      patchRound(roundId, { count })
      page.value = 1
    } else {
      patchRound(roundId, {
        replaced: [
          ...new Set([...(roundState.value[roundId]?.replaced || []), task.id])
        ]
      })
    }
    await loadTasks()
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    submitting.value = false
  }
}

async function regenerateRound(round) {
  const task = round.tasks[0]
  const count =
    roundState.value[round.id]?.count || task.batchCount || task.request.n
  await regenerateTask(task, count, true)
}

async function removeRoundPrompt(round) {
  try {
    await ElMessageBox.confirm(
      "删除这条提示词记录？对应生成结果会保留。",
      "删除提示词记录",
      { confirmButtonText: "删除", cancelButtonText: "取消" }
    )
  } catch {
    return
  }
  patchRound(round.id, { hidePrompt: true })
}

async function removeRoundResults(round) {
  const ids = historyIndex.value
    .filter((item) => (item.roundId || item.id) === round.id)
    .map((item) => item.id)
  try {
    await ElMessageBox.confirm(
      "删除本轮已经生成的图片？提示词、配置和输入仍可复用。",
      "删除生成结果",
      { confirmButtonText: "删除", cancelButtonText: "取消" }
    )
  } catch {
    return
  }
  try {
    await toolboxApi.clearImageResults({ ids })
    await loadTasks()
  } catch (error) {
    createMessage.error(String(error))
  }
}

function ignoreTaskError(round, task) {
  patchRound(round.id, {
    ignored: [
      ...new Set([...(roundState.value[round.id]?.ignored || []), task.id])
    ]
  })
}

async function resumeTask(task) {
  if (resuming.value.includes(task.id)) return
  resuming.value.push(task.id)
  try {
    await toolboxApi.resumeImageTask({ id: task.id })
    await loadTasks()
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    resuming.value = resuming.value.filter((id) => id !== task.id)
  }
}

const accounts = ref([])
const fallbackModels = [
  "gpt-image-2.5-sunburst",
  "gpt-image-2.5-flare",
  "gpt-image-2",
  "gpt-5-5-thinking",
  "gpt-5-5",
  "gpt-5-3"
].map((id) => ({ id }))
const imageModels = ref(readLocal("image-workbench-models", fallbackModels))
const loadingModels = ref(false)
const modelsError = ref("")
let modelsRequestVersion = 0
const webQuota = ref(null)
const loadingQuota = ref(false)
const quotaError = ref("")
let quotaRequestVersion = 0
// 上游恢复信息可能是日期或说明文字，只有有效日期才进行格式化。
const quotaResetLabel = computed(() => {
  const value = webQuota.value?.resetAfter
  if (!value) return ""
  return Number.isNaN(new Date(value).getTime()) ? value : formatDateTime(value)
})
const references = ref([])
const mask = ref(null)
const tasks = ref([])
const rounds = computed(() => {
  const groups = new Map()
  for (const task of tasks.value) {
    const id = task.request.roundId || task.id
    if (roundState.value[id]?.replaced?.includes(task.id)) continue
    if (!groups.has(id)) groups.set(id, { id, tasks: [] })
    groups.get(id).tasks.push(task)
  }
  return [...groups.values()]
})
const total = ref(0)
const page = ref(1)
const status = ref("")
const autoRefresh = ref(true)
const selecting = ref(false)
const selected = ref([])
const detail = ref(null)
const detailView = ref("images")
const detailLoading = ref(false)
const loadingAccounts = ref(true)
const refreshing = ref(false)
const submitting = ref(false)
const uploading = ref(false)
const exporting = ref(false)
const deleting = ref(false)
const submitError = ref("")
const listError = ref("")
let timer
let requestVersion = 0
let disposed = false
const availableAccounts = computed(() =>
  accounts.value.filter(
    (account) => !account.disabled && !account.requiresReauth
  )
)
const canSubmit = computed(
  () =>
    !submitting.value &&
    !uploading.value &&
    availableAccounts.value.some((account) => account.id === form.accountId) &&
    form.prompt.trim() &&
    form.model.trim() &&
    Number.isInteger(form.n) &&
    form.n >= 1 &&
    form.n <= 100 &&
    Number.isInteger(width.value) &&
    width.value >= 1 &&
    Number.isInteger(height.value) &&
    height.value >= 1 &&
    width.value * height.value <= 40_000_000 &&
    (form.mode !== "edit" || references.value.length) &&
    !(form.background === "transparent" && form.outputFormat === "jpeg")
)
const selectableTasks = computed(() =>
  rounds.value
    .flatMap((round) => round.tasks)
    .filter((task) => task.status !== "processing")
)
const allSelected = computed(
  () =>
    selectableTasks.value.length > 0 &&
    selectableTasks.value.every((task) => selected.value.includes(task.id))
)

async function loadAccounts() {
  loadingAccounts.value = true
  try {
    accounts.value = await toolboxApi.imageAccounts()
    if (
      !availableAccounts.value.some((account) => account.id === form.accountId)
    ) {
      form.accountId =
        (
          availableAccounts.value.find((account) => account.active) ||
          availableAccounts.value[0]
        )?.id || ""
    }
  } finally {
    loadingAccounts.value = false
  }
}

async function loadModels() {
  // 切换账号时丢弃旧请求，避免不同代理的目录结果互相覆盖。
  const version = ++modelsRequestVersion
  const accountId = form.accountId
  modelsError.value = ""
  loadingModels.value = Boolean(accountId)
  if (!accountId) return

  try {
    const result = await toolboxApi.imageModels({ accountId })
    if (version !== modelsRequestVersion || disposed) return
    imageModels.value = result.data
    writeLocal("image-workbench-models", result.data)
    // 初次使用默认选接口的第一项，刷新不覆盖手动输入或历史任务模型。
    if (!form.model.trim())
      form.model = result.default_image_model || result.data[0]?.id || ""
  } catch (error) {
    if (version === modelsRequestVersion && !disposed)
      modelsError.value = String(error)
  } finally {
    if (version === modelsRequestVersion && !disposed)
      loadingModels.value = false
  }
}

async function loadQuota() {
  const version = ++quotaRequestVersion
  const accountId = form.accountId
  const enabled = form.generationMode === "web" && Boolean(accountId)
  if (!enabled || webQuota.value?.accountId !== accountId) webQuota.value = null
  quotaError.value = ""
  loadingQuota.value = enabled
  if (!enabled) return

  try {
    const result = await toolboxApi.imageQuota({ accountId })
    // 账号切换或退出 Web 后，旧请求不能覆盖当前额度。
    if (version === quotaRequestVersion && !disposed) webQuota.value = result
  } catch (error) {
    if (version === quotaRequestVersion && !disposed)
      quotaError.value = String(error)
  } finally {
    if (version === quotaRequestVersion && !disposed) loadingQuota.value = false
  }
}

async function loadTasks() {
  // 状态筛选和轮询可能重叠，只应用最后一次请求，避免旧列表覆盖新筛选。
  const version = ++requestVersion
  refreshing.value = true
  try {
    const result = await toolboxApi.listImageTasks({
      page: page.value,
      status: status.value,
      conversationId: activeConversationId.value,
      pageSize: pageSize.value,
      groupByRound: true
    })
    if (version !== requestVersion || disposed) return
    tasks.value = result.items
    await loadHistory()
    if (version !== requestVersion || disposed) return
    total.value = result.total
    selected.value = selected.value.filter((id) =>
      selectableTasks.value.some((task) => task.id === id)
    )
    listError.value = ""
    const lastPage = Math.max(1, Math.ceil(total.value / pageSize.value))
    if (page.value > lastPage) page.value = lastPage
  } catch (error) {
    if (version === requestVersion) listError.value = String(error)
  } finally {
    if (version === requestVersion) refreshing.value = false
  }
}

async function refreshAll() {
  const results = await Promise.allSettled([loadAccounts(), loadTasks()])
  if (results[0].status === "rejected")
    listError.value = `账号读取失败：${results[0].reason}`
}

async function addImages(event, isMask) {
  const files = Array.from(event.target.files || [])
  if (!files.length || uploading.value) return
  event.target.value = ""
  uploading.value = true
  submitError.value = ""
  try {
    if (isMask && !references.value.length)
      throw new Error("请先添加需要编辑的参考图")
    if (!isMask && references.value.length + files.length > 10)
      throw new Error("最多添加 10 张参考图")
    const added = []
    for (const file of files) {
      if (
        !(
          isMask ? ["image/png"] : ["image/png", "image/jpeg", "image/webp"]
        ).includes(file.type)
      )
        throw new Error("请选择支持的图片格式")
      if (file.size > 20 * 1024 * 1024)
        throw new Error("单张图片不能超过 20 MB")
      const url = await new Promise((resolve, reject) => {
        const reader = new FileReader()
        reader.onload = () => resolve(reader.result)
        reader.onerror = () => reject(new Error("读取图片失败"))
        reader.readAsDataURL(file)
      })
      added.push({ id: crypto.randomUUID(), name: file.name, url })
    }
    const nextReferences = isMask
      ? references.value
      : [...references.value, ...added]
    const nextMask = isMask ? added[0] || mask.value : mask.value
    if (
      nextReferences.reduce((size, item) => size + item.url.length, 0) +
        (nextMask?.url.length || 0) >
      32 * 1024 * 1024
    )
      throw new Error("图片编码后总大小超过 32 MB，请减少图片或压缩后上传")
    references.value = nextReferences
    mask.value = nextMask
    form.mode = references.value.length ? "edit" : "generate"
  } catch (error) {
    submitError.value = error.message || String(error)
  } finally {
    uploading.value = false
  }
}

async function submitTask() {
  if (!canSubmit.value) return
  submitting.value = true
  submitError.value = ""
  try {
    // 草稿在提交后仍然保留；上游失败不会清空用户的提示词和参考图。
    const conversation = ensureConversation()
    const result = await toolboxApi.submitImageTask({
      ...form,
      mode: references.value.length ? "edit" : "generate",
      size: `${width.value}x${height.value}`,
      ratio: ratio.value,
      tier: tier.value,
      conversationId: conversation.id,
      roundId: crypto.randomUUID(),
      images: references.value.map((item) => item.url),
      mask: mask.value?.url || ""
    })
    conversation.updatedAt = Date.now()
    if (!conversation.renamed && conversation.title === "新对话")
      conversation.title = form.prompt.trim().slice(0, 28)
    patchRound(result.roundId, { count: form.n })
    saveHistory()
    status.value = ""
    page.value = 1
    await loadTasks()
    createMessage.success("任务已提交，正在后台生成")
  } catch (error) {
    submitError.value = String(error)
  } finally {
    submitting.value = false
  }
}

function toggleSelection() {
  selecting.value = !selecting.value
  selected.value = []
}

function toggleAll() {
  selected.value = allSelected.value
    ? []
    : selectableTasks.value.map((task) => task.id)
}

async function reuseTask(task, wholeRound = false) {
  if (uploading.value) return
  uploading.value = true
  try {
    // 编辑历史同时恢复参考图和蒙版，用户确认后再手动提交。
    // 早期任务没有记录调用模式，当时仅支持 Codex。
    const inputs = await toolboxApi.imageTaskInputs({ id: task.id })
    for (const key of Object.keys(form)) {
      if (key in task.request) form[key] = task.request[key]
    }
    form.generationMode = task.request.generationMode || "codex"
    form.n = wholeRound
      ? roundState.value[task.request.roundId || task.id]?.count ||
        task.batchCount ||
        task.request.n
      : task.request.n
    const dimensions = task.request.size.split("x").map(Number)
    width.value = dimensions[0] || 1024
    height.value = dimensions[1] || 1024
    ratio.value = task.request.ratio || "1:1"
    tier.value = task.request.tier || "1k"
    references.value = inputs.images.map((url, index) => ({
      id: crypto.randomUUID(),
      name: `参考图 ${index + 1}`,
      url
    }))
    mask.value = inputs.mask ? { name: "历史蒙版.png", url: inputs.mask } : null
    submitError.value = ""
    createMessage.success("已恢复历史任务参数")
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    uploading.value = false
  }
}

// 图片预览和参数分开呈现，点击图片时只展示生成结果。
async function showDetail(task, view = "images") {
  if (detailLoading.value) return
  detailView.value = view
  detailLoading.value = true
  try {
    const result = await toolboxApi.imageTaskDetail({ id: task.id })
    if (!disposed) detail.value = result
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    detailLoading.value = false
  }
}

function imageUrl(item) {
  return item.url || `data:image/${item.output_format};base64,${item.b64_json}`
}

function editResult(item) {
  drawing.value = { source: imageUrl(item) }
  detail.value = null
}

function referenceResult(item) {
  appendReference({
    id: crypto.randomUUID(),
    name: "生成结果.png",
    url: imageUrl(item)
  })
  detail.value = null
}

async function downloadImage(index) {
  const taskId = detail.value.id
  exporting.value = true
  try {
    const targetPath = await systemApi.saveFile({
      title: "下载图片",
      defaultPath: `image-${index + 1}.png`,
      filters: [{ name: "PNG 图片", extensions: ["png"] }]
    })
    if (targetPath)
      await toolboxApi.exportImageTasks({ ids: [taskId], index, targetPath })
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    exporting.value = false
  }
}

async function exportTasks(ids) {
  exporting.value = true
  try {
    const targetPath = await systemApi.saveFile({
      title: "导出图片",
      defaultPath: "image-workbench.zip",
      filters: [{ name: "ZIP 压缩包", extensions: ["zip"] }]
    })
    if (!targetPath) return
    const result = await toolboxApi.exportImageTasks({
      ids: [...ids],
      targetPath
    })
    createMessage.success(`已导出 ${result.imageCount} 张图片`)
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    exporting.value = false
  }
}

async function deleteTasks() {
  const ids = [...selected.value]
  try {
    await ElMessageBox.confirm(
      `删除所选 ${ids.length} 个任务及其本地图片？此操作无法撤销。`,
      "删除图片任务",
      { confirmButtonText: "删除", cancelButtonText: "取消", type: "warning" }
    )
  } catch {
    return
  }
  deleting.value = true
  try {
    await toolboxApi.deleteImageTasks({ ids })
    selected.value = []
    await loadTasks()
    createMessage.success("任务已删除")
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    deleting.value = false
  }
}

watch([() => form.accountId, () => form.generationMode], loadModels)
watch([() => form.accountId, () => form.generationMode], loadQuota)
// 切换条数与筛选都返回第一页，沿用页码监听完成实际查询。
watch([status, pageSize], () => {
  selected.value = []
  if (page.value !== 1) page.value = 1
  else loadTasks()
})
watch(page, () => {
  selected.value = []
  loadTasks()
})
const unsubscribe = subscribe("images:changed", (event) => {
  if (autoRefresh.value) loadTasks()
  // 任务结束后向上游重查额度，不根据生成数量在本地扣减。
  if (event.accountId === form.accountId && event.generationMode === "web")
    loadQuota()
})
const unsubscribeError = subscribe("images:storage-error", (event) => {
  listError.value = `任务结果保存失败：${event.message}`
})
onMounted(() => {
  refreshAll()
  timer = window.setInterval(() => {
    if (autoRefresh.value && !refreshing.value && !document.hidden) loadTasks()
  }, 3000)
})
onBeforeUnmount(() => {
  disposed = true
  requestVersion++
  window.clearInterval(timer)
  unsubscribe()
  unsubscribeError()
})
</script>

<style scoped lang="less">
.image-workbench {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  padding: 2px 2px 16px;
  color: var(--color-text);
  font-size: var(--font-size-base);

  position: relative;
  .conversation-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 0 12px;
    .conversation-button {
      padding: 6px 9px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel);
      color: var(--color-text);
      cursor: pointer;
    }
    .conversation-title {
      flex: 1;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      color: var(--color-text-muted);
    }
  }
  .conversation-sidebar {
    position: absolute;
    inset: 42px auto 16px 0;
    z-index: 5;
    width: 250px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    box-shadow: var(--shadow-panel);
    .history-head {
      display: flex;
      justify-content: space-between;
      .history-action {
        border: 0;
        color: var(--color-primary);
        background: transparent;
        cursor: pointer;
      }
    }
    .history-list {
      flex: 1;
      min-height: 0;
      overflow: auto;
      .history-item {
        margin-bottom: 8px;
        padding: 8px;
        border: 1px solid var(--color-line);
        border-radius: 6px;
        &.active {
          border-color: var(--color-primary);
          background: var(--color-panel-soft);
        }
        .history-select {
          display: flex;
          flex-direction: column;
          width: 100%;
          gap: 7px;
          padding: 0;
          text-align: left;
          border: 0;
          background: transparent;
          color: var(--color-text);
          cursor: pointer;
          .history-title {
            max-width: 100%;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }
          .history-meta {
            color: var(--color-text-muted);
            font-size: var(--font-size-sm);
          }
        }
        .history-actions {
          display: flex;
          gap: 10px;
          margin-top: 10px;
          .history-action {
            border: 0;
            color: var(--color-primary);
            background: transparent;
            padding: 0;
            cursor: pointer;
          }
        }
      }
    }
    .history-clear {
      border: 1px solid var(--color-line);
      background: var(--color-panel-soft);
      color: var(--color-danger);
      padding: 8px;
      border-radius: 6px;
      cursor: pointer;
    }
  }
  .workbench-layout {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
    align-items: stretch;

    &.is-resizing {
      cursor: col-resize;
      user-select: none;
    }

    .panel-divider {
      position: relative;
      flex: 0 0 18px;
      align-self: stretch;
      outline: none;
      cursor: col-resize;
      touch-action: none;
      &::before {
        content: "";
        position: absolute;
        top: 0;
        bottom: 0;
        left: 8px;
        width: 2px;
        border-radius: 2px;
        background: var(--color-line);
      }
      &:hover::before,
      &:focus-visible::before {
        background: var(--color-primary);
      }
    }

    .create-panel {
      flex: 0 0 auto;
      display: flex;
      flex-direction: column;
      min-width: 0;
      min-height: 0;
      overflow: auto;
      gap: 16px;
      padding: 20px;
      border: 1px solid var(--color-line);
      border-radius: 9px;
      background: var(--color-panel);

      > * {
        flex-shrink: 0;
      }

      .panel-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        font-size: var(--font-size-lg);
        .prompt-library-button {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          padding: 4px 7px;
          border: 1px solid var(--color-line);
          border-radius: 5px;
          background: var(--color-panel-soft);
          color: var(--color-primary);
          cursor: pointer;
          font-size: var(--font-size-sm);
        }
      }
      .composer-tools {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        .composer-button {
          display: flex;
          align-items: center;
          gap: 8px;
          padding: 7px 9px;
          background: var(--color-panel-soft);
          color: var(--color-text);
          border: 1px solid var(--color-line);
          border-radius: 6px;
          cursor: pointer;
        }
      }
      .image-settings {
        display: flex;
        flex-direction: column;
        gap: 12px;
        padding: 12px;
        background: var(--color-panel-soft);
        border: 1px solid var(--color-line);
        border-radius: 7px;
        .parameter-fields {
          display: flex;
          flex-wrap: wrap;
          gap: 10px;
          .form-field {
            min-width: 0;
            flex: 1 1 40%;
            &.model-field {
              flex-basis: 100%;
            }
          }
        }
        .preset-list {
          display: flex;
          flex-wrap: wrap;
          gap: 6px;
          .preset-button {
            padding: 5px 8px;
            border: 1px solid var(--color-line);
            border-radius: 5px;
            background: var(--color-panel);
            color: var(--color-text);
            cursor: pointer;
            &.active {
              border-color: var(--color-primary);
              color: var(--color-primary);
            }
            &:disabled {
              opacity: 0.35;
              cursor: not-allowed;
            }
          }
        }
        .advanced-settings {
          color: var(--color-text-muted);
          .parameter-fields {
            margin-top: 10px;
          }
        }
      }
      .web-quota {
        display: flex;
        flex-direction: column;
        gap: 6px;
        padding: 10px;
        border: 1px solid var(--color-line);
        border-radius: 7px;
        background: var(--color-panel-soft);
        .web-quota-head {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 8px;
          .quota-refresh {
            padding: 0;
            border: 0;
            color: var(--color-primary);
            background: transparent;
            font-size: var(--font-size-sm);
            cursor: pointer;
            &:disabled {
              opacity: 0.5;
              cursor: default;
            }
          }
        }
        .web-quota-info {
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
          overflow-wrap: anywhere;
        }
        .quota-empty {
          color: var(--color-warning);
        }
      }
      .parameter-fields {
        display: flex;
        flex-wrap: wrap;
        gap: 14px 12px;
      }
      .form-field {
        display: flex;
        flex-direction: column;
        gap: 7px;
        min-width: 0;
        width: 100%;
        .field-label {
          display: flex;
          align-items: baseline;
          gap: 5px;
          color: var(--color-text-muted);
          .model-refresh {
            margin-left: auto;
            padding: 0;
            border: 0;
            background: transparent;
            color: var(--color-primary);
            font-size: var(--font-size-sm);
            cursor: pointer;
            &:disabled {
              opacity: 0.5;
              cursor: default;
            }
          }
        }
        .field-input {
          width: 100%;
          min-width: 0;
          height: 39px;
          padding: 8px 9px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          outline: none;
          color: var(--color-text);
          background: var(--color-panel-soft);
          font-size: var(--font-size-base);
          &:focus {
            border-color: var(--color-primary);
          }
        }
        .prompt-input {
          height: 112px;
          min-height: 85px;
          resize: vertical;
          line-height: 1.7;
        }
      }
      .parameter-fields {
        .form-field {
          width: calc(50% - 6px);
        }
      }
      .prompt-field {
        flex: 1 0 180px;
        min-height: 180px;
        .prompt-input {
          flex: 1;
          min-height: 140px;
          height: auto;
        }
      }
      .field-key {
        color: var(--color-text-soft);
        font-size: var(--font-size-sm);
      }
      .account-hint,
      .field-error {
        margin: 0;
        color: var(--color-warning);
        line-height: 1.7;
      }
      .field-error {
        color: var(--color-danger);
        overflow-wrap: anywhere;
      }
      .reference-section {
        display: flex;
        flex-direction: column;
        gap: 9px;
        .reference-head {
          display: flex;
          justify-content: space-between;
          color: var(--color-text-muted);
        }
        .reference-list {
          display: flex;
          flex-wrap: wrap;
          gap: 8px;
          .reference-item {
            position: relative;
            width: 84px;
            .reference-image {
              width: 84px;
              height: 84px;
              object-fit: cover;
              border: 1px solid var(--color-line);
              border-radius: 6px;
            }
            .remove-reference {
              position: absolute;
              top: 4px;
              right: 4px;
              display: flex;
              padding: 3px;
              border: 0;
              border-radius: 50%;
              color: #fff;
              background: #0009;
              cursor: pointer;
            }
            .reference-name {
              display: block;
              overflow: hidden;
              white-space: nowrap;
              text-overflow: ellipsis;
              color: var(--color-text-muted);
              font-size: var(--font-size-sm);
            }
          }
          .upload-control {
            position: relative;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            gap: 7px;
            width: 84px;
            height: 84px;
            border: 1px dashed var(--color-line-strong);
            border-radius: 6px;
            color: var(--color-primary);
            cursor: pointer;
          }
        }
        .upload-input {
          position: absolute;
          inset: 0;
          width: 100%;
          height: 100%;
          opacity: 0;
          cursor: pointer;
        }
        .reference-hint {
          margin: 0;
          color: var(--color-text-soft);
          font-size: var(--font-size-sm);
          overflow-wrap: anywhere;
        }
        .mask-row {
          display: flex;
          justify-content: space-between;
          .mask-upload {
            position: relative;
            display: inline-flex;
            align-items: center;
            gap: 5px;
            color: var(--color-text-muted);
            cursor: pointer;
          }
        }
      }
      .submit-button {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 8px;
        height: 40px;
        border: 1px solid var(--color-primary-solid);
        border-radius: 6px;
        color: #fff;
        background: var(--color-primary-solid);
        cursor: pointer;
        &:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
      }
      .submit-hint {
        margin: -6px 0 0;
        color: var(--color-text-soft);
        font-size: var(--font-size-sm);
        line-height: 1.6;
      }
    }

    .tasks-panel {
      flex: 1;
      display: flex;
      flex-direction: column;
      min-width: 0;
      min-height: 0;
      overflow: auto;
      border: 1px solid var(--color-line);
      border-radius: 9px;
      background: var(--color-panel);
      .tasks-head {
        display: flex;
        flex-shrink: 0;
        flex-wrap: wrap;
        align-items: center;
        justify-content: space-between;
        gap: 14px;
        padding: 18px;
        .tasks-title {
          display: flex;
          align-items: center;
          gap: 8px;
          font-size: var(--font-size-lg);
          .task-count {
            color: var(--color-text-soft);
            font-size: var(--font-size-base);
          }
        }
        .tasks-controls {
          display: flex;
          flex-wrap: wrap;
          align-items: center;
          gap: 8px;
          .status-select {
            width: 96px;
            height: 36px;
            padding: 4px 6px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            color: var(--color-text);
            background: var(--color-panel-soft);
            font-size: var(--font-size-sm);
          }
          .icon-button {
            display: flex;
            align-items: center;
            justify-content: center;
            width: 31px;
            height: 31px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
            cursor: pointer;
          }
        }
      }
      .auto-refresh {
        display: flex;
        align-items: center;
        gap: 3px;
        white-space: nowrap;
        font-size: var(--font-size-sm);
        color: var(--color-text-muted);
      }
      .selection-bar {
        display: flex;
        flex-shrink: 0;
        align-items: center;
        gap: 10px;
        padding: 10px 18px;
        border-top: 1px solid var(--color-line);
        background: var(--color-panel-soft);
        .selection-count {
          flex: 1;
          color: var(--color-text-muted);
          font-size: var(--font-size-sm);
        }
      }
      .list-error {
        margin: 0;
        padding: 0 18px 12px;
        color: var(--color-danger);
        overflow-wrap: anywhere;
      }
      .empty-state {
        display: flex;
        flex: 1;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 9px;
        min-height: 270px;
        padding: 25px;
        .empty-icon {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 62px;
          height: 62px;
          margin-bottom: 9px;
          border: 1px dashed var(--color-line-strong);
          border-radius: 16px;
          color: var(--color-text-soft);
          background: var(--color-panel-soft);
        }
        .empty-title {
          color: var(--color-text-muted);
          font-size: var(--font-size-base);
        }
        .empty-description {
          color: var(--color-text-soft);
          font-size: var(--font-size-sm);
        }
      }
      .task-list {
        display: flex;
        flex: 1;
        min-height: 0;
        overflow: auto;
        flex-direction: column;
        padding: 0 18px 18px;
        gap: 12px;
        .round-card {
          display: flex;
          flex-direction: column;
          gap: 12px;
          .round-header {
            padding: 10px 0;
            border-bottom: 1px solid var(--color-line);
            .round-prompt {
              margin: 0 0 8px;
              white-space: pre-wrap;
              overflow-wrap: anywhere;
            }
            .round-actions {
              display: flex;
              flex-wrap: wrap;
              gap: 8px;
            }
          }
          .task-card {
            display: flex;
            flex-shrink: 0;
            flex-direction: column;
            min-width: 0;
            overflow: hidden;
            border: 1px solid var(--color-line);
            border-radius: 7px;
            &.task-selected {
              border-color: var(--color-primary);
              background: var(--color-primary-soft);
            }
            .task-meta {
              display: flex;
              align-items: center;
              flex-wrap: wrap;
              gap: 8px 12px;
              padding: 12px 14px;
              font-size: var(--font-size-sm);
              .task-check {
                display: flex;
                align-items: center;
              }
              .task-status {
                color: var(--color-text-muted);
                &.completed {
                  color: var(--color-success);
                }
                &.failed {
                  color: var(--color-danger);
                }
                &.processing {
                  color: var(--color-primary);
                }
                &.partial,
                &.interrupted {
                  color: var(--color-warning);
                }
              }
              .task-time {
                color: var(--color-text-soft);
              }
              .task-generation-mode {
                padding: 2px 6px;
                border: 1px solid var(--color-line);
                border-radius: 4px;
                color: var(--color-primary);
                background: var(--color-panel-soft);
              }
              .task-count {
                margin-left: auto;
                color: var(--color-text-muted);
              }
            }
            .task-preview {
              position: relative;
              display: flex;
              width: 100%;
              aspect-ratio: 4 / 3;
              max-height: 480px;
              padding: 0;
              border: 0;
              background: var(--color-panel-soft);
              cursor: zoom-in;
              .task-thumbnail {
                width: 100%;
                height: 100%;
                .preview-placeholder {
                  display: flex;
                  height: 100%;
                  flex-direction: column;
                  align-items: center;
                  justify-content: center;
                  gap: 12px;
                  color: var(--color-text-muted);
                  font-size: var(--font-size-sm);
                }
              }
              .preview-hint {
                position: absolute;
                right: 12px;
                bottom: 12px;
                padding: 5px 9px;
                border-radius: 5px;
                background: #0009;
                color: #fff;
                font-size: var(--font-size-sm);
              }
            }
            .task-placeholder {
              display: flex;
              min-height: 260px;
              padding: 28px;
              flex-direction: column;
              align-items: center;
              justify-content: center;
              gap: 14px;
              text-align: center;
              background: var(--color-panel-soft);
              color: var(--color-text-muted);
              .placeholder-title {
                font-size: var(--font-size-base);
              }
              .placeholder-hint {
                font-size: var(--font-size-sm);
                color: var(--color-text-soft);
              }
            }
            .task-error {
              margin: 0;
              padding: 12px 14px 0;
              color: var(--color-danger);
              font-size: var(--font-size-sm);
              overflow-wrap: anywhere;
            }
            .task-actions {
              display: flex;
              flex-wrap: wrap;
              gap: 12px 20px;
              padding: 14px;
            }
          }
        }
      }
      .pagination {
        display: flex;
        flex-shrink: 0;
        justify-content: flex-end;
        align-items: center;
        gap: 12px;
        padding: 18px;
        color: var(--color-text-muted);
        .page-size-control {
          display: flex;
          align-items: center;
          gap: 6px;
          margin-right: auto;
          white-space: nowrap;
          .page-size-select {
            height: 31px;
            padding: 0 6px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            color: var(--color-text);
            background: var(--color-panel-soft);
            font-size: var(--font-size-sm);
          }
        }
      }
    }
  }
  .action-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 31px;
    padding: 0 10px;
    border: 1px solid var(--color-line);
    border-radius: 6px;
    color: var(--color-text);
    background: var(--color-panel-soft);
    cursor: pointer;
    font-size: var(--font-size-sm);
    &.danger-button {
      color: var(--color-danger);
    }
    &:disabled {
      opacity: 0.45;
      cursor: not-allowed;
    }
  }
  .text-button {
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--color-primary);
    font-size: var(--font-size-sm);
    cursor: pointer;
    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }
  .spinning {
    animation: image-workbench-spin 1.5s linear infinite;
  }
  .image-detail-modal {
    :deep(.base-modal__panel) {
      width: min(1000px, calc(100vw - 48px));
    }
  }
  .detail-content {
    overflow: auto;
    .detail-prompt {
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      line-height: 1.8;
    }
    .detail-error {
      color: var(--color-danger);
    }
    .detail-images {
      display: flex;
      flex-wrap: wrap;
      gap: 16px;
      .detail-image-card {
        display: grid;
        grid-template-columns: minmax(0, 3fr) minmax(0, 1fr);
        margin: 0;
        width: 100%;
        height: min(60vh, 640px);
        .detail-image {
          display: block;
          box-sizing: border-box;
          min-height: 0;
          width: 100%;
          height: 100%;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel-soft);
        }
        .detail-caption {
          display: flex;
          flex-direction: column;
          gap: 16px;
          min-width: 0;
          min-height: 0;
          padding-left: 16px;
          .detail-image-info {
            flex: 1;
            min-height: 0;
            overflow: auto;
            .detail-info-title {
              margin: 0 0 12px;
              color: var(--color-text);
            }
            .detail-info-list {
              display: grid;
              gap: 12px;
              margin: 0;
              font-size: var(--font-size-sm);
              .detail-info-field {
                min-width: 0;
                .detail-info-label {
                  margin-bottom: 4px;
                  color: var(--color-text-muted);
                }
                .detail-info-value {
                  margin: 0;
                  overflow-wrap: anywhere;
                  color: var(--color-text);
                }
              }
            }
          }
          .detail-image-actions {
            display: flex;
            flex: none;
            flex-direction: column;
            gap: 8px;
            padding-top: 16px;
            border-top: 1px solid var(--color-line);
          }
        }
      }
    }
    .detail-meta {
      display: flex;
      flex-wrap: wrap;
      gap: 12px 22px;
      padding: 16px 0;
      color: var(--color-text-muted);
    }
    .technical-detail {
      color: var(--color-text-muted);
      .usage-content {
        white-space: pre-wrap;
        overflow-wrap: anywhere;
        padding: 12px;
        border-radius: 6px;
        background: var(--color-panel-soft);
        font-size: var(--font-size-sm);
      }
    }
    .detail-hint {
      color: var(--color-text-soft);
    }
  }
}
@keyframes image-workbench-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
