<template>
  <section class="image-workbench">
    <div
      v-if="refreshing || loadingTasks"
      class="workbench-top-loader"
      title="正在加载任务..."
    />
    <header class="conversation-toolbar">
      <div class="conversation-toolbar-left">
        <button
          class="conversation-button conversation-history-btn"
          :class="{ active: historyOpen }"
          type="button"
          title="查看历史会话"
          @click="historyOpen = !historyOpen"
        >
          <Clock :size="14" />
          <span>历史会话</span>
        </button>
        <button
          class="conversation-button conversation-library-btn"
          :class="{ active: promptLibraryOpen }"
          type="button"
          title="打开社区提示词库"
          @click="promptLibraryOpen = true"
        >
          <BookOpen :size="14" />
          <span>提示词库</span>
        </button>
        <div class="conversation-chip" :title="currentConversation?.title">
          <span class="conversation-chip-title">{{
            currentConversation?.title || "新对话"
          }}</span>
        </div>
        <div
          v-if="batchViewVisible && form.generationMode === 'web'"
          class="batch-quota-compact"
          :title="
            quotaError || `Web 额度，${quotaResetLabel || '恢复时间未知'}`
          "
        >
          <Database :size="13" />
          <span class="batch-quota-label">Web 额度</span>
          <span class="batch-quota-value">{{
            webQuota?.remaining ?? (loadingQuota ? "查询中" : "—")
          }}</span>
          <span class="batch-quota-unit">次</span>
          <button
            class="batch-quota-refresh"
            type="button"
            title="刷新 Web 额度"
            :disabled="loadingQuota || !form.accountId"
            @click="refreshQuota"
          >
            <RefreshCw :size="12" :class="{ spinning: loadingQuota }" />
          </button>
        </div>
      </div>
      <div class="conversation-toolbar-right">
        <button
          class="conversation-button conversation-rename-btn"
          type="button"
          :disabled="!currentConversation"
          @click="renameConversation(currentConversation)"
        >
          <Pencil :size="13" />
          <span>重命名</span>
        </button>
        <button
          class="conversation-button conversation-new-btn"
          type="button"
          @click="newConversation"
        >
          <Plus :size="14" />
          <span>新建对话</span>
        </button>
        <button
          class="conversation-button conversation-batch-btn"
          type="button"
          title="导入表格并批量生成图片"
          @click="openBatchDialog"
        >
          <Plus :size="14" />
          <span>批量生成</span>
        </button>
      </div>
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
        v-if="!batchViewVisible"
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
          <div class="panel-head-left">
            <div class="panel-head-badge">
              <Sparkles :size="18" />
            </div>
            <div class="panel-head-info">
              <span class="panel-head-title">创建任务</span>
              <span class="panel-head-sub">配置生成参数，快速创建图片任务</span>
            </div>
          </div>
        </header>

        <!-- 步骤 1: 官方账号 -->
        <div class="workflow-step">
          <div class="workflow-step-head">
            <div class="workflow-step-left">
              <span class="workflow-step-num">1</span>
              <span class="workflow-step-title">官方账号</span>
            </div>
          </div>
          <div class="workflow-step-body">
            <div class="custom-select-wrapper">
              <select
                v-model="form.accountId"
                class="field-input custom-select"
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
              <ChevronDown :size="14" class="custom-select-arrow" />
            </div>
            <p
              v-if="!loadingAccounts && !availableAccounts.length"
              class="account-hint"
            >
              请先在 Provider 页面登录或恢复 Codex 官方账号。其他 Provider
              暂不支持图片生成。
            </p>
          </div>
        </div>

        <!-- 步骤 2: Web 生成额度 -->
        <div v-if="form.generationMode === 'web'" class="workflow-step">
          <div class="workflow-step-head">
            <div class="workflow-step-left">
              <span class="workflow-step-num">2</span>
              <span class="workflow-step-title">Web 生成额度</span>
            </div>
            <button
              class="step-action-link"
              type="button"
              :disabled="loadingQuota || !form.accountId || submitting"
              @click="refreshQuota"
            >
              <RefreshCw :size="12" :class="{ spinning: loadingQuota }" />
              <span>刷新额度</span>
            </button>
          </div>
          <div class="workflow-step-body">
            <div class="quota-telemetry-card">
              <div class="quota-telemetry-top">
                <div class="quota-telemetry-left">
                  <div class="quota-telemetry-icon-box">
                    <Database :size="18" />
                  </div>
                  <div class="quota-telemetry-info">
                    <div class="quota-remaining-row">
                      <span class="quota-label">剩余</span>
                      <span class="quota-value">{{
                        webQuota?.remaining ?? (loadingQuota ? "…" : "—")
                      }}</span>
                      <span class="quota-unit">次</span>
                    </div>
                    <div class="quota-progress-track">
                      <div
                        class="quota-progress-bar"
                        :style="{ width: `${quotaProgressPercent || 70}%` }"
                      ></div>
                    </div>
                  </div>
                </div>
                <div class="quota-telemetry-meta">
                  <span class="quota-meta-line">
                    额度恢复:
                    {{ quotaResetLabel || (loadingQuota ? "查询中" : "—") }}
                  </span>
                  <span class="quota-meta-line">
                    更新于
                    {{
                      webQuota
                        ? formatDateTime(webQuota.updatedAt)
                        : loadingQuota
                          ? "查询中"
                          : "—"
                    }}
                  </span>
                </div>
              </div>
              <p v-if="quotaError" class="quota-error-text">{{ quotaError }}</p>
            </div>
          </div>
        </div>

        <!-- 步骤 3: 生成模型 -->
        <div class="workflow-step">
          <div class="workflow-step-head">
            <div class="workflow-step-left">
              <span class="workflow-step-num">3</span>
              <span class="workflow-step-title"
                >生成模型 <span class="field-key">model</span></span
              >
            </div>
            <button
              class="step-action-link"
              type="button"
              :disabled="loadingModels || !form.accountId"
              title="刷新可用模型列表"
              @click="loadModels"
            >
              <RefreshCw :size="12" :class="{ spinning: loadingModels }" />
              <span>刷新模型</span>
            </button>
          </div>
          <div class="workflow-step-body">
            <div class="custom-select-wrapper">
              <select
                v-model="form.model"
                class="field-input custom-select model-dropdown"
              >
                <option v-if="!imageModels.length" :value="form.model">
                  {{ form.model || "gpt-image-2.5-sunburst" }}
                </option>
                <option
                  v-for="model in imageModels"
                  :key="model.id"
                  :value="model.id"
                >
                  {{ model.id }}
                </option>
              </select>
              <ChevronDown :size="14" class="custom-select-arrow" />
            </div>

            <!-- 可用模型软胶囊选择器 -->
            <div
              class="model-picker-list"
              role="listbox"
              aria-label="可用模型列表"
            >
              <button
                v-for="model in displayModels"
                :key="model.id"
                type="button"
                class="model-picker-item"
                :class="{ active: form.model === model.id }"
                :title="model.id"
                @click="form.model = model.id"
              >
                <span class="model-picker-dot"></span>
                <span class="model-picker-name">{{ model.id }}</span>
              </button>
            </div>
            <p v-if="modelsError" class="field-error">{{ modelsError }}</p>
          </div>
        </div>

        <!-- 步骤 4: 提示词 -->
        <div class="workflow-step">
          <div class="workflow-step-head">
            <div class="workflow-step-left">
              <span class="workflow-step-num">4</span>
              <span class="workflow-step-title"
                >提示词 <span class="field-key">prompt</span></span
              >
            </div>
            <button
              class="step-settings-toggle"
              type="button"
              title="配置生成尺寸、画质、张数与参考图"
              @click="settingsOpen = true"
            >
              <SlidersHorizontal :size="13" />
              <span>{{ width }}×{{ height }} · {{ form.n }}张</span>
            </button>
          </div>
          <div class="workflow-step-body">
            <div class="prompt-box-container">
              <textarea
                v-model="form.prompt"
                class="field-input prompt-textarea"
                :placeholder="
                  references.length
                    ? '描述你希望如何修改参考图'
                    : '输入你想要生成的画面，也可直接粘贴图片'
                "
                @keydown="promptKeydown"
                @paste="pasteImages"
                maxlength="2000"
                required
              ></textarea>
              <div class="prompt-counter">
                {{ (form.prompt || "").length }}/2000
              </div>
            </div>

            <!-- 操作按钮栏 -->
            <div class="prompt-actions-bar">
              <div class="prompt-actions-left">
                <button
                  class="prompt-action-btn"
                  type="button"
                  :disabled="!form.prompt"
                  @click="form.prompt = ''"
                >
                  <Trash2 :size="14" />
                  <span>清空</span>
                </button>
                <button
                  class="prompt-action-btn"
                  type="button"
                  title="草图绘制"
                  @click="drawing = { source: '' }"
                >
                  <Paintbrush :size="14" />
                  <span>草图</span>
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- 步骤 5: 参考图与蒙版 -->
        <div class="workflow-step workflow-step--references">
          <div class="workflow-step-head">
            <div class="workflow-step-left">
              <span class="workflow-step-num">5</span>
              <span class="workflow-step-title">
                参考图与蒙版
                <span class="field-key">references & mask</span>
              </span>
              <span v-if="references.length" class="step-mode-pill edit">
                图片编辑模式 ({{ references.length }}/10)
              </span>
              <span v-else class="step-mode-pill text">
                可选 · {{ references.length }}/10
              </span>
            </div>
            <div v-if="references.length" class="workflow-step-right">
              <button
                class="step-action-link danger-link"
                type="button"
                title="清空已上传的参考图和蒙版"
                @click="clearAllReferences"
              >
                <Trash2 :size="12" />
                <span>清空参考图</span>
              </button>
            </div>
          </div>
          <div class="workflow-step-body">
            <!-- 参考图展示与手动上传列表 -->
            <div class="reference-gallery-list">
              <div
                v-for="(reference, index) in references"
                :key="reference.id"
                class="reference-thumb-card"
              >
                <el-image
                  class="reference-thumb-img"
                  :src="reference.url"
                  :alt="reference.name"
                  :preview-src-list="references.map((item) => item.url)"
                  :initial-index="index"
                  fit="cover"
                  preview-teleported
                />
                <span class="reference-index-tag">#{{ index + 1 }}</span>
                <button
                  class="remove-reference-btn"
                  type="button"
                  :title="`移除 ${reference.name}`"
                  :aria-label="`移除 ${reference.name}`"
                  @click="removeReference(index)"
                >
                  <X :size="12" />
                </button>
                <span class="reference-thumb-name" :title="reference.name">{{
                  reference.name
                }}</span>
              </div>

              <!-- 上传参考图卡片：未满 10 张时始终显示，用户可直接点击选择或拖入 -->
              <label
                v-if="references.length < 10"
                class="reference-upload-card"
                :class="{ 'is-disabled': uploading }"
              >
                <div class="upload-card-content">
                  <ImagePlus :size="20" class="upload-icon" />
                  <span class="upload-text">{{
                    references.length ? "继续添加" : "添加参考图"
                  }}</span>
                  <span class="upload-sub">点击或拖入</span>
                </div>
                <input
                  class="upload-file-input"
                  type="file"
                  accept="image/png,image/jpeg,image/webp"
                  multiple
                  :disabled="uploading"
                  @change="addImages($event, false)"
                />
              </label>
            </div>

            <!-- 说明文字与快捷提示 -->
            <div class="reference-hint-line">
              <span
                >支持 PNG / JPEG / WebP，单张 ≤ 20 MB。支持直接截图后使用 Ctrl+V
                粘贴。</span
              >
            </div>

            <!-- 重绘蒙版管理卡片 -->
            <div class="mask-manager-card">
              <div v-if="mask" class="mask-active-row">
                <div class="mask-badge-content">
                  <span class="mask-pill-tag">已应用蒙版</span>
                  <span class="mask-name-text" :title="mask.name">{{
                    mask.name
                  }}</span>
                  <span class="mask-hint-text">透明区域为编辑重绘范围</span>
                </div>
                <div class="mask-actions">
                  <label class="mask-action-btn">
                    <RefreshCw :size="12" />
                    <span>更换</span>
                    <input
                      class="upload-file-input"
                      type="file"
                      accept="image/png"
                      :disabled="uploading"
                      @change="addImages($event, true)"
                    />
                  </label>
                  <button
                    class="mask-action-btn danger"
                    type="button"
                    @click="mask = null"
                  >
                    <X :size="12" />
                    <span>移除</span>
                  </button>
                </div>
              </div>

              <div v-else class="mask-empty-row">
                <div class="mask-prompt-left">
                  <span class="mask-label">重绘蒙版 (可选)</span>
                  <span class="mask-desc"
                    >仅支持
                    PNG；透明区域为重绘区域，尺寸须与首张参考图一致</span
                  >
                </div>
                <div class="mask-prompt-right">
                  <label
                    class="mask-upload-btn"
                    :class="{ 'is-disabled': uploading || !references.length }"
                    :title="
                      !references.length
                        ? '请先上传首张参考图'
                        : '上传透明 PNG 蒙版'
                    "
                  >
                    <Plus :size="13" />
                    <span>上传蒙版 (PNG)</span>
                    <input
                      class="upload-file-input"
                      type="file"
                      accept="image/png"
                      :disabled="uploading || !references.length"
                      @change="addImages($event, true)"
                    />
                  </label>
                  <button
                    class="mask-sketch-btn"
                    type="button"
                    :disabled="!references.length"
                    title="在首张参考图上涂抹绘制重绘蒙版"
                    @click="drawing = { source: references[0]?.url || '' }"
                  >
                    <Paintbrush :size="13" />
                    <span>在图上涂抹</span>
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 底部生成操作栏 -->
        <div class="workflow-submit-footer">
          <button
            class="workflow-submit-btn"
            type="submit"
            :disabled="!canSubmit"
          >
            <LoaderCircle
              v-if="submitting || uploading"
              class="spinning"
              :size="16"
            />
            <Sparkles v-else-if="references.length" :size="16" />
            <ImageIcon v-else :size="16" />
            <span>{{
              submitting
                ? "正在生成…"
                : references.length
                  ? "立即编辑生成"
                  : "立即生成"
            }}</span>
          </button>
          <p
            v-if="submitError"
            class="field-error submit-footer-error"
            role="alert"
          >
            {{ submitError }}
          </p>
        </div>
      </form>
      <BatchQueuePanel
        v-else
        :tasks="tasks"
        :expected-total="batchExpectedTotal"
        :loading="loadingTasks"
        :paused="batchPaused"
        :concurrency="batchConcurrency"
        :stopping="batchQueueStopping"
        :deleting="deleting"
        :resuming-ids="resuming"
        :status-labels="statusLabels"
        v-model:selected-ids="batchQueueSelected"
        v-model:status-filter="batchQueueStatusFilter"
        v-model:page="batchQueuePage"
        v-model:page-size="batchQueuePageSize"
        :style="{
          flexBasis: `calc(${leftWidth}% - ${(18 * leftWidth) / 100}px)`
        }"
        @toggle-pause="toggleBatchPause"
        @resume-selected="resumeSelectedBatchTasks"
        @cancel-selected="cancelSelectedBatchTasks"
        @delete-selected="deleteBatchQueueTasks"
        @set-concurrency="setBatchConcurrency"
        @cancel-task="cancelSelectedBatchTasks([$event])"
        @resume-task="resumeTask"
        @show-detail="showDetail"
      />
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
      <section
        v-if="!batchViewVisible"
        id="image-tasks-panel"
        class="tasks-panel"
      >
        <!-- 遮罩层 (Loading Overlay) -->
        <Transition name="workbench-fade">
          <div v-if="loadingTasks" class="workbench-loading-overlay">
            <LoaderCircle :size="32" class="spinning" />
            <span class="overlay-text">正在读取任务...</span>
          </div>
        </Transition>

        <header class="tasks-head">
          <div class="tasks-head-left">
            <div class="tasks-head-icon-box">
              <MessageSquare :size="18" />
            </div>
            <div class="tasks-head-info">
              <div class="tasks-head-title-row">
                <span class="tasks-head-title">当前会话</span>
                <span class="tasks-head-round-badge"
                  >{{ total || rounds.length }} 轮</span
                >
              </div>
              <span class="tasks-head-sub"
                >查看任务生成结果，支持复制、复用或重新生成。</span
              >
            </div>
          </div>
          <div class="tasks-controls">
            <div class="custom-select-wrapper tasks-status-wrapper">
              <select
                v-model="status"
                class="status-select custom-select"
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
            </div>
            <label class="auto-refresh-pill" :class="{ active: autoRefresh }">
              <input v-model="autoRefresh" type="checkbox" />
              <span class="status-dot"></span>
              <span>自动刷新</span>
            </label>
            <button
              class="tasks-icon-button"
              type="button"
              aria-label="刷新账号与任务"
              title="刷新账号与任务"
              :disabled="refreshing"
              @click="refreshAll"
            >
              <RefreshCw :size="15" :class="{ spinning: refreshing }" />
            </button>
            <button
              class="tasks-select-toggle-btn"
              :class="{ active: selecting }"
              type="button"
              @click="toggleSelection"
            >
              <CheckSquare v-if="selecting" :size="14" />
              <Square v-else :size="14" />
              <span>选择</span>
            </button>
          </div>
        </header>

        <div v-if="selecting" class="selection-bar">
          <label class="selection-check-all">
            <input
              type="checkbox"
              :checked="allSelected"
              :disabled="!selectableTasks.length"
              @change="toggleAll"
            />
            <span>本页全选</span>
          </label>
          <span class="selection-count"
            >已选 <strong>{{ selected.length }}</strong> 项</span
          >
          <div class="selection-actions">
            <button
              class="action-button"
              type="button"
              :disabled="!selected.length || exporting"
              @click="exportTasks(selected)"
            >
              <Download :size="14" />
              <span>导出</span>
            </button>
            <button
              class="action-button danger-button"
              type="button"
              :disabled="!selected.length || deleting"
              @click="deleteTasks"
            >
              <Trash2 :size="14" />
              <span>删除</span>
            </button>
          </div>
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
          <section
            v-for="(round, roundIndex) in rounds"
            :key="round.id"
            class="round-card"
          >
            <header class="round-card-head">
              <div class="round-card-head-left">
                <span class="round-num-pill"
                  ># {{ rounds.length - roundIndex }}</span
                >
                <div
                  v-if="round.tasks[0]?.request.model || round.tasks[0]?.id"
                  class="round-model-chip"
                  :title="`模型: ${round.tasks[0].request.model || round.tasks[0].id}`"
                >
                  <span class="round-chip-label">任务ID</span>
                  <span class="round-chip-value">{{
                    round.tasks[0].request.model || round.tasks[0].id
                  }}</span>
                </div>
                <span
                  v-if="round.tasks[0]?.request.size"
                  class="round-tag-badge"
                  :title="`尺寸: ${round.tasks[0].request.size}`"
                >
                  {{ round.tasks[0].request.size }}
                </span>
                <span class="round-tag-badge round-tag-mode">
                  {{
                    round.tasks[0]?.request.generationMode === "web"
                      ? "WEB"
                      : "CODEX"
                  }}
                </span>
              </div>
              <div class="round-card-head-right">
                <span class="round-date-text">
                  {{ formatDateTime(round.tasks[0]?.createdAt) }}
                </span>
                <el-dropdown
                  trigger="click"
                  @command="handleRoundMenuCommand($event, round)"
                >
                  <button
                    class="round-menu-trigger-btn"
                    type="button"
                    title="更多操作"
                  >
                    <MoreVertical :size="16" />
                  </button>
                  <template #dropdown>
                    <el-dropdown-menu class="round-dropdown-menu">
                      <el-dropdown-item command="copy"
                        >复制提示词</el-dropdown-item
                      >
                      <el-dropdown-item command="reuse"
                        >复用配置</el-dropdown-item
                      >
                      <el-dropdown-item command="regenerate"
                        >全部重新生成</el-dropdown-item
                      >
                      <el-dropdown-item
                        v-if="roundCanResume(round)"
                        command="resume"
                        >恢复未完成</el-dropdown-item
                      >
                      <el-dropdown-item command="deletePrompt"
                        >删除提示词</el-dropdown-item
                      >
                      <el-dropdown-item
                        command="delete"
                        divided
                        class="danger-menu-item"
                        >删除结果</el-dropdown-item
                      >
                    </el-dropdown-menu>
                  </template>
                </el-dropdown>
              </div>
            </header>

            <div
              v-if="
                !roundState[round.id]?.hidePrompt &&
                round.tasks[0]?.request.prompt
              "
              class="round-prompt-box"
            >
              <span
                v-if="round.tasks[0]?.request.batchName"
                class="round-batch-name"
              >
                {{ round.tasks[0].request.batchName }}
              </span>
              <p
                class="round-prompt-text"
                :class="{
                  'is-clamped':
                    isLongPrompt(round.tasks[0].request.prompt) &&
                    !isPromptExpanded(round.id)
                }"
              >
                {{ round.tasks[0].request.prompt }}
              </p>
              <button
                v-if="isLongPrompt(round.tasks[0].request.prompt)"
                class="prompt-expand-btn"
                type="button"
                @click="togglePrompt(round.id)"
              >
                <component
                  :is="isPromptExpanded(round.id) ? ChevronUp : ChevronDown"
                  :size="13"
                />
                <span>{{
                  isPromptExpanded(round.id) ? "收起提示词" : "展开完整提示词"
                }}</span>
              </button>
            </div>

            <div class="round-quick-actions">
              <button
                v-if="round.tasks[0]?.request.prompt"
                class="round-quick-btn"
                type="button"
                :title="
                  copiedRoundId === round.id ? '已复制到剪切板' : '复制提示词'
                "
                @click="copyPrompt(round.tasks[0].request.prompt, round.id)"
              >
                <Check
                  v-if="copiedRoundId === round.id"
                  :size="13"
                  class="copy-success-icon"
                />
                <Copy v-else :size="13" />
                <span>{{
                  copiedRoundId === round.id ? "已复制" : "复制提示词"
                }}</span>
              </button>
              <button
                class="round-quick-btn"
                type="button"
                title="复用此轮配置到输入表单"
                @click="reuseTask(round.tasks[0], true)"
              >
                <SlidersHorizontal :size="13" />
                <span>复用配置</span>
              </button>
              <button
                class="round-quick-btn"
                type="button"
                :disabled="submitting"
                title="按此轮提示词与设置全部重新生成"
                @click="regenerateRound(round)"
              >
                <RefreshCw :size="13" :class="{ spinning: submitting }" />
                <span>全部重新生成</span>
              </button>
              <button
                class="round-quick-btn round-quick-btn--danger"
                type="button"
                title="删除此轮生成的所有结果"
                @click="removeRoundResults(round)"
              >
                <Trash2 :size="13" />
                <span>删除结果</span>
              </button>
            </div>

            <div class="round-tasks-grid">
              <article
                v-for="task in round.tasks"
                :key="task.id"
                class="image-task-card"
                :class="{
                  'task-selected': selected.includes(task.id),
                  'task-has-error':
                    task.error?.message &&
                    !roundState[round.id]?.ignored?.includes(task.id)
                }"
              >
                <header class="image-task-status-bar">
                  <div class="image-task-status-left">
                    <label
                      v-if="selecting"
                      class="task-check"
                      :aria-label="`选择任务 ${formatDateTime(task.createdAt)}`"
                    >
                      <input
                        v-model="selected"
                        type="checkbox"
                        :value="task.id"
                        :disabled="task.status === 'processing'"
                      />
                    </label>
                    <div class="image-task-status-pill" :class="task.status">
                      <LoaderCircle
                        v-if="task.status === 'processing'"
                        class="spinning"
                        :size="12"
                      />
                      <Clock v-else-if="task.status === 'queued'" :size="12" />
                      <AlertCircle
                        v-else-if="
                          ['failed', 'interrupted'].includes(task.status)
                        "
                        :size="12"
                      />
                      <CheckCircle2
                        v-else-if="
                          task.status === 'succeeded' ||
                          task.status === 'completed'
                        "
                        :size="13"
                      />
                      <span v-else class="status-indicator-dot"></span>
                      <span class="status-text">{{
                        statusLabels[task.status] ||
                        (task.status === "succeeded" ? "已完成" : task.status)
                      }}</span>
                    </div>
                    <span
                      class="image-task-time"
                      :title="formatDateTime(task.createdAt)"
                    >
                      <Clock :size="12" />
                      <span>{{ formatDateTime(task.createdAt) }}</span>
                    </span>
                  </div>
                  <div class="image-task-status-right">
                    <span class="image-task-count">
                      {{ task.imageCount }} / {{ task.request.n }} 张
                    </span>
                  </div>
                </header>

                <div
                  v-if="task.imageCount"
                  class="image-hero-preview"
                  :class="{ 'is-loading': detailLoading }"
                >
                  <el-image
                    class="image-hero-img"
                    :src="task.thumbnail"
                    fit="contain"
                    lazy
                  >
                    <template #placeholder>
                      <div class="preview-placeholder">
                        <LoaderCircle class="spinning" :size="28" />
                        <span>正在加载图片…</span>
                      </div>
                    </template>
                    <template #error>
                      <div class="preview-placeholder error">
                        <ImageOff :size="28" />
                        <span>预览暂不可用，点击查看原图</span>
                      </div>
                    </template>
                  </el-image>
                  <div
                    class="image-hero-overlay"
                    @click.self="showDetail(task, 'images')"
                  >
                    <button
                      class="overlay-hero-view-btn"
                      type="button"
                      :disabled="detailLoading"
                      @click="showDetail(task, 'images')"
                    >
                      <Maximize2 :size="14" />
                      <span>查看原图</span>
                    </button>
                    <div class="overlay-hero-actions">
                      <button
                        class="overlay-action-chip"
                        type="button"
                        title="查看参数"
                        :disabled="detailLoading"
                        @click.stop="showDetail(task, 'parameters')"
                      >
                        <Info :size="13" />
                        <span>查看参数</span>
                      </button>
                      <button
                        class="overlay-action-chip"
                        type="button"
                        title="复用参数"
                        @click.stop="reuseTask(task)"
                      >
                        <SlidersHorizontal :size="13" />
                        <span>复用参数</span>
                      </button>
                      <button
                        class="overlay-action-chip"
                        type="button"
                        title="导出图片"
                        :disabled="exporting"
                        @click.stop="exportTasks([task.id])"
                      >
                        <Download :size="13" />
                        <span>导出图片</span>
                      </button>
                    </div>
                    <span v-if="task.imageCount > 1" class="overlay-multi-hint">
                      共 {{ task.imageCount }} 张
                    </span>
                  </div>
                </div>

                <div
                  v-else
                  class="task-placeholder"
                  role="status"
                  :class="task.status"
                >
                  <template
                    v-if="['processing', 'queued'].includes(task.status)"
                  >
                    <div class="pulse-loader-ring">
                      <LoaderCircle class="spinning" :size="32" />
                    </div>
                    <span class="placeholder-title">{{
                      task.status === "queued" ? "排队中…" : "AI 正在绘制画面…"
                    }}</span>
                    <span class="placeholder-hint">{{
                      autoRefresh
                        ? "完成后会自动显示在这里"
                        : "完成后点击上方刷新查看结果"
                    }}</span>
                  </template>
                  <template v-else>
                    <div class="empty-icon-box">
                      <ImageOff :size="32" :stroke-width="1.3" />
                    </div>
                    <span class="placeholder-title"
                      >{{
                        statusLabels[task.status] || task.status
                      }}，暂无生成图片</span
                    >
                    <span class="placeholder-hint">可以复用参数重新生成</span>
                  </template>
                </div>

                <div
                  v-if="
                    task.error?.message &&
                    !roundState[round.id]?.ignored?.includes(task.id)
                  "
                  class="task-error-box"
                >
                  <AlertCircle :size="14" class="error-icon" />
                  <p class="task-error-text">{{ task.error.message }}</p>
                </div>

                <footer
                  v-if="
                    !task.imageCount ||
                    canResumeImageTask(task) ||
                    (task.error?.message &&
                      !roundState[round.id]?.ignored?.includes(task.id))
                  "
                  class="task-aux-actions"
                >
                  <button
                    v-if="!task.imageCount"
                    class="task-btn task-btn-primary"
                    type="button"
                    :disabled="
                      submitting ||
                      ['queued', 'processing'].includes(task.status)
                    "
                    title="以此任务参数重新生成"
                    @click="regenerateTask(task)"
                  >
                    <RefreshCw :size="13" />
                    <span>重新生成</span>
                  </button>
                  <button
                    v-if="canResumeImageTask(task)"
                    class="task-btn task-btn-warning"
                    type="button"
                    :disabled="resuming.includes(task.id)"
                    title="继续等待任务完成"
                    @click="resumeTask(task)"
                  >
                    <RotateCcw :size="13" />
                    <span>继续等待</span>
                  </button>
                  <button
                    v-if="
                      task.error?.message &&
                      !roundState[round.id]?.ignored?.includes(task.id)
                    "
                    class="task-btn task-btn-ghost"
                    type="button"
                    title="忽略此错误信息"
                    @click="ignoreTaskError(round, task)"
                  >
                    <X :size="13" />
                    <span>忽略错误</span>
                  </button>
                </footer>
              </article>
            </div>
          </section>
        </div>

        <footer class="tasks-pagination">
          <div class="tasks-pagination-left">
            <span class="pagination-label">每页</span>
            <div class="custom-select-wrapper pagination-select-wrapper">
              <select
                v-model.number="pageSize"
                class="pagination-select custom-select"
                aria-label="每页显示条数"
              >
                <option
                  v-for="size in pageSizeOptions"
                  :key="size"
                  :value="size"
                >
                  {{ size }}
                </option>
              </select>
            </div>
            <span class="pagination-label">条</span>
          </div>
          <div class="tasks-pagination-right">
            <button
              class="pagination-nav-btn"
              type="button"
              :disabled="page <= 1"
              title="上一页"
              @click="page--"
            >
              <ChevronLeft :size="14" />
              <span>上一页</span>
            </button>
            <span class="pagination-indicator">
              {{ page }} / {{ Math.max(1, Math.ceil(total / pageSize)) }}
            </span>
            <button
              class="pagination-nav-btn"
              type="button"
              :disabled="page * pageSize >= total"
              title="下一页"
              @click="page++"
            >
              <span>下一页</span>
              <ChevronRight :size="14" />
            </button>
          </div>
        </footer>
      </section>
      <section v-else class="batch-results-panel">
        <!-- 遮罩层 (Loading Overlay) -->
        <Transition name="workbench-fade">
          <div v-if="loadingTasks" class="workbench-loading-overlay">
            <LoaderCircle :size="32" class="spinning" />
            <span class="overlay-text">正在加载生成结果...</span>
          </div>
        </Transition>

        <header class="batch-results-header">
          <div class="batch-results-header-left">
            <h3 class="batch-results-title">生成结果</h3>
            <span class="batch-results-counter">
              {{ filteredResultTasks.length }} 项
            </span>
          </div>

          <div class="batch-results-toolbar">
            <!-- 搜索框 -->
            <div class="batch-search-box">
              <Search :size="13" class="search-icon" />
              <input
                v-model="batchResultSearch"
                type="text"
                class="batch-search-input"
                placeholder="搜索图片名称或提示词..."
              />
              <button
                v-if="batchResultSearch"
                type="button"
                class="search-clear-btn"
                @click="batchResultSearch = ''"
              >
                <X :size="12" />
              </button>
            </div>

            <!-- 视图切换 (Grid / List) -->
            <div class="batch-view-toggle">
              <button
                type="button"
                class="view-toggle-btn"
                :class="{ 'is-active': batchResultViewMode === 'grid' }"
                title="网格视图"
                @click="batchResultViewMode = 'grid'"
              >
                <Grid :size="14" />
              </button>
              <button
                type="button"
                class="view-toggle-btn"
                :class="{ 'is-active': batchResultViewMode === 'list' }"
                title="列表视图"
                @click="batchResultViewMode = 'list'"
              >
                <List :size="14" />
              </button>
            </div>

            <!-- 状态过滤下拉 -->
            <div class="custom-select-wrapper is-mini status-filter-select">
              <select
                v-model="batchResultStatus"
                class="custom-select mini-select"
              >
                <option value="completed">已完成</option>
                <option value="">全部状态</option>
                <option value="processing">进行中</option>
                <option value="queued">排队中</option>
                <option value="failed">失败</option>
              </select>
              <ChevronDown :size="12" class="custom-select-arrow" />
            </div>
          </div>
        </header>

        <!-- 结果主区域：网格视图或列表视图 -->
        <div ref="batchResultScroll" class="batch-results-body">
          <div
            v-if="paginatedResultTasks.length && batchResultViewMode === 'grid'"
            class="batch-card-grid"
          >
            <article
              v-for="task in paginatedResultTasks"
              :key="task.id"
              class="batch-result-card"
            >
              <!-- 图片缩略图容器 -->
              <div
                class="result-card-media"
                @click="task.imageCount ? showDetail(task, 'images') : null"
              >
                <el-image
                  v-if="task.thumbnail && task.imageCount"
                  :src="task.thumbnail"
                  fit="cover"
                  lazy
                  class="result-card-img"
                />
                <div
                  v-else-if="['queued', 'processing'].includes(task.status)"
                  class="result-card-loading"
                >
                  <LoaderCircle :size="24" class="spinning" />
                  <span>正在绘制...</span>
                </div>
                <div v-else class="result-card-empty">
                  <ImageOff :size="24" />
                  <span>无图片</span>
                </div>

                <!-- 浮动状态徽章 -->
                <div
                  class="result-card-badge"
                  :class="`is-${batchStatusTone(task.status)}`"
                >
                  <span class="badge-dot" />
                  <span>{{ statusLabels[task.status] || task.status }}</span>
                </div>
              </div>

              <!-- 卡片信息 -->
              <div class="result-card-content">
                <h4
                  class="result-card-title"
                  :title="task.request.batchName || '未命名任务'"
                >
                  {{ task.request.batchName || "未命名任务" }}
                </h4>
                <p class="result-card-prompt" :title="task.request.prompt">
                  {{ task.request.prompt }}
                </p>

                <!-- 底部快捷操作栏 -->
                <div class="result-card-actions">
                  <button
                    class="card-act-btn"
                    type="button"
                    title="查看大图"
                    :disabled="!task.imageCount"
                    @click="showDetail(task, 'images')"
                  >
                    <ZoomIn :size="13" />
                  </button>
                  <button
                    class="card-act-btn"
                    type="button"
                    title="下载图片"
                    :disabled="!task.imageCount"
                    @click="downloadTaskImage(task)"
                  >
                    <Download :size="13" />
                  </button>
                  <button
                    class="card-act-btn"
                    type="button"
                    title="复制提示词"
                    @click="copyPromptText(task.request.prompt)"
                  >
                    <Copy :size="13" />
                  </button>
                  <button
                    class="card-act-btn"
                    type="button"
                    title="查看参数"
                    @click="showDetail(task, 'parameters')"
                  >
                    <SlidersHorizontal :size="13" />
                  </button>
                </div>
              </div>
            </article>
          </div>

          <!-- 列表视图 -->
          <div
            v-else-if="
              paginatedResultTasks.length && batchResultViewMode === 'list'
            "
            class="batch-results-list"
          >
            <article
              v-for="task in paginatedResultTasks"
              :key="task.id"
              class="batch-result-item"
              :class="task.status"
            >
              <div
                class="batch-result-thumb"
                @click="task.imageCount ? showDetail(task, 'images') : null"
              >
                <el-image
                  v-if="task.thumbnail && task.imageCount"
                  :src="task.thumbnail"
                  fit="cover"
                  lazy
                />
                <LoaderCircle
                  v-else-if="['queued', 'processing'].includes(task.status)"
                  :size="22"
                  class="batch-result-spinner spinning"
                />
                <ImageOff v-else :size="20" />
              </div>

              <div class="batch-result-main">
                <div class="batch-result-name-row">
                  <span class="batch-result-name">{{
                    task.request.batchName || "未命名任务"
                  }}</span>
                  <span
                    class="queue-status-pill"
                    :class="`is-${batchStatusTone(task.status)}`"
                  >
                    <span class="pill-dot" />
                    <span>{{ statusLabels[task.status] || task.status }}</span>
                  </span>
                </div>
                <span class="batch-result-prompt">{{
                  task.request.prompt
                }}</span>
              </div>

              <div class="batch-result-actions">
                <button
                  v-if="task.imageCount"
                  class="row-action-btn"
                  type="button"
                  title="下载图片"
                  @click="downloadTaskImage(task)"
                >
                  <Download :size="13" />
                </button>
                <button
                  class="row-action-btn"
                  type="button"
                  title="复制提示词"
                  @click="copyPromptText(task.request.prompt)"
                >
                  <Copy :size="13" />
                </button>
                <button
                  class="row-action-btn"
                  type="button"
                  title="查看详情"
                  @click="showDetail(task, 'parameters')"
                >
                  <MoreHorizontal :size="13" />
                </button>
              </div>
            </article>
          </div>

          <!-- 空结果提示 -->
          <div v-else class="batch-empty-results">
            <ImageIcon :size="40" class="empty-icon" />
            <h4>暂无生成结果</h4>
            <p>
              {{
                batchResultSearch
                  ? "未找到符合搜索条件的结果"
                  : "等待批量图片任务生成完成"
              }}
            </p>
          </div>
        </div>

        <!-- 结果底部翻页 -->
        <footer class="batch-results-footer">
          <div class="footer-left">
            <div class="custom-select-wrapper is-mini">
              <select
                v-model.number="batchResultPageSize"
                class="custom-select mini-select"
              >
                <option :value="12">每页 12 条</option>
                <option :value="20">每页 20 条</option>
                <option :value="30">每页 30 条</option>
                <option :value="60">每页 60 条</option>
              </select>
              <ChevronDown :size="12" class="custom-select-arrow" />
            </div>
          </div>

          <div class="footer-right">
            <div class="batch-pagination">
              <button
                class="page-nav-btn"
                type="button"
                :disabled="batchResultPage <= 1"
                title="上一页"
                @click="batchResultPage--"
              >
                <ChevronLeft :size="13" />
              </button>
              <template
                v-for="(p, idx) in getVisiblePages(
                  batchResultPage,
                  batchResultTotalPages
                )"
                :key="idx"
              >
                <span v-if="p === '...'" class="page-ellipsis">...</span>
                <button
                  v-else
                  class="page-num-btn"
                  :class="{ 'is-active': p === batchResultPage }"
                  type="button"
                  @click="batchResultPage = p"
                >
                  {{ p }}
                </button>
              </template>
              <button
                class="page-nav-btn"
                type="button"
                :disabled="batchResultPage >= batchResultTotalPages"
                title="下一页"
                @click="batchResultPage++"
              >
                <ChevronRight :size="13" />
              </button>
            </div>
          </div>
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
    <TaskDetailModal
      v-if="detail"
      :detail="detail"
      :detail-view="detailView"
      :status-labels="statusLabels"
      :format-date-time="formatDateTime"
      :format-quality="formatQuality"
      :image-url="imageUrl"
      :exporting="exporting"
      :submitting="submitting"
      @close="detail = null"
      @edit-result="editResult"
      @reference-result="referenceResult"
      @download-image="downloadImage"
      @reuse="handleReuseFromModal"
      @regenerate="handleRegenerateFromModal"
    />

    <ImageSettingsModal
      v-if="settingsOpen"
      v-model:width="width"
      v-model:height="height"
      v-model:ratio="ratio"
      v-model:tier="tier"
      :form="form"
      :size-presets="sizePresets"
      :quality-options="qualityOptions"
      :fields="fields"
      :description="`${width} × ${height} · ${formatQuality(form.quality)} · ${form.n} 张`"
      @close="settingsOpen = false"
      @reset="resetToDefaultSettings"
      @apply-size="applySizePreset"
    />
    <ImageBatchDialog
      v-if="batchDialogOpen || batchRunning"
      :visible="batchDialogOpen"
      :conversation-id="batchConversationId"
      :settings="batchSettings"
      :models="displayModels"
      :size-presets="sizePresets"
      @close="closeBatchDialog"
      @session-created="createBatchConversation"
      @tasks-created="handleBatchTasksCreated"
      @finished="handleBatchFinished"
    />
  </section>
</template>

<script setup>
import {
  computed,
  defineAsyncComponent,
  nextTick,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  watch
} from "vue"
import {
  AlertCircle,
  BookOpen,
  Check,
  CheckCircle2,
  CheckSquare,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  Clock,
  Copy,
  Database,
  Download,
  Grid,
  Image as ImageIcon,
  ImageOff,
  ImagePlus,
  Info,
  List,
  LoaderCircle,
  Maximize2,
  MessageSquare,
  Minus,
  MoreHorizontal,
  MoreVertical,
  Pause,
  Paintbrush,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  SlidersHorizontal,
  Sparkles,
  Square,
  Trash2,
  X,
  ZoomIn
} from "lucide-vue-next"
import {
  ElDropdown,
  ElDropdownItem,
  ElDropdownMenu,
  ElImage,
  ElMessageBox
} from "element-plus"
import "element-plus/es/components/dropdown/style/css"
import "element-plus/es/components/image/style/css"
import "element-plus/es/components/message-box/style/css"
import BatchQueuePanel from "./components/BatchQueuePanel.vue"
import ImageDrawingDialog from "./components/ImageDrawingDialog.vue"
import ImageSettingsModal from "./components/ImageSettingsModal.vue"
import TaskDetailModal from "./components/TaskDetailModal.vue"
import { systemApi, toolboxApi } from "@/api"
import { subscribe } from "@/api/request"
import { formatDateTime } from "@/utils/formatters"
import { createMessage } from "@/utils/message"
import { readLocal, writeLocal } from "./utils/storage"

// reactive 会解包模式引用，表单回填直接同步到页面头部。
const generationMode = defineModel("generationMode", {
  type: String,
  default: "web"
})

// 词库按需加载，打开时才读取分类和分页内容。
const ImagePromptLibrary = defineAsyncComponent(
  () => import("./components/ImagePromptLibrary.vue")
)
const ImageBatchDialog = defineAsyncComponent(
  () => import("./components/ImageBatchDialog.vue")
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
function canResumeImageTask(task) {
  // 暂停、中断或失败的任务均支持重试或恢复执行
  return ["interrupted", "failed"].includes(task?.status)
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
const batchDialogOpen = ref(false)
const batchConversationId = ref("")
// 记录落库前保留预期总数，避免批量队列短暂显示 0/0。
const batchExpectedTotal = ref(0)
// 暂停后可调整并发数量，恢复任务时传给后端生效。
const batchConcurrency = ref(2)
function setBatchConcurrency(value) {
  const next = Math.min(10, Math.max(1, Number(value) || 2))
  batchConcurrency.value = next
  if (activeConversationId.value) {
    try {
      localStorage.setItem(
        `batch_concurrency_${activeConversationId.value}`,
        String(next)
      )
    } catch (_) {}
  }
}
const batchResultScroll = ref(null)
const batchRunning = ref(false)
const batchPaused = ref(false)
const batchQueueSelected = ref([])
const batchQueueStopping = ref(false)
const currentConversation = computed(() =>
  conversations.value.find((item) => item.id === activeConversationId.value)
)
const batchSettings = computed(() => ({
  accountId: form.accountId,
  generationMode: form.generationMode,
  model: form.model,
  size: `${width.value}x${height.value}`,
  quality: form.quality,
  outputFormat: form.outputFormat,
  background: form.background,
  responseFormat: form.responseFormat,
  ratio: ratio.value,
  tier: tier.value
}))
function isBatchConversationRecord(conversation) {
  return Boolean(
    conversation?.batch || conversation?.title?.startsWith("批量生成 · ")
  )
}
const currentConversationIsBatch = computed(() =>
  isBatchConversationRecord(currentConversation.value)
)
const batchMode = computed(() => currentConversationIsBatch.value)
const batchViewVisible = computed(
  () =>
    currentConversationIsBatch.value ||
    (batchRunning.value &&
      activeConversationId.value === batchConversationId.value)
)
const batchQueuePage = ref(1)
const batchQueuePageSize = ref(20)
const batchQueueStatusFilter = ref("all")

// 右侧生成结果搜索、过滤、视图与分页
const batchResultSearch = ref("")
const batchResultStatus = ref("completed")
const batchResultViewMode = ref("grid")
const batchResultPage = ref(1)
const batchResultPageSize = ref(20)

const filteredResultTasks = computed(() => {
  // 重试任务会保留旧记录以便追踪，但结果列表只展示当前有效记录。
  let list = tasks.value.filter(
    (task) =>
      !roundState.value[task.request.roundId || task.id]?.replaced?.includes(
        task.id
      )
  )
  if (batchResultStatus.value === "completed") {
    list = list.filter(
      (task) =>
        ["completed", "partial"].includes(task.status) && task.imageCount > 0
    )
  } else if (batchResultStatus.value) {
    list = list.filter((task) => task.status === batchResultStatus.value)
  }
  if (batchResultSearch.value.trim()) {
    const q = batchResultSearch.value.trim().toLowerCase()
    list = list.filter(
      (task) =>
        (task.request.batchName &&
          task.request.batchName.toLowerCase().includes(q)) ||
        (task.request.prompt && task.request.prompt.toLowerCase().includes(q))
    )
  }
  return list
})

const batchResultTotalPages = computed(() =>
  Math.max(
    1,
    Math.ceil(filteredResultTasks.value.length / batchResultPageSize.value)
  )
)

const paginatedResultTasks = computed(() => {
  const start = (batchResultPage.value - 1) * batchResultPageSize.value
  return filteredResultTasks.value.slice(
    start,
    start + batchResultPageSize.value
  )
})

watch([batchResultSearch, batchResultStatus], () => {
  batchResultPage.value = 1
  nextTick(() => batchResultScroll.value?.scrollTo({ top: 0 }))
})
watch([batchResultPage, batchResultPageSize], () => {
  nextTick(() => batchResultScroll.value?.scrollTo({ top: 0 }))
})

function batchStatusTone(status) {
  if (["completed", "partial"].includes(status)) return "completed"
  if (status === "processing") return "processing"
  if (status === "queued") return "queued"
  if (["failed", "interrupted"].includes(status)) return "failed"
  return "queued"
}

function getVisiblePages(current, total) {
  if (total <= 7) {
    return Array.from({ length: total }, (_, i) => i + 1)
  }
  const pages = []
  pages.push(1)
  if (current > 3) {
    pages.push("...")
  }
  const start = Math.max(2, current - 1)
  const end = Math.min(total - 1, current + 1)
  for (let i = start; i <= end; i++) {
    pages.push(i)
  }
  if (current < total - 2) {
    pages.push("...")
  }
  pages.push(total)
  return pages
}

async function downloadTaskImage(task) {
  if (!task.imageCount) return
  try {
    const filename = `${task.request.batchName || "image"}-${task.id.slice(0, 8)}.png`
    const targetPath = await systemApi.saveFile({
      title: "下载图片",
      defaultPath: filename,
      filters: [{ name: "PNG 图片", extensions: ["png"] }]
    })
    if (targetPath) {
      await toolboxApi.exportImageTasks({
        ids: [task.id],
        index: 0,
        targetPath
      })
      createMessage.success("图片已下载")
    }
  } catch (error) {
    createMessage.error(String(error))
  }
}

async function copyPromptText(text) {
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    createMessage.success("提示词已复制到剪贴板")
  } catch {
    createMessage.error("复制失败")
  }
}
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

function openBatchDialog() {
  // 打开弹框只准备批量配置，点击开始后才创建独立会话。
  batchConversationId.value = ""
  batchExpectedTotal.value = 0
  batchConcurrency.value = 2
  batchPaused.value = false
  batchDialogOpen.value = true
}

function createBatchConversation(id, total = 0, concurrency = 2) {
  const item = {
    id,
    title: `批量生成 · ${formatDateTime(Date.now())}`,
    updatedAt: Date.now(),
    renamed: true,
    batch: true
  }
  conversations.value.unshift(item)
  batchConversationId.value = id
  batchExpectedTotal.value = Number(total) || 0
  batchConcurrency.value = Math.min(10, Math.max(1, Number(concurrency) || 2))
  tasks.value = []
  total.value = 0
  batchRunning.value = true
  batchPaused.value = false
  activeConversationId.value = id
  status.value = ""
  page.value = 1
  saveHistory()
  loadTasks()
}

function closeBatchDialog() {
  batchDialogOpen.value = false
  if (batchConversationId.value) loadTasks()
}

async function handleBatchTasksCreated() {
  // 任务记录落库后立即刷新，避免批量面板先显示 0/0。
  await loadTasks()
}

async function handleBatchFinished({ total: batchTotal, failed }) {
  // 批量完成后同步会话索引和当前列表，保留可恢复的失败任务。
  if (currentConversation.value?.id === batchConversationId.value) {
    currentConversation.value.updatedAt = Date.now()
    currentConversation.value.title = `批量生成 · ${batchTotal - failed}/${batchTotal} 完成`
  }
  saveHistory()
  await loadHistory()
  await loadTasks()
  batchRunning.value = failed > 0
  if (!batchRunning.value) batchPaused.value = false
  batchQueueSelected.value = []
}

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
        batch: Boolean(item.batchName),
        title:
          id === "legacy"
            ? "早期任务"
            : (item.prompt || "生图对话").slice(0, 28),
        updatedAt: item.createdAt
      }
      conversations.value.push(conversation)
    }
    if (item.batchName) conversation.batch = true
    conversation.updatedAt = Math.max(conversation.updatedAt, item.createdAt)
  }
  conversations.value.sort((a, b) => b.updatedAt - a.updatedAt)
  saveHistory()
}

function selectConversation(id) {
  activeConversationId.value = id
  batchExpectedTotal.value = 0
  tasks.value = []
  total.value = 0
  batchPaused.value = false
  historyOpen.value = false
  selected.value = []
  status.value = ""
  page.value = 1
  saveHistory()
  loadingTasks.value = true
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

function clearAllReferences() {
  references.value = []
  mask.value = null
  form.mode = "generate"
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
    await toolboxApi.resumeImageTask({
      id: task.id,
      ...(batchMode.value ? { concurrency: batchConcurrency.value } : {})
    })
    await loadTasks()
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    resuming.value = resuming.value.filter((id) => id !== task.id)
  }
}

async function resumeRound(round) {
  const pending = round.tasks.filter((task) => canResumeImageTask(task))
  for (const task of pending) await resumeTask(task)
}

async function resumeSelectedBatchTasks(ids = batchQueueSelected.value) {
  const targets = [...ids].filter((id) =>
    tasks.value.some((task) => task.id === id && canResumeImageTask(task))
  )
  if (!targets.length || batchQueueStopping.value) return
  batchQueueStopping.value = true
  try {
    for (const id of targets) {
      await toolboxApi.resumeImageTask({
        id,
        concurrency: batchConcurrency.value
      })
    }
    if (batchPaused.value) {
      batchPaused.value = false
    }
    batchQueueSelected.value = batchQueueSelected.value.filter(
      (id) => !targets.includes(id)
    )
    await loadTasks()
    createMessage.success(`已恢复 / 重试 ${targets.length} 项任务`)
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    batchQueueStopping.value = false
  }
}

async function cancelSelectedBatchTasks(ids = batchQueueSelected.value) {
  const targets = [...ids].filter((id) =>
    tasks.value.some(
      (task) => task.id === id && ["queued", "processing"].includes(task.status)
    )
  )
  if (!targets.length || batchQueueStopping.value) return
  batchQueueStopping.value = true
  try {
    // 用户取消的任务保留记录，但不再进入恢复队列。
    await toolboxApi.cancelImageTasks({ ids: targets, recoverable: false })
    batchQueueSelected.value = batchQueueSelected.value.filter(
      (id) => !targets.includes(id)
    )
    await loadTasks()
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    batchQueueStopping.value = false
  }
}

async function toggleBatchPause() {
  if (batchQueueStopping.value) return
  const activeTasks = tasks.value.filter((task) =>
    ["queued", "processing"].includes(task.status)
  )
  const interruptedTasks = tasks.value.filter(
    (task) => canResumeImageTask(task) && task.status === "interrupted"
  )
  const targets = batchPaused.value
    ? interruptedTasks.map((task) => task.id)
    : activeTasks.map((task) => task.id)
  if (!targets.length) return
  batchQueueStopping.value = true
  try {
    if (batchPaused.value) {
      for (const id of targets)
        await toolboxApi.resumeImageTask({
          id,
          concurrency: batchConcurrency.value
        })
      batchPaused.value = false
    } else {
      // 暂停使用可恢复中断，避免排队任务被取消后丢失。
      await toolboxApi.cancelImageTasks({ ids: targets, recoverable: true })
      batchPaused.value = true
    }
    await loadTasks()
  } catch (error) {
    createMessage.error(String(error))
  } finally {
    batchQueueStopping.value = false
  }
}

async function deleteBatchQueueTasks(ids = batchQueueSelected.value) {
  const targets = [...ids]
  if (!targets.length) return
  try {
    await toolboxApi.deleteImageTasks({ ids: targets })
    batchQueueSelected.value = []
    await loadTasks()
  } catch (error) {
    createMessage.error(String(error))
  }
}

const accounts = ref([])
const fallbackModels = [
  "gpt-image-2.5-sunburst",
  "gpt-image-2.5-flare",
  "gpt-image-2",
  "gpt-5.6-sol",
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

const quotaProgressPercent = computed(() => {
  if (!webQuota.value || typeof webQuota.value.remaining !== "number") return 0
  const max = webQuota.value.total || Math.max(150, webQuota.value.remaining)
  return Math.min(
    100,
    Math.max(0, Math.round((webQuota.value.remaining / max) * 100))
  )
})

const defaultQuickModels = [
  { id: "gpt-image-2" },
  { id: "gpt-image-1.5" },
  { id: "gpt-image-2.5-flare" },
  { id: "gpt-image-2.5-sunburst" },
  { id: "gpt-image-2.5-flare-2026-09-08" },
  { id: "gpt-image-2.5-sunburst-2026-09-08" },
  { id: "gpt-5-5" },
  { id: "gpt-5-6" },
  { id: "gpt-5-3-mini" },
  { id: "gpt-5-5-mini" },
  { id: "gpt-5-6-mini" }
]

const displayModels = computed(() => {
  if (imageModels.value && imageModels.value.length > 0)
    return imageModels.value
  return defaultQuickModels
})

function handleRoundMenuCommand(command, round) {
  if (command === "copy") {
    copyPrompt(round.tasks[0]?.request?.prompt, round.id)
  } else if (command === "reuse") {
    reuseTask(round.tasks[0], true)
  } else if (command === "regenerate") {
    regenerateRound(round)
  } else if (command === "resume") {
    resumeRound(round)
  } else if (command === "deletePrompt") {
    removeRoundPrompt(round)
  } else if (command === "delete") {
    removeRoundResults(round)
  }
}

function roundCanResume(round) {
  return round.tasks.some((task) => canResumeImageTask(task))
}
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

const expandedPrompts = ref(new Set())
function togglePrompt(roundId) {
  const next = new Set(expandedPrompts.value)
  if (next.has(roundId)) {
    next.delete(roundId)
  } else {
    next.add(roundId)
  }
  expandedPrompts.value = next
}
function isPromptExpanded(roundId) {
  return expandedPrompts.value.has(roundId)
}
function isLongPrompt(text) {
  return Boolean(text && (text.length > 90 || text.includes("\n")))
}

const copiedRoundId = ref(null)
let copiedTimer = null
async function copyPrompt(text, roundId) {
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    copiedRoundId.value = roundId
    if (copiedTimer) clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => {
      if (copiedRoundId.value === roundId) copiedRoundId.value = null
    }, 1500)
    createMessage.success("提示词已复制到剪切板")
  } catch (err) {
    createMessage.error("复制失败: " + (err?.message || String(err)))
  }
}
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
const loadingTasks = ref(true)
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
const hasSettingsError = computed(
  () =>
    !Number.isInteger(width.value) ||
    width.value < 1 ||
    !Number.isInteger(height.value) ||
    height.value < 1 ||
    width.value * height.value > 40_000_000 ||
    !Number.isInteger(form.n) ||
    form.n < 1 ||
    form.n > 100 ||
    (form.background === "transparent" && form.outputFormat === "jpeg")
)
function resetToDefaultSettings() {
  form.quality = "auto"
  width.value = 1024
  height.value = 1024
  ratio.value = "1:1"
  tier.value = "1k"
  form.n = 1
  form.outputFormat = "png"
  form.background = ""
  form.responseFormat = "url"
  createMessage.success("已恢复默认图像设置")
}

const canSubmit = computed(() =>
  Boolean(
    !submitting.value &&
    !uploading.value &&
    availableAccounts.value.some((account) => account.id === form.accountId) &&
    form.prompt.trim() &&
    form.model.trim() &&
    !hasSettingsError.value &&
    (form.mode !== "edit" || references.value.length)
  )
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

async function refreshQuota() {
  if (loadingQuota.value || submitting.value || !form.accountId) return
  webQuota.value = null
  await loadQuota(true)
}

async function loadQuota(forceRefresh = false) {
  const version = ++quotaRequestVersion
  const accountId = form.accountId
  const enabled = form.generationMode === "web" && Boolean(accountId)
  if (!enabled || webQuota.value?.accountId !== accountId) webQuota.value = null
  quotaError.value = ""
  loadingQuota.value = enabled
  if (!enabled) return

  try {
    const payload = { accountId }
    if (forceRefresh) payload.refreshToken = Date.now()
    const result = await toolboxApi.imageQuota(payload)
    // 账号切换或退出 Web 后，旧请求不能覆盖当前额度。
    if (version === quotaRequestVersion && !disposed) webQuota.value = result
  } catch (error) {
    if (version === quotaRequestVersion && !disposed)
      quotaError.value = String(error)
  } finally {
    if (version === quotaRequestVersion && !disposed) loadingQuota.value = false
  }
}

function consumeQuotaFromImageEvent(event) {
  const imageCount = Number(event?.imageCount || 0)
  if (
    form.generationMode !== "web" ||
    !form.accountId ||
    !Number.isFinite(imageCount) ||
    imageCount <= 0 ||
    (event?.accountId && event.accountId !== form.accountId) ||
    (event?.generationMode && event.generationMode !== "web") ||
    typeof webQuota.value?.remaining !== "number" ||
    loadingQuota.value
  )
    return
  webQuota.value = {
    ...webQuota.value,
    remaining: Math.max(0, webQuota.value.remaining - imageCount),
    updatedAt: Date.now()
  }
}

async function loadTasks() {
  // 状态筛选和轮询可能重叠，只应用最后一次请求，避免旧列表覆盖新筛选。
  const version = ++requestVersion
  refreshing.value = true
  try {
    const isBatchMode = batchMode.value
    const query = {
      page: isBatchMode ? 1 : page.value,
      status: isBatchMode ? "" : status.value,
      conversationId: activeConversationId.value,
      pageSize: isBatchMode ? 100 : pageSize.value,
      groupByRound: !isBatchMode
    }
    let result = await toolboxApi.listImageTasks(query)
    if (isBatchMode && result.total > result.items.length) {
      // 批量面板显示会话内的每一条任务，不能只取后端单页的 100 条。
      const pageCount = Math.ceil(result.total / query.pageSize)
      const pages = await Promise.all(
        Array.from({ length: pageCount - 1 }, (_, index) =>
          toolboxApi.listImageTasks({ ...query, page: index + 2 })
        )
      )
      result = {
        ...result,
        items: [result.items, ...pages.map((page) => page.items)].flat()
      }
    }
    if (version !== requestVersion || disposed) return
    tasks.value = result.items
    if (
      isBatchMode &&
      (!batchRunning.value ||
        activeConversationId.value !== batchConversationId.value)
    ) {
      const storedConcurrency = result.items.find((task) =>
        Number.isInteger(task.batchConcurrency)
      )?.batchConcurrency
      const localCustom = activeConversationId.value
        ? localStorage.getItem(
            `batch_concurrency_${activeConversationId.value}`
          )
        : null
      if (localCustom) {
        batchConcurrency.value = Math.min(
          10,
          Math.max(1, Number(localCustom) || 2)
        )
      } else if (storedConcurrency) {
        batchConcurrency.value = Math.min(10, Math.max(1, storedConcurrency))
      }
    }
    await loadHistory()
    if (version !== requestVersion || disposed) return
    total.value = result.total
    selected.value = selected.value.filter((id) =>
      selectableTasks.value.some((task) => task.id === id)
    )
    listError.value = ""
    const lastPage = Math.max(
      1,
      Math.ceil(total.value / (isBatchMode ? query.pageSize : pageSize.value))
    )
    if (page.value > lastPage) page.value = lastPage
  } catch (error) {
    if (version === requestVersion) listError.value = String(error)
  } finally {
    if (version === requestVersion) {
      refreshing.value = false
      loadingTasks.value = false
    }
  }
}

async function refreshAll() {
  loadingTasks.value = true
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

function formatQuality(quality) {
  const labels = {
    auto: "自动 (Auto)",
    low: "低 (Low)",
    medium: "中 (Medium)",
    high: "高 (High)",
    xhigh: "超高 (X-High)",
    max: "最高 (Max)"
  }
  return labels[quality] || quality || "默认"
}

function handleReuseFromModal() {
  if (!detail.value) return
  reuseTask(detail.value)
  detail.value = null
}

async function handleRegenerateFromModal() {
  if (
    !detail.value ||
    submitting.value ||
    ["queued", "processing"].includes(detail.value.status)
  )
    return
  const currentDetail = detail.value
  detail.value = null
  await regenerateTask(currentDetail)
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
watch(
  [() => form.accountId, () => form.generationMode, () => form.model],
  loadQuota
)
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
  if (autoRefresh.value || batchViewVisible.value) loadTasks()
  consumeQuotaFromImageEvent(event)
})
const unsubscribeError = subscribe("images:storage-error", (event) => {
  listError.value = `任务结果保存失败：${event.message}`
})
onMounted(() => {
  refreshAll()
  timer = window.setInterval(() => {
    if (
      (autoRefresh.value || batchViewVisible.value) &&
      !refreshing.value &&
      !document.hidden
    )
      loadTasks()
  }, 3000)
})
onBeforeUnmount(() => {
  disposed = true
  requestVersion++
  window.clearInterval(timer)
  if (copiedTimer) clearTimeout(copiedTimer)
  unsubscribe()
  unsubscribeError()
})

defineExpose({
  refreshAll,
  refreshing,
  loadingTasks
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

  .workbench-top-loader {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2.5px;
    z-index: 50;
    background: linear-gradient(
      90deg,
      transparent 0%,
      var(--color-primary, #2563eb) 50%,
      transparent 100%
    );
    background-size: 200% 100%;
    animation: top-loader-slide 1.5s ease-in-out infinite;
  }

  @keyframes top-loader-slide {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }

  /* 面板遮罩层 (Panel Loading Overlay) */
  .workbench-loading-overlay {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    width: 100%;
    height: 100%;
    z-index: 60;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 12px;
    background: color-mix(
      in srgb,
      var(--color-panel, #ffffff) 82%,
      transparent
    );
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    color: var(--color-primary, #2563eb);
    border-radius: inherit;
    pointer-events: all;
    user-select: none;

    .overlay-text {
      font-size: 13px;
      font-weight: 500;
      color: var(--color-text-muted);
      text-align: center;
    }
  }

  .workbench-fade-enter-active,
  .workbench-fade-leave-active {
    transition: opacity 0.2s ease;
  }

  .workbench-fade-enter-from,
  .workbench-fade-leave-to {
    opacity: 0;
  }

  .conversation-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 0 14px;
    flex: none;

    .conversation-toolbar-left,
    .conversation-toolbar-right {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .conversation-button {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      height: 32px;
      padding: 0 14px;
      border: 1px solid var(--color-line);
      border-radius: 9999px;
      background: var(--color-panel);
      color: var(--color-text);
      font-size: 13px;
      font-weight: 500;
      cursor: pointer;
      white-space: nowrap;
      flex-shrink: 0;
      transition: all 0.2s ease;

      &:hover:not(:disabled) {
        border-color: var(--color-primary);
        color: var(--color-primary);
        background: var(--color-primary-soft);
      }

      &.active {
        border-color: var(--color-primary);
        background: var(--color-primary-soft);
        color: var(--color-primary);
      }

      &:disabled {
        opacity: 0.45;
        cursor: not-allowed;
      }

      &.conversation-new-btn {
        background: #2563eb;
        border-color: #2563eb;
        color: #ffffff;

        &:hover {
          background: #1d4ed8;
          border-color: #1d4ed8;
          color: #ffffff;
        }
      }

      &.conversation-batch-btn {
        border-color: #0f766e;
        background: #f0fdfa;
        color: #0f766e;

        &:hover {
          border-color: #0d9488;
          background: #ccfbf1;
          color: #115e59;
        }
      }
    }

    .conversation-chip {
      display: inline-flex;
      align-items: center;
      height: 32px;
      padding: 0 16px;
      border-radius: 9999px;
      background: #eff6ff;
      border: 1px solid #bfdbfe;
      color: #2563eb;
      font-size: 13px;
      font-weight: 500;
      max-width: 360px;

      .conversation-chip-title {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }

    .batch-quota-compact {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      height: 32px;
      max-width: 240px;
      padding: 0 8px;
      border: 1px solid #bfdbfe;
      border-radius: 9999px;
      background: #eff6ff;
      color: #2563eb;
      font-size: 12px;
      white-space: nowrap;

      .batch-quota-label {
        color: #475569;
      }

      .batch-quota-value {
        color: #1d4ed8;
        font-size: 14px;
        font-variant-numeric: tabular-nums;
      }

      .batch-quota-unit {
        color: #475569;
      }

      .batch-quota-refresh {
        display: grid;
        width: 22px;
        height: 22px;
        place-items: center;
        padding: 0;
        border: 0;
        border-radius: 50%;
        background: transparent;
        color: inherit;
        cursor: pointer;

        &:hover:not(:disabled) {
          background: rgba(37, 99, 235, 0.12);
        }

        &:disabled {
          opacity: 0.45;
          cursor: not-allowed;
        }
      }
    }
  }

  .conversation-sidebar {
    position: absolute;
    inset: 46px auto 16px 0;
    z-index: 10;
    width: 280px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    border: 1px solid var(--color-line);
    border-radius: 12px;
    background: var(--color-panel);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.12);

    .history-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      font-size: 14px;
      font-weight: 600;
      color: var(--color-text);

      .history-action {
        border: 0;
        color: var(--color-primary);
        background: transparent;
        cursor: pointer;
        font-size: 12px;
      }
    }

    .history-list {
      flex: 1;
      min-height: 0;
      overflow-y: auto;

      .history-item {
        margin-bottom: 8px;
        padding: 10px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        transition: all 0.2s;

        &:hover {
          border-color: var(--color-line-strong);
          background: var(--color-panel-soft);
        }

        &.active {
          border-color: #93c5fd;
          background: #eff6ff;
        }

        .history-select {
          display: flex;
          flex-direction: column;
          width: 100%;
          gap: 6px;
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
            font-size: 13px;
            font-weight: 500;
          }

          .history-meta {
            color: var(--color-text-muted);
            font-size: 11.5px;
          }
        }

        .history-actions {
          display: flex;
          gap: 10px;
          margin-top: 8px;

          .history-action {
            border: 0;
            color: var(--color-primary);
            background: transparent;
            padding: 0;
            cursor: pointer;
            font-size: 12px;

            &:hover {
              text-decoration: underline;
            }
          }
        }
      }
    }

    .history-clear {
      border: 1px solid var(--color-line);
      background: var(--color-panel-soft);
      color: var(--color-danger);
      padding: 8px;
      border-radius: 8px;
      cursor: pointer;
      font-size: 12.5px;
      font-weight: 500;
      transition: all 0.2s;

      &:hover {
        background: rgba(239, 68, 68, 0.08);
        border-color: rgba(239, 68, 68, 0.3);
      }
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

    /* 左侧创建面板（4个步骤） */
    .create-panel {
      flex: 0 0 auto;
      display: flex;
      flex-direction: column;
      min-width: 0;
      min-height: 0;
      overflow-y: auto;
      gap: 14px;
      padding: 20px;
      border: 1px solid var(--color-line);
      border-radius: 12px;
      background: var(--color-panel);
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);

      > * {
        flex-shrink: 0;
      }

      .panel-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: 4px;

        .panel-head-left {
          display: flex;
          align-items: center;
          gap: 12px;

          .panel-head-badge {
            display: grid;
            width: 38px;
            height: 38px;
            flex: none;
            place-items: center;
            border-radius: 10px;
            background: #eff6ff;
            color: #2563eb;
          }

          .panel-head-info {
            display: flex;
            flex-direction: column;
            gap: 2px;

            .panel-head-title {
              font-size: 16px;
              font-weight: 700;
              color: var(--color-text);
              letter-spacing: 0.1px;
            }

            .panel-head-sub {
              font-size: 12.5px;
              color: var(--color-text-muted);
            }
          }
        }
      }

      /* 工作流步骤通用样式 */
      .workflow-step {
        display: flex;
        flex-direction: column;
        gap: 8px;

        .workflow-step-head {
          display: flex;
          align-items: center;
          justify-content: space-between;

          .workflow-step-left {
            display: flex;
            align-items: center;
            gap: 8px;

            .workflow-step-num {
              display: grid;
              width: 20px;
              height: 20px;
              place-items: center;
              border-radius: 50%;
              background: #2563eb;
              color: #ffffff;
              font-size: 11px;
              font-weight: 700;
            }

            .workflow-step-title {
              font-size: 14px;
              font-weight: 600;
              color: var(--color-text);

              .field-key {
                margin-left: 4px;
                font-size: 12px;
                font-weight: 400;
                color: var(--color-text-muted);
              }
            }
          }

          .step-action-link {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            border: 0;
            background: transparent;
            color: #2563eb;
            font-size: 12px;
            cursor: pointer;
            padding: 0;

            &:hover:not(:disabled) {
              text-decoration: underline;
            }

            &:disabled {
              opacity: 0.5;
              cursor: not-allowed;
            }
          }

          .step-settings-toggle {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            height: 26px;
            padding: 0 10px;
            border-radius: 9999px;
            border: 1px solid var(--color-line);
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
            font-size: 12px;
            cursor: pointer;
            transition: all 0.2s;

            &:hover {
              border-color: var(--color-line-strong);
              color: var(--color-text);
              background: var(--color-panel);
            }
          }
        }

        .workflow-step-body {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
      }

      /* 自定义 select 下拉组件 */
      .custom-select-wrapper {
        position: relative;
        width: 100%;

        .custom-select {
          width: 100%;
          height: 38px;
          padding: 0 34px 0 12px;
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel-soft);
          color: var(--color-text);
          font-size: 13.5px;
          appearance: none;
          outline: none;
          cursor: pointer;
          transition: border-color 0.2s;

          &:focus {
            border-color: #2563eb;
            background: var(--color-panel);
          }

          &:disabled {
            opacity: 0.6;
            cursor: not-allowed;
          }
        }

        .custom-select-arrow {
          position: absolute;
          right: 12px;
          top: 50%;
          transform: translateY(-50%);
          pointer-events: none;
          color: var(--color-text-muted);
        }
      }

      /* 额度遥测卡片 */
      .quota-telemetry-card {
        background: #f0f7ff;
        border: 1px solid #dbeafe;
        border-radius: 12px;
        padding: 14px 16px;

        .quota-telemetry-top {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 14px;

          .quota-telemetry-left {
            display: flex;
            align-items: center;
            gap: 12px;

            .quota-telemetry-icon-box {
              display: grid;
              width: 38px;
              height: 38px;
              flex: none;
              place-items: center;
              border-radius: 8px;
              background: #ffffff;
              border: 1px solid #bfdbfe;
              color: #2563eb;
            }

            .quota-telemetry-info {
              display: flex;
              flex-direction: column;
              gap: 6px;

              .quota-remaining-row {
                display: flex;
                align-items: baseline;
                gap: 4px;

                .quota-label {
                  font-size: 12px;
                  color: #475569;
                }

                .quota-value {
                  font-size: 20px;
                  font-weight: 700;
                  color: #1e293b;
                  line-height: 1;
                }

                .quota-unit {
                  font-size: 12px;
                  color: #475569;
                }
              }

              .quota-progress-track {
                width: 130px;
                height: 6px;
                border-radius: 9999px;
                background: #dbeafe;
                overflow: hidden;

                .quota-progress-bar {
                  height: 100%;
                  border-radius: 9999px;
                  background: #2563eb;
                  transition: width 0.3s ease;
                }
              }
            }
          }

          .quota-telemetry-meta {
            display: flex;
            flex-direction: column;
            align-items: flex-end;
            gap: 3px;
            font-size: 11.5px;
            color: #64748b;
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          }
        }

        .quota-error-text {
          margin: 8px 0 0;
          font-size: 12px;
          color: var(--color-danger);
        }
      }

      /* 模型软胶囊选择器 */
      .model-picker-list {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        margin-top: 4px;

        .model-picker-item {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          height: 30px;
          padding: 0 12px;
          border-radius: 9999px;
          border: 1px solid var(--color-line);
          background: var(--color-panel-soft);
          color: var(--color-text-muted);
          font-size: 12.5px;
          cursor: pointer;
          transition: all 0.2s;

          .model-picker-dot {
            width: 6px;
            height: 6px;
            border-radius: 50%;
            background: #94a3b8;
            transition: all 0.2s;
          }

          .model-picker-name {
            max-width: 240px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }

          &:hover {
            border-color: #93c5fd;
            color: var(--color-text);
          }

          &.active {
            background: #eff6ff;
            border-color: #93c5fd;
            color: #2563eb;
            font-weight: 500;

            .model-picker-dot {
              background: #2563eb;
              box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.2);
            }
          }
        }
      }

      /* 提示词输入区 */
      .prompt-box-container {
        position: relative;
        width: 100%;

        .prompt-textarea {
          width: 100%;
          min-height: 120px;
          padding: 12px 14px 28px;
          border: 1px solid var(--color-line);
          border-radius: 10px;
          background: var(--color-panel-soft);
          color: var(--color-text);
          font-size: 13.5px;
          line-height: 1.6;
          resize: vertical;
          outline: none;
          box-sizing: border-box;
          transition: border-color 0.2s;

          &:focus {
            border-color: #2563eb;
            background: var(--color-panel);
          }
        }

        .prompt-counter {
          position: absolute;
          right: 12px;
          bottom: 8px;
          font-size: 11.5px;
          color: var(--color-text-muted);
          pointer-events: none;
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        }
      }

      /* 步骤 5: 参考图与蒙版 */
      .workflow-step--references {
        .workflow-step-head {
          .step-mode-pill {
            display: inline-flex;
            align-items: center;
            padding: 2px 8px;
            border-radius: 9999px;
            font-size: 11px;
            font-weight: 500;
            line-height: 1.4;

            &.edit {
              background: #eff6ff;
              border: 1px solid #bfdbfe;
              color: #2563eb;
              font-weight: 600;
            }

            &.text {
              background: var(--color-panel-soft);
              border: 1px solid var(--color-line);
              color: var(--color-text-muted);
            }
          }

          .step-action-link.danger-link {
            color: #ef4444;

            &:hover:not(:disabled) {
              color: #dc2626;
            }
          }
        }

        .reference-gallery-list {
          display: flex;
          flex-wrap: wrap;
          gap: 10px;
          align-items: center;
          margin-bottom: 6px;

          .reference-thumb-card {
            position: relative;
            width: 76px;
            height: 76px;
            border-radius: 8px;
            overflow: hidden;
            border: 1px solid var(--color-line);
            background: var(--color-panel-soft);
            box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
            flex-shrink: 0;

            .reference-thumb-img {
              width: 100%;
              height: 100%;
              display: block;
            }

            .reference-index-tag {
              position: absolute;
              bottom: 3px;
              left: 4px;
              font-size: 10px;
              font-weight: 600;
              padding: 1px 5px;
              border-radius: 3px;
              background: rgba(0, 0, 0, 0.65);
              backdrop-filter: blur(4px);
              color: #ffffff;
              line-height: 1.2;
              font-family:
                ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            }

            .remove-reference-btn {
              position: absolute;
              top: 3px;
              right: 3px;
              width: 20px;
              height: 20px;
              border-radius: 50%;
              background: rgba(0, 0, 0, 0.65);
              backdrop-filter: blur(4px);
              color: #ffffff;
              border: 0;
              display: grid;
              place-items: center;
              cursor: pointer;
              transition: all 0.2s;

              &:hover {
                background: #ef4444;
                transform: scale(1.1);
              }
            }

            .reference-thumb-name {
              display: none;
            }
          }

          .reference-upload-card {
            position: relative;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            width: 76px;
            height: 76px;
            border: 1.5px dashed #93c5fd;
            border-radius: 8px;
            background: #f8faff;
            cursor: pointer;
            transition: all 0.2s;
            box-sizing: border-box;
            text-align: center;
            flex-shrink: 0;

            &:hover {
              border-color: #2563eb;
              background: #eff6ff;
            }

            &.is-disabled {
              opacity: 0.5;
              cursor: not-allowed;
              border-color: var(--color-line);
              background: var(--color-panel-soft);
            }

            .upload-card-content {
              display: flex;
              flex-direction: column;
              align-items: center;
              justify-content: center;
              gap: 2px;

              .upload-icon {
                color: #2563eb;
                margin-bottom: 1px;
              }

              .upload-text {
                font-size: 11px;
                font-weight: 600;
                color: #2563eb;
                line-height: 1.2;
              }

              .upload-sub {
                font-size: 10px;
                color: #64748b;
                line-height: 1.2;
              }
            }

            .upload-file-input {
              position: absolute;
              inset: 0;
              opacity: 0;
              cursor: pointer;
              width: 100%;
              height: 100%;
            }
          }
        }

        .reference-hint-line {
          font-size: 11.5px;
          color: var(--color-text-muted);
          line-height: 1.5;
          margin-bottom: 8px;
        }

        .mask-manager-card {
          border: 1px solid var(--color-line);
          border-radius: 8px;
          background: var(--color-panel-soft);
          padding: 10px 12px;

          .mask-active-row {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 12px;

            .mask-badge-content {
              display: flex;
              align-items: center;
              gap: 8px;
              min-width: 0;

              .mask-pill-tag {
                background: #f5f3ff;
                color: #7c3aed;
                border: 1px solid #ddd6fe;
                font-size: 11px;
                font-weight: 600;
                padding: 2px 8px;
                border-radius: 4px;
                flex-shrink: 0;
              }

              .mask-name-text {
                font-size: 12px;
                font-weight: 500;
                color: var(--color-text);
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
                max-width: 140px;
              }

              .mask-hint-text {
                font-size: 11.5px;
                color: var(--color-text-muted);
              }
            }

            .mask-actions {
              display: flex;
              align-items: center;
              gap: 6px;

              .mask-action-btn {
                position: relative;
                display: inline-flex;
                align-items: center;
                gap: 4px;
                height: 26px;
                padding: 0 9px;
                border-radius: 5px;
                border: 1px solid var(--color-line);
                background: var(--color-panel);
                color: var(--color-text);
                font-size: 11.5px;
                cursor: pointer;
                transition: all 0.2s;

                &:hover {
                  border-color: var(--color-line-strong);
                  background: var(--color-panel-soft);
                }

                &.danger {
                  color: #ef4444;

                  &:hover {
                    border-color: #fca5a5;
                    background: #fef2f2;
                  }
                }

                .upload-file-input {
                  position: absolute;
                  inset: 0;
                  opacity: 0;
                  cursor: pointer;
                  width: 100%;
                  height: 100%;
                }
              }
            }
          }

          .mask-empty-row {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 12px;
            flex-wrap: wrap;

            .mask-prompt-left {
              display: flex;
              flex-direction: column;
              gap: 2px;

              .mask-label {
                font-size: 12.5px;
                font-weight: 600;
                color: var(--color-text);
              }

              .mask-desc {
                font-size: 11.5px;
                color: var(--color-text-muted);
              }
            }

            .mask-prompt-right {
              display: flex;
              align-items: center;
              gap: 8px;

              .mask-upload-btn {
                position: relative;
                display: inline-flex;
                align-items: center;
                gap: 5px;
                height: 28px;
                padding: 0 11px;
                border-radius: 6px;
                border: 1px dashed #93c5fd;
                background: #eff6ff;
                color: #2563eb;
                font-size: 12px;
                font-weight: 500;
                cursor: pointer;
                transition: all 0.2s;

                &:hover {
                  border-color: #2563eb;
                  background: #dbeafe;
                }

                &.is-disabled {
                  opacity: 0.5;
                  cursor: not-allowed;
                  border-color: var(--color-line);
                  background: var(--color-panel-soft);
                  color: var(--color-text-muted);
                }

                .upload-file-input {
                  position: absolute;
                  inset: 0;
                  opacity: 0;
                  cursor: pointer;
                  width: 100%;
                  height: 100%;
                }
              }

              .mask-sketch-btn {
                display: inline-flex;
                align-items: center;
                gap: 5px;
                height: 28px;
                padding: 0 11px;
                border-radius: 6px;
                border: 1px solid var(--color-line);
                background: var(--color-panel);
                color: var(--color-text);
                font-size: 12px;
                font-weight: 500;
                cursor: pointer;
                transition: all 0.2s;

                &:hover:not(:disabled) {
                  border-color: var(--color-line-strong);
                  background: var(--color-panel-soft);
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

      .prompt-actions-bar {
        display: flex;
        align-items: center;
        justify-content: flex-start;
        gap: 8px;
        margin-top: 4px;

        .prompt-actions-left {
          display: flex;
          align-items: center;
          gap: 8px;

          .prompt-action-btn {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            height: 32px;
            padding: 0 12px;
            border-radius: 7px;
            border: 1px solid var(--color-line);
            background: var(--color-panel);
            color: var(--color-text-muted);
            font-size: 12.5px;
            cursor: pointer;
            transition: all 0.2s;

            &:hover:not(:disabled) {
              border-color: var(--color-line-strong);
              color: var(--color-text);
              background: var(--color-panel-soft);
            }

            &:disabled {
              opacity: 0.45;
              cursor: not-allowed;
            }
          }
        }
      }

      /* 表单底部提交栏 */
      .workflow-submit-footer {
        display: flex;
        flex-direction: column;
        gap: 8px;
        margin-top: auto;
        padding-top: 14px;
        border-top: 1px solid var(--color-line);

        .workflow-submit-btn {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          gap: 8px;
          width: 100%;
          height: 42px;
          border-radius: 9px;
          border: 0;
          background: #2563eb;
          color: #ffffff;
          font-size: 14px;
          font-weight: 600;
          cursor: pointer;
          box-shadow: 0 2px 8px rgba(37, 99, 235, 0.2);
          transition: all 0.2s ease;

          &:hover:not(:disabled) {
            background: #1d4ed8;
            box-shadow: 0 4px 12px rgba(37, 99, 235, 0.3);
            transform: translateY(-1px);
          }

          &:active:not(:disabled) {
            transform: translateY(0);
          }

          &:disabled {
            opacity: 0.5;
            cursor: not-allowed;
            box-shadow: none;
          }
        }

        .submit-footer-error {
          margin: 0;
          font-size: 12px;
          color: var(--color-danger);
          text-align: center;
        }
      }

      .account-hint,
      .field-error {
        margin: 4px 0 0;
        font-size: 12px;
        line-height: 1.5;
      }

      .account-hint {
        color: var(--color-warning);
      }

      .field-error {
        color: var(--color-danger);
      }
    }

    /* =========================================================================
       批量生成双列工作台样式 (Asymmetric Batch Workbench Layout)
       ========================================================================= */
    .batch-results-panel {
      position: relative;
      display: flex;
      min-width: 0;
      min-height: 0;
      flex-direction: column;
      gap: 12px;
      overflow: hidden;
      padding: 16px;
      border: 1px solid var(--color-line);
      border-radius: 12px;
      background: var(--color-panel);
      box-shadow: 0 1px 3px rgba(15, 23, 42, 0.03);
    }

    /* -------------------------------------------------------------------------
       右侧生成结果面板 (Batch Results Panel)
       ------------------------------------------------------------------------- */
    .batch-results-panel {
      flex: 1 1 auto;
    }

    .batch-results-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      flex-wrap: wrap;
      flex-shrink: 0;
      padding-bottom: 12px;
      border-bottom: 1px solid var(--color-line);

      .batch-results-header-left {
        display: flex;
        align-items: baseline;
        gap: 8px;

        .batch-results-title {
          margin: 0;
          font-size: 15px;
          font-weight: 700;
          color: var(--color-text);
          letter-spacing: -0.01em;
        }

        .batch-results-counter {
          font-size: 12.5px;
          color: var(--color-text-muted);
        }
      }

      .batch-results-toolbar {
        display: flex;
        align-items: center;
        gap: 8px;
      }
    }

    /* 搜索框 */
    .batch-search-box {
      position: relative;
      display: flex;
      align-items: center;
      width: 180px;
      height: 28px;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel);
      padding: 0 6px 0 26px;
      transition: border-color 0.16s ease;

      &:focus-within {
        border-color: #2563eb;
        box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.12);
      }

      .search-icon {
        position: absolute;
        left: 8px;
        color: var(--color-text-soft);
        pointer-events: none;
      }

      .batch-search-input {
        width: 100%;
        border: 0;
        outline: none;
        background: transparent;
        font-size: 12px;
        color: var(--color-text);
        padding: 0;

        &::placeholder {
          color: var(--color-text-soft);
        }
      }

      .search-clear-btn {
        display: grid;
        width: 16px;
        height: 16px;
        place-items: center;
        border: 0;
        background: transparent;
        color: var(--color-text-muted);
        cursor: pointer;
        padding: 0;
        border-radius: 50%;

        &:hover {
          color: var(--color-text);
          background: var(--color-panel-soft);
        }
      }
    }

    /* 视图切换 (Grid / List) */
    .batch-view-toggle {
      display: inline-flex;
      align-items: center;
      border: 1px solid var(--color-line);
      border-radius: 6px;
      background: var(--color-panel-soft);
      padding: 1px;

      .view-toggle-btn {
        display: grid;
        width: 26px;
        height: 24px;
        place-items: center;
        border: 0;
        border-radius: 4px;
        background: transparent;
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.14s ease;

        &:hover {
          color: var(--color-text);
        }

        &.is-active {
          background: var(--color-panel);
          color: #2563eb;
          box-shadow: 0 1px 2px rgba(0, 0, 0, 0.06);
        }
      }
    }

    .batch-results-body {
      flex: 1;
      min-height: 0;
      overflow-y: auto;
      scrollbar-gutter: stable;
    }

    /* 3 列卡片网格 (3-Column Card Grid matching 3.png) */
    .batch-card-grid {
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 12px;
      padding-bottom: 8px;

      @media (max-width: 900px) {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }

      @media (max-width: 600px) {
        grid-template-columns: 1fr;
      }
    }

    .batch-result-card {
      display: flex;
      flex-direction: column;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      overflow: hidden;
      transition: all 0.16s ease;

      &:hover {
        border-color: var(--color-line-strong);
        box-shadow: 0 4px 12px rgba(15, 23, 42, 0.06);
      }

      .result-card-media {
        position: relative;
        aspect-ratio: 16 / 10;
        background: var(--color-panel-soft);
        overflow: hidden;
        cursor: pointer;

        .result-card-img {
          width: 100%;
          height: 100%;
          transition: transform 0.2s ease;

          &:hover {
            transform: scale(1.03);
          }
        }

        .result-card-loading,
        .result-card-empty {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          height: 100%;
          color: var(--color-text-soft);
          gap: 6px;
          font-size: 11px;
        }

        /* 浮动在图片右上角的状态徽章 */
        .result-card-badge {
          position: absolute;
          top: 6px;
          right: 6px;
          display: inline-flex;
          align-items: center;
          gap: 4px;
          height: 20px;
          padding: 0 7px;
          border-radius: 9999px;
          font-size: 10.5px;
          font-weight: 600;
          backdrop-filter: blur(8px);
          box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);

          .badge-dot {
            width: 5px;
            height: 5px;
            border-radius: 50%;
          }

          &.is-completed {
            background: rgba(240, 253, 244, 0.9);
            border: 1px solid rgba(220, 252, 231, 0.9);
            color: #16a34a;
            .badge-dot {
              background: #22c55e;
            }
          }

          &.is-processing {
            background: rgba(239, 246, 255, 0.9);
            border: 1px solid rgba(219, 234, 254, 0.9);
            color: #2563eb;
            .badge-dot {
              background: #3b82f6;
            }
          }

          &.is-queued {
            background: rgba(254, 249, 195, 0.9);
            border: 1px solid rgba(254, 240, 138, 0.9);
            color: #854d0e;
            .badge-dot {
              background: #eab308;
            }
          }

          &.is-failed {
            background: rgba(254, 242, 242, 0.9);
            border: 1px solid rgba(254, 226, 226, 0.9);
            color: #dc2626;
            .badge-dot {
              background: #ef4444;
            }
          }
        }
      }

      .result-card-content {
        display: flex;
        flex-direction: column;
        padding: 10px;
        gap: 4px;
        flex: 1;

        .result-card-title {
          margin: 0;
          font-size: 12.5px;
          font-weight: 600;
          color: var(--color-text);
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .result-card-prompt {
          margin: 0;
          font-size: 11px;
          line-height: 1.4;
          color: var(--color-text-muted);
          display: -webkit-box;
          -webkit-line-clamp: 2;
          line-clamp: 2;
          -webkit-box-orient: vertical;
          overflow: hidden;
          min-height: 30px;
        }

        .result-card-actions {
          display: flex;
          align-items: center;
          justify-content: flex-end;
          gap: 6px;
          margin-top: auto;
          padding-top: 8px;
          border-top: 1px solid var(--color-line);

          .card-act-btn {
            display: grid;
            width: 24px;
            height: 24px;
            place-items: center;
            border: 1px solid var(--color-line);
            border-radius: 5px;
            background: var(--color-panel);
            color: var(--color-text-muted);
            cursor: pointer;
            transition: all 0.14s ease;

            &:hover:not(:disabled) {
              border-color: #2563eb;
              color: #2563eb;
              background: var(--color-primary-soft);
            }

            &:disabled {
              opacity: 0.4;
              cursor: not-allowed;
            }
          }
        }
      }
    }

    /* 列表视图模式 */
    .batch-results-list {
      display: flex;
      flex-direction: column;
      gap: 6px;

      .batch-result-item {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 8px 12px;
        border: 1px solid var(--color-line);
        border-radius: 8px;
        background: var(--color-panel);
        transition: background-color 0.14s ease;

        &:hover {
          background: var(--color-panel-soft);
        }

        .batch-result-thumb {
          display: grid;
          width: 48px;
          height: 48px;
          flex: 0 0 48px;
          place-items: center;
          overflow: hidden;
          border-radius: 6px;
          background: var(--color-panel-soft);
          border: 1px solid var(--color-line);
          cursor: pointer;

          :deep(.el-image) {
            width: 100%;
            height: 100%;
          }
        }

        .batch-result-main {
          display: flex;
          flex-direction: column;
          flex: 1;
          min-width: 0;
          gap: 3px;

          .batch-result-name-row {
            display: flex;
            align-items: center;
            gap: 8px;

            .batch-result-name {
              font-size: 13px;
              font-weight: 600;
              color: var(--color-text);
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
            }
          }

          .batch-result-prompt {
            font-size: 11.5px;
            color: var(--color-text-muted);
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
          }
        }

        .batch-result-actions {
          display: flex;
          align-items: center;
          gap: 6px;
          flex-shrink: 0;
        }
      }
    }

    .batch-empty-results {
      display: flex;
      flex: 1;
      min-height: 100%;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 60px 20px;
      text-align: center;
      color: var(--color-text-muted);

      .empty-icon {
        color: var(--color-text-soft);
        margin-bottom: 12px;
      }

      h4 {
        margin: 0 0 4px;
        font-size: 14px;
        color: var(--color-text);
      }

      p {
        margin: 0;
        font-size: 12px;
        color: var(--color-text-muted);
      }
    }

    /* 结果底部翻页 */
    .batch-results-footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding-top: 10px;
      border-top: 1px solid var(--color-line);
      flex-shrink: 0;

      .footer-left {
        display: flex;
        align-items: center;
      }

      .footer-right {
        display: flex;
        align-items: center;
      }
    }

    /* 通用分页组件 (Batch Pagination) */
    .batch-pagination {
      display: inline-flex;
      align-items: center;
      gap: 4px;

      .page-nav-btn {
        display: grid;
        width: 26px;
        height: 26px;
        place-items: center;
        border: 1px solid var(--color-line);
        border-radius: 5px;
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.14s ease;

        &:hover:not(:disabled) {
          border-color: var(--color-line-strong);
          color: var(--color-text);
          background: var(--color-panel-soft);
        }

        &:disabled {
          opacity: 0.4;
          cursor: not-allowed;
        }
      }

      .page-num-btn {
        display: grid;
        min-width: 26px;
        height: 26px;
        padding: 0 6px;
        place-items: center;
        border: 1px solid var(--color-line);
        border-radius: 5px;
        background: var(--color-panel);
        color: var(--color-text);
        font-size: 11.5px;
        font-family: "Bahnschrift", monospace;
        cursor: pointer;
        transition: all 0.14s ease;

        &:hover {
          border-color: #2563eb;
          color: #2563eb;
        }

        &.is-active {
          border-color: #2563eb;
          background: #2563eb;
          color: #ffffff;
          font-weight: 700;
        }
      }

      .page-ellipsis {
        display: grid;
        width: 20px;
        height: 26px;
        place-items: center;
        color: var(--color-text-soft);
        font-size: 12px;
      }
    }

    /* 迷你下拉选择框 */
    .custom-select-wrapper.is-mini {
      position: relative;
      display: inline-flex;
      align-items: center;

      .mini-select {
        height: 26px;
        padding: 0 22px 0 8px;
        border: 1px solid var(--color-line);
        border-radius: 5px;
        background: var(--color-panel);
        color: var(--color-text);
        font-size: 11.5px;
        outline: none;
        cursor: pointer;
        appearance: none;

        &:focus {
          border-color: #2563eb;
        }
      }

      .custom-select-arrow {
        position: absolute;
        right: 6px;
        color: var(--color-text-muted);
        pointer-events: none;
      }
    }

    /* 右侧任务列表面板 */
    .tasks-panel {
      position: relative;
      flex: 1;
      display: flex;
      flex-direction: column;
      min-width: 0;
      min-height: 0;
      overflow: hidden;
      border: 1px solid var(--color-line);
      border-radius: 12px;
      background: var(--color-panel);

      .tasks-head {
        display: flex;
        flex-shrink: 0;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        flex-wrap: wrap;
        row-gap: 10px;
        padding: 12px 18px;
        border-bottom: 1px solid var(--color-line);
        background: var(--color-panel);

        .tasks-head-left {
          display: flex;
          align-items: center;
          gap: 12px;
          min-width: 0;
          flex: 1 1 180px;

          .tasks-head-icon-box {
            display: grid;
            width: 36px;
            height: 36px;
            flex: none;
            place-items: center;
            border-radius: 8px;
            background: #eff6ff;
            color: #2563eb;
          }

          .tasks-head-info {
            display: flex;
            flex-direction: column;
            gap: 2px;
            min-width: 0;

            .tasks-head-title-row {
              display: flex;
              align-items: center;
              gap: 8px;
              white-space: nowrap;

              .tasks-head-title {
                font-size: 16px;
                font-weight: 700;
                color: var(--color-text);
                white-space: nowrap;
              }

              .tasks-head-round-badge {
                padding: 1px 8px;
                border-radius: 9999px;
                background: #eff6ff;
                border: 1px solid #bfdbfe;
                color: #2563eb;
                font-size: 11px;
                font-weight: 600;
                white-space: nowrap;
                flex-shrink: 0;
              }
            }

            .tasks-head-sub {
              font-size: 12px;
              color: var(--color-text-muted);
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
            }
          }
        }

        .tasks-controls {
          display: flex;
          align-items: center;
          gap: 8px;
          flex-shrink: 0;

          .tasks-status-wrapper {
            width: 106px;
            flex-shrink: 0;

            .status-select {
              height: 32px;
              padding: 0 26px 0 10px;
              border: 1px solid var(--color-line);
              border-radius: 6px;
              background: var(--color-panel-soft);
              color: var(--color-text);
              font-size: 12.5px;
              white-space: nowrap;
            }
          }

          .auto-refresh-pill {
            display: inline-flex;
            align-items: center;
            gap: 6px;
            height: 32px;
            padding: 0 10px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
            font-size: 12.5px;
            cursor: pointer;
            user-select: none;
            transition: all 0.2s;
            white-space: nowrap;
            flex-shrink: 0;

            input {
              display: none;
            }

            .status-dot {
              width: 6px;
              height: 6px;
              border-radius: 50%;
              background: #94a3b8;
              transition: all 0.2s;
              flex-shrink: 0;
            }

            span {
              white-space: nowrap;
            }

            &.active {
              color: var(--color-text);
              border-color: rgba(16, 185, 129, 0.35);
              background: rgba(16, 185, 129, 0.08);

              .status-dot {
                background: var(--color-success, #10b981);
                box-shadow: 0 0 6px rgba(16, 185, 129, 0.5);
              }
            }
          }

          .tasks-icon-button {
            display: grid;
            width: 32px;
            height: 32px;
            flex-shrink: 0;
            place-items: center;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
            cursor: pointer;
            transition: all 0.2s;

            &:hover:not(:disabled) {
              border-color: var(--color-line-strong);
              color: var(--color-text);
            }

            &:disabled {
              opacity: 0.45;
              cursor: not-allowed;
            }
          }

          .tasks-select-toggle-btn {
            display: inline-flex;
            align-items: center;
            gap: 5px;
            height: 32px;
            padding: 0 10px;
            border: 1px solid var(--color-line);
            border-radius: 6px;
            background: var(--color-panel-soft);
            color: var(--color-text-muted);
            font-size: 12.5px;
            cursor: pointer;
            transition: all 0.2s;
            white-space: nowrap;
            flex-shrink: 0;

            span {
              white-space: nowrap;
            }

            &.active {
              border-color: var(--color-primary);
              color: var(--color-primary);
              background: var(--color-primary-soft);
            }
          }
        }
      }

      .selection-bar {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 10px 18px;
        background: var(--color-primary-soft);
        border-bottom: 1px solid var(--color-line);
        animation: selection-slide-down 0.2s ease;

        .selection-check-all {
          display: inline-flex;
          align-items: center;
          gap: 6px;
          cursor: pointer;
          font-size: 12.5px;
          font-weight: 500;
          color: var(--color-text);
        }

        .selection-count {
          font-size: 12.5px;
          color: var(--color-text-muted);
        }

        .selection-actions {
          display: flex;
          gap: 8px;
        }
      }

      .empty-state {
        display: flex;
        min-height: 280px;
        flex: 1;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 12px;
        padding: 24px;
        text-align: center;
        color: var(--color-text-muted);

        .empty-icon {
          display: grid;
          width: 56px;
          height: 56px;
          place-items: center;
          border-radius: 50%;
          background: var(--color-panel-soft);
          border: 1px solid var(--color-line);
          color: var(--color-text-soft);
        }

        .empty-title {
          font-size: 15px;
          font-weight: 600;
          color: var(--color-text);
        }

        .empty-description {
          font-size: 13px;
          color: var(--color-text-muted);
        }
      }

      .task-list {
        display: flex;
        flex-direction: column;
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: 16px;

        /* 单轮任务卡片容器 */
        .round-card {
          display: flex;
          flex-direction: column;
          gap: 12px;
          padding: 16px 18px;
          margin-bottom: 16px;
          border: 1px solid var(--color-line);
          border-radius: 12px;
          background: var(--color-panel);
          box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);

          .round-card-head {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 12px;

            .round-card-head-left {
              display: flex;
              align-items: center;
              gap: 8px;
              flex-wrap: wrap;

              .round-num-pill {
                padding: 2px 10px;
                border-radius: 9999px;
                background: #eff6ff;
                border: 1px solid #bfdbfe;
                color: #2563eb;
                font-weight: 700;
                font-size: 12px;
                font-family:
                  ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                  monospace;
              }

              .round-model-chip {
                display: inline-flex;
                align-items: center;
                border: 1px solid var(--color-line);
                border-radius: 6px;
                overflow: hidden;
                font-size: 11.5px;

                .round-chip-label {
                  padding: 2px 7px;
                  background: var(--color-panel-soft);
                  color: var(--color-text-muted);
                  border-right: 1px solid var(--color-line);
                }

                .round-chip-value {
                  padding: 2px 9px;
                  background: var(--color-panel);
                  color: var(--color-text);
                  font-family:
                    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                    monospace;
                  max-width: 220px;
                  overflow: hidden;
                  text-overflow: ellipsis;
                  white-space: nowrap;
                }
              }

              .round-tag-badge {
                padding: 2px 8px;
                border-radius: 5px;
                border: 1px solid var(--color-line);
                background: var(--color-panel-soft);
                color: var(--color-text-muted);
                font-size: 11.5px;
                font-family:
                  ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                  monospace;

                &.round-tag-mode {
                  font-weight: 600;
                  color: #2563eb;
                  background: #eff6ff;
                  border-color: #bfdbfe;
                }
              }
            }

            .round-card-head-right {
              display: flex;
              align-items: center;
              gap: 8px;

              .round-date-text {
                font-size: 12px;
                color: var(--color-text-muted);
                font-family:
                  ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                  monospace;
              }

              .round-menu-trigger-btn {
                display: grid;
                width: 28px;
                height: 28px;
                place-items: center;
                border: 1px solid var(--color-line);
                border-radius: 6px;
                background: var(--color-panel);
                color: var(--color-text-muted);
                cursor: pointer;
                transition: all 0.2s;

                &:hover {
                  border-color: var(--color-line-strong);
                  color: var(--color-text);
                }
              }
            }
          }

          /* 提示词引言框 */
          .round-prompt-box {
            background: #f8fafc;
            border: 1px solid #e2e8f0;
            border-left: 3px solid #2563eb;
            border-radius: 8px;
            padding: 12px 14px;

            .round-batch-name {
              display: block;
              margin-bottom: 5px;
              color: #2563eb;
              font-size: 12px;
            }

            .round-prompt-text {
              margin: 0;
              font-size: 13px;
              line-height: 1.6;
              color: var(--color-text);
              word-break: break-word;

              &.is-clamped {
                display: -webkit-box;
                -webkit-box-orient: vertical;
                -webkit-line-clamp: 2;
                line-clamp: 2;
                overflow: hidden;
              }
            }

            .prompt-expand-btn {
              display: inline-flex;
              align-items: center;
              gap: 4px;
              margin-top: 6px;
              padding: 0;
              border: 0;
              background: transparent;
              color: #2563eb;
              font-size: 11.5px;
              font-weight: 500;
              cursor: pointer;

              &:hover {
                text-decoration: underline;
              }
            }
          }

          /* 4 个快捷操作按钮 */
          .round-quick-actions {
            display: flex;
            align-items: center;
            gap: 8px;
            flex-wrap: wrap;

            .round-quick-btn {
              display: inline-flex;
              align-items: center;
              gap: 5px;
              height: 28px;
              padding: 0 10px;
              border-radius: 6px;
              border: 1px solid var(--color-line);
              background: var(--color-panel);
              color: var(--color-text-muted);
              font-size: 12px;
              cursor: pointer;
              white-space: nowrap;
              flex-shrink: 0;
              transition: all 0.2s;

              &:hover:not(:disabled) {
                border-color: var(--color-line-strong);
                color: var(--color-text);
                background: var(--color-panel-soft);
              }

              &--danger {
                color: #ef4444;

                &:hover:not(:disabled) {
                  border-color: #fca5a5;
                  background: #fef2f2;
                  color: #dc2626;
                }
              }

              &:disabled {
                opacity: 0.45;
                cursor: not-allowed;
              }
            }
          }

          /* 任务结果区域 */
          .round-tasks-grid {
            display: flex;
            flex-direction: column;
            gap: 12px;

            .image-task-card {
              border: 1px solid var(--color-line);
              border-radius: 10px;
              background: var(--color-panel);
              overflow: hidden;
              transition: all 0.2s;

              &:hover {
                border-color: var(--color-line-strong);
                box-shadow: 0 4px 14px rgba(0, 0, 0, 0.04);
              }

              .image-task-status-bar {
                display: flex;
                align-items: center;
                justify-content: space-between;
                padding: 8px 12px;
                background: var(--color-panel-soft);
                border-bottom: 1px solid var(--color-line);
                font-size: 12px;

                .image-task-status-left {
                  display: flex;
                  align-items: center;
                  gap: 10px;

                  .image-task-status-pill {
                    display: inline-flex;
                    align-items: center;
                    gap: 5px;
                    font-size: 11.5px;
                    font-weight: 600;

                    &.succeeded,
                    &.completed {
                      color: #10b981;
                    }

                    &.processing {
                      color: #2563eb;
                    }

                    &.failed,
                    &.interrupted {
                      color: #ef4444;
                    }
                  }

                  .image-task-time {
                    display: inline-flex;
                    align-items: center;
                    gap: 4px;
                    color: var(--color-text-muted);
                    font-size: 11.5px;
                    font-family:
                      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                      monospace;
                  }
                }

                .image-task-status-right {
                  .image-task-count {
                    font-size: 11.5px;
                    color: var(--color-text-muted);
                    font-weight: 500;
                  }
                }
              }

              /* 巨幕大图预览与悬浮操作层 */
              .image-hero-preview {
                position: relative;
                width: 100%;
                min-height: 260px;
                max-height: 480px;
                background: var(--color-panel-soft);
                display: flex;
                align-items: center;
                justify-content: center;
                overflow: hidden;

                .image-hero-img {
                  width: 100%;
                  height: 100%;
                  max-height: 480px;

                  :deep(img) {
                    object-fit: contain;
                    max-height: 480px;
                  }
                }

                .image-hero-overlay {
                  position: absolute;
                  inset: 0;
                  display: flex;
                  flex-direction: column;
                  align-items: center;
                  justify-content: center;
                  gap: 14px;
                  background: rgba(0, 0, 0, 0.45);
                  backdrop-filter: blur(2px);
                  opacity: 0;
                  transition: opacity 0.2s ease;
                  cursor: pointer;

                  &:hover {
                    opacity: 1;
                  }

                  .overlay-hero-view-btn {
                    display: inline-flex;
                    align-items: center;
                    gap: 6px;
                    height: 34px;
                    padding: 0 18px;
                    border-radius: 9999px;
                    border: 0;
                    background: rgba(255, 255, 255, 0.95);
                    color: #0f172a;
                    font-size: 13px;
                    font-weight: 600;
                    cursor: pointer;
                    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
                    transition: transform 0.2s;

                    &:hover {
                      transform: scale(1.05);
                      background: #ffffff;
                    }
                  }

                  .overlay-hero-actions {
                    display: flex;
                    align-items: center;
                    gap: 8px;

                    .overlay-action-chip {
                      display: inline-flex;
                      align-items: center;
                      gap: 5px;
                      height: 28px;
                      padding: 0 12px;
                      border-radius: 9999px;
                      border: 1px solid rgba(255, 255, 255, 0.3);
                      background: rgba(0, 0, 0, 0.6);
                      backdrop-filter: blur(8px);
                      color: #ffffff;
                      font-size: 12px;
                      font-weight: 500;
                      cursor: pointer;
                      transition: all 0.2s;

                      &:hover {
                        background: rgba(0, 0, 0, 0.85);
                        border-color: rgba(255, 255, 255, 0.6);
                        transform: translateY(-1px);
                      }
                    }
                  }
                }
              }

              .task-placeholder {
                display: flex;
                min-height: 180px;
                padding: 24px;
                flex-direction: column;
                align-items: center;
                justify-content: center;
                gap: 10px;
                text-align: center;
                background: var(--color-panel-soft);
                color: var(--color-text-muted);

                .pulse-loader-ring {
                  color: #2563eb;
                }

                .empty-icon-box {
                  display: grid;
                  width: 52px;
                  height: 52px;
                  place-items: center;
                  border-radius: 50%;
                  background: var(--color-panel);
                  border: 1px solid var(--color-line);
                  color: var(--color-text-muted);
                }

                &.failed,
                &.interrupted {
                  .empty-icon-box {
                    background: #fef2f2;
                    border-color: #fecaca;
                    color: #ef4444;
                  }
                }

                .placeholder-title {
                  font-weight: 600;
                  font-size: 13.5px;
                  color: var(--color-text);
                }

                .placeholder-hint {
                  font-size: 12px;
                  color: var(--color-text-muted);
                }
              }

              .task-error-box {
                display: flex;
                align-items: flex-start;
                gap: 8px;
                margin: 10px 14px 0;
                padding: 10px 12px;
                border-radius: 8px;
                background: #fef2f2;
                border: 1px solid #fecaca;
                color: #b91c1c;

                .error-icon {
                  flex-shrink: 0;
                  margin-top: 2px;
                  color: #ef4444;
                }

                .task-error-text {
                  margin: 0;
                  font-size: 12px;
                  line-height: 1.5;
                  word-break: break-word;
                  color: #991b1b;
                  text-align: left;
                }
              }

              .task-aux-actions {
                display: flex;
                align-items: center;
                gap: 8px;
                padding: 10px 14px;
                background: var(--color-panel);
                border-top: 1px solid var(--color-line);
                margin-top: 10px;

                .task-btn {
                  display: inline-flex;
                  align-items: center;
                  justify-content: center;
                  gap: 5px;
                  height: 30px;
                  padding: 0 12px;
                  border-radius: 6px;
                  font-size: 12px;
                  font-weight: 500;
                  cursor: pointer;
                  white-space: nowrap;
                  flex-shrink: 0;
                  transition: all 0.2s ease;
                  border: 1px solid var(--color-line);
                  background: var(--color-panel-soft);
                  color: var(--color-text);

                  &:disabled {
                    opacity: 0.5;
                    cursor: not-allowed;
                  }

                  &.task-btn-primary {
                    border-color: #2563eb;
                    background: #2563eb;
                    color: #ffffff;

                    &:hover:not(:disabled) {
                      background: #1d4ed8;
                      border-color: #1d4ed8;
                    }
                  }

                  &.task-btn-warning {
                    border-color: #fde68a;
                    background: #fffbeb;
                    color: #b45309;

                    &:hover:not(:disabled) {
                      background: #fef3c7;
                      border-color: #fcd34d;
                    }
                  }

                  &.task-btn-ghost {
                    border-color: var(--color-line);
                    background: var(--color-panel);
                    color: var(--color-text-muted);

                    &:hover:not(:disabled) {
                      border-color: var(--color-line-strong);
                      color: var(--color-text);
                      background: var(--color-panel-soft);
                    }
                  }
                }
              }
            }
          }
        }
      }

      /* 底部翻页组件 */
      .tasks-pagination {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 12px 18px;
        border-top: 1px solid var(--color-line);
        background: var(--color-panel);

        .tasks-pagination-left {
          display: flex;
          align-items: center;
          gap: 8px;
          font-size: 12.5px;
          color: var(--color-text-muted);

          .pagination-select-wrapper {
            width: 72px;

            .pagination-select {
              height: 28px;
              padding: 0 24px 0 10px;
              border: 1px solid var(--color-line);
              border-radius: 6px;
              background: var(--color-panel-soft);
              color: var(--color-text);
              font-size: 12.5px;
            }
          }
        }

        .tasks-pagination-right {
          display: flex;
          align-items: center;
          gap: 10px;

          .pagination-nav-btn {
            display: inline-flex;
            align-items: center;
            gap: 4px;
            height: 28px;
            padding: 0 10px;
            border-radius: 6px;
            border: 1px solid var(--color-line);
            background: var(--color-panel);
            color: var(--color-text-muted);
            font-size: 12px;
            cursor: pointer;
            transition: all 0.2s;

            &:hover:not(:disabled) {
              border-color: var(--color-line-strong);
              color: var(--color-text);
              background: var(--color-panel-soft);
            }

            &:disabled {
              opacity: 0.4;
              cursor: not-allowed;
            }
          }

          .pagination-indicator {
            font-size: 12.5px;
            font-weight: 600;
            color: var(--color-text);
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          }
        }
      }
    }
  }

  :global(:root[data-theme="dark"]),
  .tools-view--dark & {
    .conversation-toolbar {
      .conversation-chip {
        background: rgba(37, 99, 235, 0.16);
        border-color: rgba(96, 165, 250, 0.35);
        color: #60a5fa;
      }

      .batch-quota-compact {
        background: rgba(37, 99, 235, 0.16);
        border-color: rgba(96, 165, 250, 0.35);
        color: #93c5fd;

        .batch-quota-label,
        .batch-quota-unit {
          color: #bfdbfe;
        }

        .batch-quota-value {
          color: #f1f5f9;
        }
      }
    }

    .create-panel {
      .panel-head .panel-head-badge {
        background: rgba(37, 99, 235, 0.16);
        color: #60a5fa;
      }

      .quota-telemetry-card {
        background: rgba(30, 41, 59, 0.6);
        border-color: rgba(59, 130, 246, 0.25);

        .quota-telemetry-top .quota-telemetry-left {
          .quota-telemetry-icon-box {
            background: var(--color-panel);
            border-color: rgba(59, 130, 246, 0.3);
            color: #60a5fa;
          }

          .quota-telemetry-info {
            .quota-remaining-row {
              .quota-value {
                color: #f1f5f9;
              }
              .quota-label,
              .quota-unit {
                color: #94a3b8;
              }
            }

            .quota-progress-track {
              background: rgba(255, 255, 255, 0.12);
            }
          }
        }

        .quota-telemetry-meta {
          color: #94a3b8;
        }
      }

      .model-picker-list .model-picker-item.active {
        background: rgba(37, 99, 235, 0.2);
        border-color: rgba(96, 165, 250, 0.4);
        color: #93c5fd;
      }

      .workflow-step--references {
        .workflow-step-head .step-mode-pill.edit {
          background: rgba(37, 99, 235, 0.16);
          border-color: rgba(96, 165, 250, 0.35);
          color: #60a5fa;
        }

        .reference-gallery-list .reference-upload-card {
          background: rgba(37, 99, 235, 0.08);
          border-color: rgba(96, 165, 250, 0.35);

          &:hover {
            background: rgba(37, 99, 235, 0.16);
            border-color: #60a5fa;
          }

          .upload-card-content {
            .upload-icon,
            .upload-text {
              color: #60a5fa;
            }

            .upload-sub {
              color: #94a3b8;
            }
          }
        }

        .mask-manager-card {
          .mask-active-row .mask-badge-content .mask-pill-tag {
            background: rgba(139, 92, 246, 0.16);
            border-color: rgba(139, 92, 246, 0.35);
            color: #c4b5fd;
          }

          .mask-empty-row .mask-prompt-right .mask-upload-btn {
            background: rgba(37, 99, 235, 0.14);
            border-color: rgba(96, 165, 250, 0.35);
            color: #60a5fa;

            &:hover {
              background: rgba(37, 99, 235, 0.22);
              border-color: #60a5fa;
            }
          }
        }
      }

      .workflow-submit-footer {
        .workflow-submit-btn {
          background: #2563eb;
          box-shadow: 0 2px 8px rgba(37, 99, 235, 0.4);

          &:hover:not(:disabled) {
            background: #3b82f6;
          }
        }
      }
    }

    /* 批量生成暗色模式适配 */
    .batch-results-panel {
      background: var(--color-panel);
      border-color: var(--color-line);
    }

    .batch-result-card {
      background: var(--color-panel);
      border-color: var(--color-line);

      &:hover {
        border-color: var(--color-line-strong);
        box-shadow: 0 4px 14px rgba(0, 0, 0, 0.3);
      }

      .result-card-media {
        background: var(--color-panel-soft);

        .result-card-badge {
          &.is-completed {
            background: rgba(20, 83, 45, 0.85);
            border-color: rgba(34, 197, 94, 0.4);
            color: #4ade80;
          }
          &.is-processing {
            background: rgba(30, 58, 138, 0.85);
            border-color: rgba(59, 130, 246, 0.4);
            color: #60a5fa;
          }
          &.is-queued {
            background: rgba(113, 63, 18, 0.85);
            border-color: rgba(234, 179, 8, 0.4);
            color: #facc15;
          }
          &.is-failed {
            background: rgba(127, 29, 29, 0.85);
            border-color: rgba(239, 68, 68, 0.4);
            color: #f87171;
          }
        }
      }
    }

    .tasks-panel {
      .tasks-head .tasks-head-left .tasks-head-icon-box {
        background: rgba(37, 99, 235, 0.16);
        color: #60a5fa;
      }

      .tasks-head
        .tasks-head-left
        .tasks-head-info
        .tasks-head-title-row
        .tasks-head-round-badge {
        background: rgba(37, 99, 235, 0.16);
        border-color: rgba(96, 165, 250, 0.35);
        color: #60a5fa;
      }

      .task-placeholder {
        &.failed,
        &.interrupted {
          .empty-icon-box {
            background: rgba(239, 68, 68, 0.14);
            border-color: rgba(239, 68, 68, 0.3);
            color: #f87171;
          }
        }
      }

      .task-error-box {
        background: rgba(239, 68, 68, 0.12);
        border-color: rgba(239, 68, 68, 0.3);

        .error-icon {
          color: #f87171;
        }

        .task-error-text {
          color: #fca5a5;
        }
      }

      .task-aux-actions {
        .task-btn {
          &.task-btn-warning {
            background: rgba(245, 158, 11, 0.15);
            border-color: rgba(245, 158, 11, 0.3);
            color: #fbbf24;

            &:hover:not(:disabled) {
              background: rgba(245, 158, 11, 0.25);
            }
          }
        }
      }

      .task-list .round-card {
        .round-card-head .round-card-head-left {
          .round-num-pill {
            background: rgba(37, 99, 235, 0.16);
            border-color: rgba(96, 165, 250, 0.35);
            color: #60a5fa;
          }

          .round-tag-mode {
            background: rgba(37, 99, 235, 0.16);
            border-color: rgba(96, 165, 250, 0.35);
            color: #60a5fa;
          }
        }

        .round-prompt-box {
          background: rgba(15, 23, 42, 0.6);
          border-color: var(--color-line);
          border-left: 3px solid #3b82f6;
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
}
@keyframes image-workbench-spin {
  to {
    transform: rotate(360deg);
  }
}
@keyframes selection-slide-down {
  from {
    opacity: 0;
    transform: translateY(-6px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
</style>
