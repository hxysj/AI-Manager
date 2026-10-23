<template>
  <section class="lan-share-view">
    <!-- 科技感顶部控制中枢 HUD -->
    <header class="cyber-header">
      <div class="cyber-brand">
        <div class="cyber-logo-box">
          <Share2 :size="18" class="cyber-logo-icon" />
        </div>
        <div class="cyber-brand-text">
          <div class="cyber-brand-title">
            <span class="brand-main">设备快传</span>
            <span class="brand-tag">LAN CYBER DROP</span>
          </div>
          <span class="cyber-local-name">
            NODE // {{ state.native.deviceName || "本机节点" }}
          </span>
        </div>
      </div>

      <div class="cyber-header-actions">
        <!-- 运行状态指示 -->
        <div
          class="cyber-status-chip"
          :class="{
            'cyber-status-online':
              state.service.running && !state.native.hidden,
            'cyber-status-hidden': state.service.running && state.native.hidden
          }"
        >
          <span
            class="cyber-pulse-dot"
            :class="
              state.service.running
                ? state.native.hidden
                  ? 'cyber-pulse-dot--amber'
                  : 'cyber-pulse-dot--emerald'
                : 'cyber-pulse-dot--gray'
            "
          ></span>
          <span>{{
            state.service.running
              ? state.native.hidden
                ? "隐身状态"
                : "在线状态"
              : "离线状态"
          }}</span>
        </div>

        <!-- 连接设备 -->
        <button
          class="cyber-header-btn"
          type="button"
          :disabled="loading"
          @click="openConnectDialog"
        >
          <Link :size="13" />
          <span>{{ state.service.running ? "连接设备" : "上线" }}</span>
        </button>

        <!-- 访客访问 -->
        <button
          class="cyber-header-btn cyber-header-btn--primary"
          type="button"
          :disabled="!state.service.running"
          @click="showAccessDialog"
        >
          <QrCode :size="13" />
          <span>访客扫码</span>
        </button>

        <!-- 删除所有连接设备和所有聊天消息 -->
        <button
          class="cyber-header-btn cyber-header-btn--danger"
          type="button"
          :disabled="loading || clearAllPending"
          title="删除所有连接设备和所有聊天消息"
          @click="openClearAllModal"
        >
          <Trash2 :size="13" />
          <span>删除所有设备与消息</span>
        </button>

        <!-- 设置改为居中弹框，避免下拉菜单被顶部区域裁剪。 -->
        <button
          class="cyber-menu-btn"
          type="button"
          aria-label="快传网络设置"
          :aria-expanded="settingsOpen"
          @click="settingsOpen = true"
        >
          <Settings2 :size="15" />
        </button>
      </div>
    </header>

    <BaseModal
      v-if="settingsOpen"
      class="drop-settings-modal"
      title="设备快传设置"
      description="管理节点可见性和访问地址"
      @close="settingsOpen = false"
    >
      <div class="drop-settings-content">
        <div
          class="drop-settings-status"
          :class="{ 'is-online': state.service.running }"
        >
          <span class="drop-settings-status-dot"></span>
          <div class="drop-settings-status-copy">
            <span>{{ state.service.running ? "服务在线" : "服务已暂停" }}</span>
            <small>{{ serviceSummary }}</small>
          </div>
        </div>

        <div class="drop-settings-list">
          <button
            class="drop-settings-item"
            type="button"
            :disabled="togglingHidden"
            @click="toggleHidden"
          >
            <span class="drop-settings-item-icon"
              ><ShieldCheck :size="17"
            /></span>
            <span class="drop-settings-item-copy">
              <span>{{
                state.native.hidden ? "恢复节点可见" : "隐藏本机节点"
              }}</span>
              <small>{{
                state.native.hidden
                  ? "已配对设备仍可连接本机"
                  : "隐藏后不会回应新的发现和配对请求"
              }}</small>
            </span>
            <ChevronRight :size="16" class="drop-settings-item-arrow" />
          </button>
        </div>

        <div class="drop-settings-addresses">
          <div class="drop-settings-section-title">
            <QrCode :size="15" />
            <span>二维码访问地址</span>
          </div>
          <div class="drop-settings-address-list">
            <button
              v-for="ip in state.service.lanIps || []"
              :key="ip"
              class="drop-settings-address"
              :class="{ 'is-active': ip === state.service.lanIp }"
              type="button"
              :disabled="
                !state.service.running || ip === state.service.lanIp || loading
              "
              @click="chooseAccessIp(ip)"
            >
              <span>{{ ip }}</span>
              <span>{{
                ip === state.service.lanIp ? "当前使用" : "切换"
              }}</span>
            </button>
            <span
              v-if="!(state.service.lanIps || []).length"
              class="drop-settings-empty"
            >
              服务上线后显示局域网地址
            </span>
          </div>
        </div>

        <div class="drop-settings-danger-card">
          <div class="drop-settings-danger-header">
            <Trash2 :size="14" />
            <span>彻底清理</span>
          </div>
          <p class="drop-settings-danger-desc">
            断开所有连接设备、解除设备配对，并彻底删除全部聊天记录、接收文件及传输缓存。
          </p>
          <button
            class="drop-settings-danger-btn"
            type="button"
            :disabled="clearAllPending"
            @click="openClearAllModal"
          >
            <Trash2 :size="13" />
            <span>删除所有连接设备和所有聊天消息</span>
          </button>
        </div>

        <button
          v-if="state.service.running"
          class="drop-settings-offline"
          type="button"
          @click="stopService"
        >
          <Power :size="16" />
          <span>暂停接收并离线</span>
        </button>
        <button
          v-else
          class="drop-settings-offline"
          type="button"
          @click="startService"
        >
          <Power :size="16" />
          <span>重新上线</span>
        </button>
      </div>
    </BaseModal>

    <!-- 中枢主体区 -->
    <div class="cyber-body">
      <!-- 侧边节点导航栏 -->
      <aside class="cyber-sidebar">
        <!-- 切换 Tab: 设备与群聊 -->
        <div class="cyber-sidebar-tools">
          <div class="cyber-nav-tabs">
            <button
              class="cyber-nav-tab"
              :class="{ 'cyber-nav-tab--active': chatMode === 'direct' }"
              type="button"
              @click="switchChatMode('direct')"
            >
              <span class="tab-label">设备节点</span>
              <span class="tab-badge">{{ visibleDevices.length }}</span>
            </button>
            <button
              class="cyber-nav-tab"
              :class="{ 'cyber-nav-tab--active': chatMode === 'group' }"
              type="button"
              @click="switchChatMode('group')"
            >
              <span class="tab-label">加密群聊</span>
              <span class="tab-badge">{{ state.groups.length }}</span>
            </button>
          </div>
          <button
            v-if="chatMode === 'direct'"
            class="cyber-icon-action-btn cyber-icon-action-btn--danger"
            type="button"
            title="删除所有连接设备和所有聊天消息"
            aria-label="删除所有连接设备和所有聊天消息"
            :disabled="loading || clearAllPending"
            @click="openClearAllModal"
          >
            <Trash2 :size="14" />
          </button>
          <button
            v-if="chatMode === 'group'"
            class="cyber-icon-action-btn"
            type="button"
            title="创建群聊"
            aria-label="创建群聊"
            @click="openCreateGroup"
          >
            <Plus :size="15" />
          </button>
        </div>

        <!-- 搜索框 -->
        <div class="cyber-search-wrapper">
          <Search :size="13" class="cyber-search-icon" />
          <input
            v-model="deviceKeyword"
            class="cyber-search-input"
            :placeholder="
              chatMode === 'group' ? '搜索群聊信道...' : '搜索节点名称或 IP...'
            "
            aria-label="搜索设备或群聊"
          />
        </div>

        <!-- 节点列表 -->
        <nav class="cyber-conversations" aria-label="快传会话">
          <template v-if="chatMode === 'direct'">
            <div
              v-for="device in visibleDevices"
              :key="device.id"
              class="cyber-device-card"
              :class="{
                'cyber-device-card--active': selectedDeviceId === device.id
              }"
            >
              <button
                class="cyber-device-select"
                type="button"
                :disabled="deleteDevicePending"
                @click="chooseDevice(device)"
              >
                <!-- 节点图标 -->
                <div
                  class="cyber-device-avatar"
                  :class="{ 'is-online': device.online }"
                >
                  <MonitorSmartphone v-if="device.native" :size="17" />
                  <Globe v-else :size="17" />
                </div>

                <!-- 节点文字信息 -->
                <div class="cyber-device-meta">
                  <div class="cyber-device-head">
                    <span
                      class="cyber-device-name"
                      :title="device.name || device.autoName"
                    >
                      {{ device.name || device.autoName || "未命名设备" }}
                    </span>
                    <span
                      class="cyber-node-tag"
                      :class="device.native ? 'tag-client' : 'tag-web'"
                    >
                      {{ device.native ? "P2P" : "WEB" }}
                    </span>
                    <span
                      v-if="deviceUnreadCount(device.id)"
                      class="cyber-unread-badge"
                    >
                      {{ formatUnreadCount(deviceUnreadCount(device.id)) }}
                    </span>
                  </div>

                  <div
                    class="cyber-device-preview"
                    :title="devicePreviewText(device)"
                  >
                    {{ devicePreviewText(device) }}
                  </div>

                  <div class="cyber-device-telemetry">
                    <span class="telemetry-ip">{{
                      device.ip || "历史设备"
                    }}</span>
                  </div>
                </div>
              </button>

              <button
                class="cyber-device-delete-btn"
                type="button"
                title="删除设备"
                :aria-label="`删除设备 ${device.name || device.autoName || '未命名设备'}`"
                :disabled="deleteDevicePending"
                @click="requestDeleteDevice(device)"
              >
                <Trash2 :size="13" />
              </button>
            </div>

            <!-- 空状态：雷达旋转动画 -->
            <div v-if="!visibleDevices.length" class="cyber-sidebar-empty">
              <div class="cyber-radar-hud">
                <div class="radar-circle radar-circle--1"></div>
                <div class="radar-circle radar-circle--2"></div>
                <div class="radar-sweep-hand"></div>
                <Radio :size="22" class="radar-center-icon" />
              </div>
              <span class="cyber-empty-title">SEARCHING FOR NODES</span>
              <small class="cyber-empty-hint">
                点击「连接设备」扫描当前局域网，等待对方同意后建立连接。
              </small>
            </div>
          </template>

          <template v-else>
            <div
              v-for="group in visibleGroups"
              :key="group.id"
              class="cyber-device-card"
              :class="{
                'cyber-device-card--active': selectedGroupId === group.id
              }"
            >
              <button
                class="cyber-device-select"
                type="button"
                @click="selectGroup(group.id)"
              >
                <div class="cyber-device-avatar cyber-device-avatar--group">
                  <Users :size="17" />
                </div>
                <div class="cyber-device-meta">
                  <div class="cyber-device-head">
                    <span class="cyber-device-name" :title="group.name">{{
                      group.name
                    }}</span>
                    <span class="cyber-node-tag tag-group">GROUP</span>
                    <span
                      v-if="groupUnreadCount(group.id)"
                      class="cyber-unread-badge"
                    >
                      {{ formatUnreadCount(groupUnreadCount(group.id)) }}
                    </span>
                  </div>

                  <div
                    class="cyber-device-preview"
                    :title="groupLastMessage(group.id)"
                  >
                    {{ groupLastMessage(group.id) }}
                  </div>

                  <div class="cyber-device-telemetry">
                    <span class="telemetry-ip"
                      >{{ group.members?.length || 0 }} 位成员</span
                    >
                  </div>
                </div>
              </button>

              <button
                class="cyber-device-delete-btn"
                type="button"
                title="解散群聊"
                :aria-label="`解散群聊 ${group.name}`"
                @click="deleteGroup(group.id)"
              >
                <Trash2 :size="13" />
              </button>
            </div>

            <div v-if="!visibleGroups.length" class="cyber-sidebar-empty">
              <div class="cyber-radar-hud">
                <div class="radar-circle radar-circle--1"></div>
                <div class="radar-circle radar-circle--2"></div>
                <Users :size="22" class="radar-center-icon" />
              </div>
              <span class="cyber-empty-title">NO ACTIVE GROUPS</span>
              <small class="cyber-empty-hint">
                点击右上角「+」即可创建加密传输群聊。
              </small>
            </div>
          </template>
        </nav>

        <!-- 底部网络遥测 -->
        <div class="cyber-sidebar-telemetry">
          <div class="telemetry-row">
            <span class="telemetry-label">CHANNEL:</span>
            <span class="telemetry-val">
              <Lock :size="12" class="telemetry-icon" />
              <span>P2P DIRECT ENCRYPTED</span>
            </span>
          </div>
          <div class="telemetry-row">
            <span class="telemetry-label">SECURITY:</span>
            <span class="telemetry-val">
              <ShieldCheck :size="12" class="telemetry-icon" />
              <span>ED25519 // AUTH VERIFIED</span>
            </span>
          </div>
        </div>
      </aside>

      <!-- 消息工作台（仅在存在选中设备或群组时渲染） -->
      <LanShareSessionWorkspace
        v-if="hasActiveTarget"
        :chat-mode="chatMode"
        :sessions="deviceSessions"
        :current-group="currentGroup"
        :current-device="currentDevice"
        :selected-session-id="selectedSessionId"
        :current-session="currentSession"
        :current-session-id="currentSessionId"
        :service="state.service"
        :state-version="stateVersion"
        @select-session="selectSession"
        @delete-session="deleteSession"
        @create-session="createNewSession"
        @update-group="updateGroup"
        @remove-group-member="removeGroupMember"
        @clear-group-messages="clearGroupMessages"
        @delete-group="deleteGroup"
        @delete-history="deleteSelectedDeviceHistory"
        @delete-device="requestDeleteDevice(currentDevice)"
        @refresh-state="loadState"
        @preview-file="openPreviewDialog"
        @download-file="downloadFile"
        @copy-text="copyText"
      />
      <div v-else class="cyber-workspace-empty">
        <div class="cyber-workspace-empty-hud">
          <div class="cyber-radar-hud">
            <div class="radar-circle radar-circle--1"></div>
            <div class="radar-circle radar-circle--2"></div>
            <Radio :size="28" class="radar-center-icon" />
          </div>
          <span class="cyber-empty-title">
            {{ chatMode === "group" ? "NO ACTIVE GROUP" : "NO DEVICE SELECTED" }}
          </span>
          <p class="cyber-empty-hint">
            {{
              chatMode === "group"
                ? "请在左侧选择加密群聊或点击「+」创建新群组"
                : "请在左侧选择在线或已配对设备以开启传输会话"
            }}
          </p>
        </div>
      </div>
    </div>

    <!-- 删除设备确认弹窗 -->
    <DeleteConfirmModal
      v-if="deleteDeviceTarget"
      class="drop-delete-device-modal"
      title="删除设备"
      :name="deleteDeviceTarget.name"
      description="确认删除该设备吗？将彻底删除该设备以及所有相关的聊天记录和接收文件。"
      :pending="deleteDevicePending"
      :error="deleteDeviceError"
      @close="closeDeleteDevice"
      @confirm="confirmDeleteDevice"
    />

    <!-- 清空所有设备与聊天消息二次确认弹窗 -->
    <DeleteConfirmModal
      v-if="clearAllModalOpen"
      class="drop-delete-device-modal"
      title="删除所有连接设备和所有聊天消息"
      name="所有连接设备及所有聊天消息"
      description="确认彻底删除所有连接设备和所有聊天消息吗？此操作将断开所有设备连接、解除所有设备配对，并彻底删除全部直接会话、群聊消息、接收文件及传输记录。"
      :pending="clearAllPending"
      :error="clearAllError"
      @close="closeClearAllModal"
      @confirm="confirmClearAll"
    />

    <!-- 访客访问二维码弹窗 -->
    <LanShareAccessDialog
      v-if="accessDialogOpen"
      :qr-svg="accessQrSvg"
      :access-url="state.service.accessUrl"
      @close="accessDialogOpen = false"
      @copy-url="copyAccessUrl"
    />

    <!-- 文件预览弹窗 -->
    <LanSharePreviewDialog
      v-if="previewDialog.open"
      :file="previewDialog.file"
      :preview-url="previewDialog.previewUrl"
      :preview-kind="previewDialog.previewKind"
      :text-content="previewDialog.textContent"
      :saving="savingFile"
      @close="closePreviewDialog"
      @download="downloadFile"
    />

    <!-- 文件保存成功提示 -->
    <BaseModal
      v-if="savedFile"
      class="drop-save-success-modal"
      title="文件保存成功"
      @close="savedFile = null"
    >
      <template #header>
        <button
          class="drop-save-success-close"
          type="button"
          aria-label="关闭"
          @click="savedFile = null"
        >
          <X :size="16" />
        </button>
      </template>

      <div class="drop-save-success-content">
        <!-- 顶部绿色发光成功徽标与散落微粒 -->
        <div class="drop-save-success-illustration" aria-hidden="true">
          <svg
            class="drop-save-success-badge-svg"
            viewBox="0 0 100 100"
            fill="none"
            xmlns="http://www.w3.org/2000/svg"
          >
            <defs>
              <filter
                id="saveSuccessHalo"
                x="-30%"
                y="-30%"
                width="160%"
                height="160%"
              >
                <feGaussianBlur stdDeviation="8" />
              </filter>
              <linearGradient
                id="saveSuccessGrad"
                x1="20"
                y1="20"
                x2="80"
                y2="80"
                gradientUnits="userSpaceOnUse"
              >
                <stop offset="0%" stop-color="#10b981" />
                <stop offset="100%" stop-color="#059669" />
              </linearGradient>
            </defs>

            <!-- 柔和外发光圈 -->
            <circle
              cx="50"
              cy="50"
              r="34"
              fill="#10b981"
              opacity="0.22"
              filter="url(#saveSuccessHalo)"
            />

            <!-- 散落彩色微粒 -->
            <circle cx="28" cy="22" r="3" fill="#60a5fa" opacity="0.8" />
            <circle cx="75" cy="24" r="2.5" fill="#34d399" opacity="0.85" />
            <circle cx="88" cy="48" r="3.5" fill="#60a5fa" opacity="0.75" />
            <circle cx="16" cy="50" r="2.5" fill="#2dd4bf" opacity="0.8" />
            <circle cx="24" cy="74" r="3" fill="#38bdf8" opacity="0.8" />
            <circle cx="78" cy="74" r="2.5" fill="#34d399" opacity="0.85" />

            <!-- 中心绿色主徽标 -->
            <circle cx="50" cy="50" r="27" fill="url(#saveSuccessGrad)" />

            <!-- 居中白色对勾 -->
            <path
              d="M39 50.5 L46.5 58 L61 43.5"
              stroke="#ffffff"
              stroke-width="3.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </div>

        <h2 class="drop-save-success-title">文件保存成功</h2>
        <p class="drop-save-success-desc">
          “<span class="drop-save-success-filename">{{ savedFile.name }}</span
          >”已保存到本地。
        </p>

        <!-- 路径卡片 -->
        <div class="drop-save-success-path-card">
          <div class="drop-save-success-path-icon" aria-hidden="true">
            <Folder :size="18" />
          </div>
          <span
            class="drop-save-success-path-text"
            :title="savedFile.path"
            @click="copySavedFilePath"
          >
            {{ savedFile.path }}
          </span>
          <button
            class="drop-save-success-copy-btn"
            :class="{ 'is-copied': savedPathCopied }"
            type="button"
            :title="savedPathCopied ? '已复制' : '复制文件路径'"
            aria-label="复制文件路径"
            @click="copySavedFilePath"
          >
            <Check v-if="savedPathCopied" :size="15" :stroke-width="2.5" />
            <Copy v-else :size="15" />
          </button>
        </div>

        <!-- 底部操作按钮 -->
        <div class="drop-save-success-actions">
          <button
            class="drop-save-btn drop-save-btn--cancel"
            type="button"
            @click="savedFile = null"
          >
            关闭
          </button>
          <button
            class="drop-save-btn drop-save-btn--primary"
            type="button"
            @click="openSavedFileDirectory"
          >
            <FolderOpen :size="16" />
            <span>打开所在目录</span>
          </button>
        </div>
      </div>
    </BaseModal>

    <!-- 扫描候选只在连接弹窗展示，双方确认后才加入设备节点列表。 -->
    <BaseModal
      v-if="connectDialogOpen"
      class="drop-connect-modal"
      title="连接设备"
      @close="connectDialogOpen = false"
    >
      <template #header>
        <div class="drop-connect-header">
          <h2 class="drop-connect-title">连接设备</h2>
          <p class="drop-connect-description">
            选择局域网内发现的设备，核对安全校验码并等待对方同意后建立连接。
          </p>
        </div>
        <button
          class="drop-connect-close"
          type="button"
          aria-label="关闭"
          @click="connectDialogOpen = false"
        >
          <X :size="16" />
        </button>
      </template>

      <div class="drop-connect-content">
        <div class="drop-connect-filter-box">
          <el-input
            v-model="connectIpFilter"
            class="drop-connect-filter-input"
            placeholder="按 IP 地址过滤，例如：192.168.1"
            aria-label="按 IP 地址过滤发现的设备"
            clearable
          >
            <template #prefix>
              <Search :size="15" class="drop-connect-filter-icon" />
            </template>
          </el-input>
        </div>

        <div class="drop-connect-list" aria-label="发现的设备">
          <div
            v-for="peer in discoveredPeers"
            :key="peer.id"
            class="drop-connect-card drop-connect-device"
            :class="{
              'is-waiting': !!peer.pairingCode,
              'is-ready': !peer.pairingCode
            }"
            @click="connectDiscoveredPeer(peer)"
          >
            <div class="drop-connect-avatar">
              <component :is="getPeerDeviceIcon(peer)" :size="20" />
            </div>
            <div class="drop-connect-info drop-connect-device-info">
              <div class="drop-connect-name-row">
                <span class="drop-connect-name drop-connect-device-name">
                  {{ peer.name || "未命名设备" }}
                </span>
                <span
                  class="drop-connect-badge"
                  :class="
                    peer.pairingCode
                      ? 'drop-connect-badge--waiting'
                      : 'drop-connect-badge--ready'
                  "
                >
                  <span class="badge-dot">●</span>
                  {{
                    peer.pairingCode
                      ? typeof peer.pairingCode === "string" &&
                        peer.pairingCode !== "true"
                        ? `待确认 · ${peer.pairingCode}`
                        : "待确认"
                      : "可连接"
                  }}
                </span>
              </div>
              <span class="drop-connect-ip drop-connect-device-ip">
                {{ peer.ip }}:{{ peer.port }}
              </span>
            </div>
            <button
              class="drop-connect-btn"
              :class="
                peer.pairingCode
                  ? 'drop-connect-btn--waiting'
                  : 'drop-connect-btn--primary'
              "
              type="button"
              :disabled="loading || !!peer.pairingCode"
              @click.stop="connectDiscoveredPeer(peer)"
            >
              <Link2 :size="15" />
              <span class="drop-connect-device-status">连接</span>
            </button>
          </div>
          <span v-if="!discoveredPeers.length" class="drop-connect-empty">
            {{
              scanning || state.native.scanning
                ? "正在扫描局域网设备…"
                : connectIpFilter.trim()
                  ? "没有匹配该 IP 的设备"
                  : "暂无可连接设备，请确认对方已上线且未隐藏节点"
            }}
          </span>
        </div>

        <div class="drop-connect-hint-row">
          <Info :size="15" class="drop-connect-hint-icon" />
          <span class="drop-connect-hint">
            未确认、已拒绝或相同节点的设备不会加入候选节点列表。隐藏节点不会被其他设备发现，但仍可主动扫描局域网设备。
          </span>
        </div>

        <div class="drop-connect-actions">
          <button
            class="drop-connect-action-close"
            type="button"
            @click="connectDialogOpen = false"
          >
            关闭
          </button>
          <button
            class="drop-connect-action-scan"
            type="button"
            :disabled="loading || scanning || state.native.scanning"
            @click="scanNearby"
          >
            <RefreshCw
              :size="15"
              :class="{ 'is-spinning': scanning || state.native.scanning }"
            />
            <span>重新扫描</span>
          </button>
        </div>
      </div>
    </BaseModal>

    <!-- 创建群聊弹窗 -->
    <BaseModal
      v-if="createGroupOpen"
      class="drop-group-modal"
      title="创建加密群聊信道"
      @close="createGroupOpen = false"
    >
      <template #header>
        <div class="drop-group-header">
          <div class="drop-group-header-icon" aria-hidden="true">
            <UserPlus :size="20" />
          </div>
          <div class="drop-group-header-text">
            <h2 class="drop-group-title">创建加密群聊信道</h2>
            <p class="drop-group-description">
              选择局域网内的设备组建多方加密群聊信道
            </p>
          </div>
        </div>
        <button
          class="drop-group-close"
          type="button"
          aria-label="关闭"
          @click="createGroupOpen = false"
        >
          <X :size="16" />
        </button>
      </template>

      <form class="drop-group-form" @submit.prevent="createGroup">
        <div class="drop-group-field">
          <label class="drop-group-field-label" for="group-name-input">
            群聊名称
          </label>
          <el-input
            id="group-name-input"
            v-model="newGroup.name"
            class="drop-group-name-input"
            placeholder="输入群聊名称，例如：团队项目组"
            :disabled="loading"
            clearable
          />
        </div>

        <div class="drop-group-notice" role="note">
          <Info :size="16" class="drop-group-notice-icon" />
          <div class="drop-group-notice-text">
            群聊至少需要 3
            人（包含本机），请至少选择两台设备。离线的网页设备重新连接后可查看邀请。
          </div>
        </div>

        <div class="drop-group-section">
          <div class="drop-group-section-header">
            <span class="drop-group-section-title">可选设备</span>
            <span class="drop-group-count-badge">
              已选 {{ selectedGroupDeviceIds.length }} 台
            </span>
          </div>

          <div
            class="drop-group-devices"
            role="group"
            aria-label="可选设备列表"
          >
            <div
              v-for="device in connectedDevices"
              :key="device.id"
              class="drop-group-device-card"
              :class="{
                'is-selected': newGroup.deviceIds.includes(device.id),
                'is-disabled': loading || device.groupSupported === false
              }"
              role="checkbox"
              :aria-checked="newGroup.deviceIds.includes(device.id)"
              :aria-disabled="loading || device.groupSupported === false"
              :tabindex="device.groupSupported === false ? -1 : 0"
              @click="toggleGroupDevice(device)"
              @keydown.space.prevent="toggleGroupDevice(device)"
              @keydown.enter.prevent="toggleGroupDevice(device)"
            >
              <span
                class="drop-group-checkbox"
                :class="{
                  'is-checked': newGroup.deviceIds.includes(device.id),
                  'is-disabled': device.groupSupported === false
                }"
                aria-hidden="true"
              >
                <Check
                  v-if="newGroup.deviceIds.includes(device.id)"
                  :size="12"
                  :stroke-width="3"
                />
              </span>

              <div class="drop-group-device-avatar" aria-hidden="true">
                <component :is="getPeerDeviceIcon(device)" :size="18" />
              </div>

              <div class="drop-group-device-info">
                <span class="drop-group-device-name">
                  {{ device.name || device.autoName || "未命名设备" }}
                </span>
                <span class="drop-group-device-ip">
                  {{ device.ip || "局域网终端" }}
                </span>
              </div>

              <div class="drop-group-device-status">
                <span
                  v-if="device.groupSupported === false"
                  class="drop-group-status-badge drop-group-status-badge--disabled"
                >
                  客户端版本不支持群聊
                </span>
                <span
                  v-else-if="device.online"
                  class="drop-group-status-badge drop-group-status-badge--online"
                >
                  <span class="status-dot">●</span> 在线
                </span>
                <span
                  v-else
                  class="drop-group-status-badge drop-group-status-badge--offline"
                >
                  <span class="status-dot">●</span> 离线
                </span>
              </div>
            </div>

            <div v-if="!connectedDevices.length" class="drop-group-empty">
              <Users :size="32" class="drop-group-empty-icon" />
              <p class="drop-group-empty-text">暂无可邀请设备</p>
              <p class="drop-group-empty-subtext">
                请先让至少一台移动设备或浏览器通过扫码/访客地址接入。
              </p>
            </div>
          </div>
        </div>

        <div class="drop-group-actions">
          <button
            type="button"
            class="drop-group-btn drop-group-btn--cancel"
            :disabled="loading"
            @click="createGroupOpen = false"
          >
            取消
          </button>
          <button
            type="submit"
            class="drop-group-btn drop-group-btn--primary"
            :disabled="
              loading ||
              selectedGroupDeviceIds.length < 2 ||
              !newGroup.name.trim()
            "
          >
            <UserPlus :size="15" />
            <span>{{ loading ? "正在创建..." : "创建并邀请" }}</span>
          </button>
        </div>
      </form>
    </BaseModal>

    <!-- 右侧消息提示弹框：自动消失并支持手动关闭 -->
    <Teleport to="body">
      <Transition name="drop-toast-slide">
        <aside
          v-if="toastMessage"
          class="drop-right-toast"
          role="alert"
          aria-live="assertive"
          @mouseenter="pauseToast"
          @mouseleave="resumeToast"
        >
          <div class="drop-right-toast__icon">
            <AlertCircle :size="18" />
          </div>
          <div class="drop-right-toast__body">
            <span class="drop-right-toast__title">系统提示</span>
            <p class="drop-right-toast__text">{{ toastMessage }}</p>
          </div>
          <button
            class="drop-right-toast__close"
            type="button"
            aria-label="关闭提示"
            @click="dismissToast"
          >
            <X :size="14" />
          </button>
          <div class="drop-right-toast__bar"></div>
        </aside>
      </Transition>
    </Teleport>
  </section>
</template>

<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue"
import { ElCheckbox, ElInput } from "element-plus"
import "element-plus/es/components/checkbox/style/css"
import "element-plus/es/components/input/style/css"
import {
  AlertCircle,
  Check,
  ChevronRight,
  Copy,
  Folder,
  FolderOpen,
  Globe,
  Info,
  Laptop,
  Link,
  Link2,
  Lock,
  Monitor,
  MonitorSmartphone,
  Plus,
  QrCode,
  Radio,
  RefreshCw,
  Search,
  Settings2,
  Share2,
  ShieldCheck,
  Smartphone,
  Trash2,
  UserPlus,
  Users,
  Power,
  X
} from "lucide-vue-next"
import { lanShareApi, systemApi } from "@/api"
import { createMessage } from "@/utils/message"
import BaseModal from "@/components/BaseModal.vue"
import DeleteConfirmModal from "@/features/providers/components/ProviderDeleteConfirmModal.vue"
import { fileUrl } from "./utils"
import LanShareAccessDialog from "./components/LanShareAccessDialog.vue"
import LanSharePreviewDialog from "./components/LanSharePreviewDialog.vue"
import LanShareSessionWorkspace from "./components/LanShareSessionWorkspace.vue"

const state = reactive({
  service: {
    running: false,
    accessUrl: "",
    qrSvg: "",
    lanIp: "",
    lanIps: [],
    port: 0,
    onlineDevices: 0
  },
  devices: [],
  sessions: [],
  groups: [],
  native: {
    deviceName: "",
    peers: [],
    pairingRequests: [],
    error: "",
    hidden: false,
    scanning: false
  },
  messages: []
})

const loading = ref(false)
const scanning = ref(false)
const togglingHidden = ref(false)
const savingFile = ref(false)
const savedFile = ref(null)
const deleteDeviceTarget = ref(null)
const deleteDevicePending = ref(false)
const deleteDeviceError = ref("")
const clearAllModalOpen = ref(false)
const clearAllPending = ref(false)
const clearAllError = ref("")
const createGroupOpen = ref(false)
// 控制设备快传设置弹框。
const settingsOpen = ref(false)
const newGroup = reactive({ name: "新的群聊", deviceIds: [] })

const deviceKeyword = ref("")
const connectDialogOpen = ref(false)
const connectIpFilter = ref("")
const accessDialogOpen = ref(false)
const accessQrSvg = ref("")
const selectedDeviceId = ref("")
const selectedSessionId = ref("")
const selectedGroupId = ref("")
const chatMode = ref("direct")
const stateVersion = ref(0)
const previewDialog = reactive({
  open: false,
  file: null,
  previewUrl: "",
  previewKind: "unsupported",
  textContent: ""
})

const toastMessage = ref("")
let toastTimer = null
let toastRemaining = 3500
let toastStartTime = 0

function showRightToast(msg, duration = 3500) {
  if (!msg) return
  toastMessage.value = msg
  toastRemaining = duration
  clearTimeout(toastTimer)
  toastStartTime = Date.now()
  toastTimer = setTimeout(() => {
    toastMessage.value = ""
  }, duration)
}

function pauseToast() {
  clearTimeout(toastTimer)
  const elapsed = Date.now() - toastStartTime
  toastRemaining = Math.max(1000, toastRemaining - elapsed)
}

function resumeToast() {
  clearTimeout(toastTimer)
  toastStartTime = Date.now()
  toastTimer = setTimeout(() => {
    toastMessage.value = ""
  }, toastRemaining)
}

function dismissToast() {
  clearTimeout(toastTimer)
  toastMessage.value = ""
}

function getPeerDeviceIcon(peer) {
  const name = String(peer?.name || "").toLowerCase()
  if (
    name.includes("laptop") ||
    name.includes("notebook") ||
    name.includes("macbook") ||
    name.includes("thinkpad")
  ) {
    return Laptop
  }
  if (
    name.includes("phone") ||
    name.includes("mobile") ||
    name.includes("android") ||
    name.includes("iphone")
  ) {
    return Smartphone
  }
  if (
    name.includes("desktop") ||
    name.includes("pc") ||
    name.includes("workstation") ||
    name.includes("server")
  ) {
    return Monitor
  }
  return MonitorSmartphone
}

let stopStateListener = null
let stopDevicesListener = null
let stopNativeListener = null
let scanTimer = null
let initialSessionResolved = false

const serviceSummary = computed(() => {
  if (!state.service.running) {
    return "服务未启动"
  }
  if (!state.service.lanIp) return "网络已断开，等待可用局域网地址"
  return `${state.service.lanIp}:${state.service.port} · ${
    state.service.onlineDevices || 0
  } 台在线`
})

const currentSessionId = computed(() => {
  return currentSession.value?.id || ""
})

const currentSession = computed(() => {
  const session =
    state.sessions.find((item) => item.id === selectedSessionId.value) || null
  if (chatMode.value === "direct") {
    return isDirectSession(session) ? session : null
  }
  if (chatMode.value === "group") {
    return session?.mode === "group" ? session : null
  }
  return session
})

const currentDevice = computed(() => {
  return (
    connectedDevices.value.find(
      (device) => device.id === selectedDeviceId.value
    ) || null
  )
})

const currentGroup = computed(() => {
  return (
    state.groups.find((group) => group.id === selectedGroupId.value) || null
  )
})

const hasActiveTarget = computed(() => {
  if (chatMode.value === "group") {
    return Boolean(currentGroup.value)
  }
  return Boolean(currentDevice.value)
})

const deviceSessions = computed(() => {
  return directSessions.value.filter((session) => {
    return selectedDeviceId.value && isDirectDeviceSession(session)
  })
})

const directSessions = computed(() => {
  return state.sessions.filter((session) => isDirectSession(session))
})

const groupSessions = computed(() => {
  return state.sessions.filter((session) => session.mode === "group")
})

const visibleGroups = computed(() =>
  state.groups.filter((group) =>
    group.name.toLowerCase().includes(deviceKeyword.value.trim().toLowerCase())
  )
)

// 扫描候选和已配对设备使用同一发现状态，但主列表仅展示已确认的连接。
const connectedDevices = computed(() => {
  const devices = new Map(
    state.devices
      .filter((device) => !device.native)
      .map((device) => [device.id, { ...device, groupSupported: true }])
  )
  for (const peer of state.native.peers || []) {
    if (!peer.paired && !peer.remoteDeleted) continue
    devices.set(peer.id, {
      ...state.devices.find((device) => device.id === peer.id),
      ...peer,
      native: true
    })
  }
  return [...devices.values()]
})

const selectedGroupDeviceIds = computed(() =>
  connectedDevices.value
    .filter(
      (device) =>
        device.groupSupported !== false &&
        newGroup.deviceIds.includes(device.id)
    )
    .map((device) => device.id)
)

const visibleDevices = computed(() => {
  const query = deviceKeyword.value.trim().toLowerCase()
  return connectedDevices.value
    .filter((device) =>
      `${device.name || device.autoName || ""} ${device.ip || ""}`
        .toLowerCase()
        .includes(query)
    )
    .sort((left, right) => Number(right.online) - Number(left.online))
})

const discoveredPeers = computed(() => {
  const ip = connectIpFilter.value.trim().toLowerCase()
  return (state.native.peers || [])
    .filter(
      (peer) =>
        !peer.paired &&
        (peer.online || peer.pairingCode) &&
        peer.ip.toLowerCase().includes(ip)
    )
    .sort((left, right) =>
      left.ip.localeCompare(right.ip, undefined, { numeric: true })
    )
})

function deviceLastMessage(deviceId) {
  const sessionIds = new Set(
    state.sessions
      .filter(
        (session) => session.deviceId === deviceId && session.mode !== "group"
      )
      .map((session) => session.id)
  )
  const message = state.messages.find((message) =>
    sessionIds.has(message.sessionId)
  )
  if (!message) return "发送文件或消息"
  return (
    message.content ||
    (message.attachments?.length
      ? `[${message.attachments.length} 个附件] ${message.attachments[0].name}`
      : "文件消息")
  )
}

function groupLastMessage(groupId) {
  const session = groupSessions.value.find((item) => item.groupId === groupId)
  if (!session) return "群聊传输信道"
  const message = state.messages.find((item) => item.sessionId === session.id)
  if (!message) return "发送群文件或消息"
  return (
    message.content ||
    (message.attachments?.length
      ? `[${message.attachments.length} 个附件] ${message.attachments[0].name}`
      : "文件消息")
  )
}

function devicePreviewText(device) {
  if (device.remoteDeleted) {
    return "对方已删除此设备 · 无法发送"
  }
  if (device.pairingCode) {
    return `等待对方确认 · ${device.pairingCode}`
  }
  return deviceLastMessage(device.id)
}

// 设备卡片和群聊卡片按会话记录聚合显示未读数量。
function deviceUnreadCount(deviceId) {
  return state.sessions
    .filter(
      (session) => session.deviceId === deviceId && session.mode !== "group"
    )
    .reduce((total, session) => total + Number(session.unreadCount || 0), 0)
}

function groupUnreadCount(groupId) {
  return state.sessions
    .filter(
      (session) => session.mode === "group" && session.groupId === groupId
    )
    .reduce((total, session) => total + Number(session.unreadCount || 0), 0)
}

function formatUnreadCount(count) {
  return count > 99 ? "99+" : count
}

async function chooseDevice(device) {
  await openDeviceSessions(device.id)
}

async function openConnectDialog() {
  if (!state.service.running) await startService()
  if (!state.service.running) return
  connectIpFilter.value = ""
  if (state.native) state.native.error = ""
  toastMessage.value = ""
  connectDialogOpen.value = true
  await scanNearby()
}

async function connectDiscoveredPeer(peer) {
  if (loading.value || peer.pairingCode || peer.paired) return
  await runAction(() => lanShareApi.connectPeer({ peerId: peer.id }))
}

async function toggleHidden() {
  if (togglingHidden.value) return
  settingsOpen.value = false
  togglingHidden.value = true
  try {
    const result = unwrapData(
      await lanShareApi.setHidden({ hidden: !state.native.hidden })
    )
    state.native = result
    createMessage.success(
      result.hidden ? "节点已隐藏，已配对设备仍可连接" : "节点已恢复可见"
    )
  } catch (error) {
    createMessage.error(error?.message || "设置节点可见性失败")
  } finally {
    togglingHidden.value = false
  }
}

async function scanNearby() {
  if (scanning.value) return
  scanning.value = true
  try {
    await lanShareApi.scanDevices()
    clearTimeout(scanTimer)
    scanTimer = setTimeout(() => {
      scanning.value = false
    }, 2000)
  } catch (err) {
    createMessage.error(err?.message || "设备扫描失败")
    scanning.value = false
  }
}

onMounted(async () => {
  stopStateListener = lanShareApi.onStateChanged(applyState)
  stopDevicesListener = lanShareApi.onDevicesChanged((devices) => {
    state.devices = devices || []
  })
  stopNativeListener = lanShareApi.onNativeChanged((native) => {
    state.native = native
    if (native?.error) {
      showRightToast(native.error)
    }
    // 原生设备的心跳独立推送，收到后同步群聊成员在线状态，避免重新加载聊天记录。
    const onlineById = new Map(
      (native.peers || []).map((peer) => [peer.id, Boolean(peer.online)])
    )
    state.groups = state.groups.map((group) => ({
      ...group,
      members: (group.members || []).map((member) =>
        onlineById.has(member.deviceId)
          ? { ...member, online: onlineById.get(member.deviceId) }
          : member
      )
    }))
  })
  await loadState()
  if (!state.service.running) await startService()
})

watch(
  () => state.native?.error,
  (newErr) => {
    if (newErr) {
      showRightToast(newErr)
    }
  }
)

watch(selectedDeviceId, (deviceId) => {
  if (chatMode.value !== "direct") {
    return
  }
  if (!deviceId) {
    selectedSessionId.value = ""
    return
  }
  if (
    selectedSessionId.value &&
    (!isDirectDeviceSession(currentSession.value) ||
      currentSession.value?.deviceId !== deviceId)
  ) {
    selectedSessionId.value = ""
  }
  if (!selectedSessionId.value) {
    selectedSessionId.value = findDirectSessionId(deviceId)
  }
})

watch(selectedSessionId, (sessionId) => {
  const session = state.sessions.find((item) => item.id === sessionId)
  if (
    session &&
    chatMode.value === "direct" &&
    isDirectSession(session) &&
    selectedDeviceId.value !== session.deviceId
  ) {
    selectedDeviceId.value = session.deviceId
  }
})

onBeforeUnmount(() => {
  if (stopStateListener) stopStateListener()
  if (stopDevicesListener) stopDevicesListener()
  if (stopNativeListener) stopNativeListener()
  clearTimeout(scanTimer)
  clearTimeout(toastTimer)
})

function applyState(payload) {
  const nextState = payload?.service ? payload : unwrapData(payload) || {}
  state.service = nextState.service || state.service
  accessQrSvg.value = state.service.qrSvg || ""
  state.devices = nextState.devices || []
  state.sessions = nextState.sessions || []
  state.groups = nextState.groups || []
  state.messages = nextState.messages || []
  state.native = nextState.native || state.native
  if (nextState.native?.error) {
    showRightToast(nextState.native.error)
  }
  stateVersion.value += 1

  if (nextState.currentSession?.id) {
    if (nextState.currentSession.mode === "group") {
      selectedGroupId.value = nextState.currentSession.groupId || ""
      if (chatMode.value === "group") {
        selectedSessionId.value = nextState.currentSession.id
        selectedDeviceId.value = ""
      }
    } else if (chatMode.value === "direct") {
      selectedSessionId.value = nextState.currentSession.id
      selectedDeviceId.value = nextState.currentSession.deviceId || ""
      selectedGroupId.value = ""
    }
    initialSessionResolved = true
  } else if (!initialSessionResolved) {
    initialSessionResolved = true
  }
  if (
    chatMode.value === "direct" &&
    !selectedDeviceId.value &&
    connectedDevices.value.length
  ) {
    selectedDeviceId.value = connectedDevices.value[0].id
    selectedSessionId.value = findDirectSessionId(selectedDeviceId.value)
  }
}

function unwrapData(result) {
  return result?.status && "data" in result ? result.data : result
}

function isDirectSession(session) {
  return Boolean(session) && session.mode !== "group"
}

function isDirectDeviceSession(session, deviceId = selectedDeviceId.value) {
  return (
    isDirectSession(session) &&
    Boolean(deviceId) &&
    session.deviceId === deviceId
  )
}

function findDirectSessionId(deviceId) {
  return (
    state.sessions.find((session) => {
      return isDirectDeviceSession(session, deviceId)
    })?.id || ""
  )
}

async function runAction(action, successMessage) {
  loading.value = true
  try {
    const result = unwrapData(await action())
    if (result?.service) {
      applyState(result)
    }
    if (successMessage) {
      createMessage.success(successMessage)
    }
    return result
  } catch (error) {
    createMessage.error(error?.message || String(error))
    return null
  } finally {
    loading.value = false
  }
}

async function loadState() {
  await runAction(async () => lanShareApi.getState())
}

async function startService() {
  settingsOpen.value = false
  const result = await runAction(async () => lanShareApi.startService({}))
  if (result) {
    state.service = {
      ...state.service,
      ...result
    }
    await loadState()
  }
}

async function showAccessDialog() {
  if (!state.service.running) {
    return
  }
  if (!accessQrSvg.value) {
    await loadState()
  }
  accessQrSvg.value = state.service.qrSvg || ""
  accessDialogOpen.value = true
}

async function setAccessIp(lanIp) {
  if (
    !state.service.running ||
    !lanIp ||
    lanIp === state.service.lanIp ||
    loading.value
  ) {
    return
  }
  await runAction(
    () => lanShareApi.setAccessIp({ lanIp }),
    `二维码已切换到 ${lanIp}`
  )
}

async function stopService() {
  settingsOpen.value = false
  const result = await runAction(
    async () => lanShareApi.stopService(),
    "设备快传服务已关闭。"
  )
  if (result !== null) {
    accessDialogOpen.value = false
    accessQrSvg.value = ""
    closePreviewDialog()
    await loadState()
  }
}

function chooseAccessIp(lanIp) {
  settingsOpen.value = false
  return setAccessIp(lanIp)
}

async function createNewSession() {
  if (!selectedDeviceId.value) {
    return
  }
  await runAction(
    async () => lanShareApi.createSession({ deviceId: selectedDeviceId.value }),
    "新会话已创建。"
  )
}

function openCreateGroup() {
  newGroup.name = "新的群聊"
  newGroup.deviceIds = []
  createGroupOpen.value = true
}

function toggleGroupDevice(device) {
  if (loading.value || device?.groupSupported === false) return
  const index = newGroup.deviceIds.indexOf(device.id)
  if (index > -1) {
    newGroup.deviceIds.splice(index, 1)
  } else {
    newGroup.deviceIds.push(device.id)
  }
}

async function createGroup() {
  if (
    loading.value ||
    selectedGroupDeviceIds.value.length < 2 ||
    !newGroup.name.trim()
  )
    return
  const payload = {
    name: newGroup.name.trim(),
    messageVisibility: "all",
    deviceIds: [...selectedGroupDeviceIds.value]
  }
  const result = await runAction(
    async () => lanShareApi.createGroup(payload),
    "群聊已创建。"
  )
  if (result?.currentSession) {
    selectedSessionId.value = result.currentSession.id
    selectedGroupId.value = result.currentSession.groupId || ""
    createGroupOpen.value = false
    chatMode.value = "group"
  }
}

async function updateGroup(payload) {
  await runAction(
    async () => lanShareApi.updateGroup(payload),
    "群聊设置已更新。"
  )
  await loadState()
}

async function removeGroupMember(payload) {
  await runAction(
    async () => lanShareApi.removeGroupMember(payload),
    "群成员已移出。"
  )
  await loadState()
}

async function clearGroupMessages(groupId) {
  if (!groupId || !window.confirm("确认清空该群聊的全部消息吗？")) {
    return
  }
  await runAction(
    async () => lanShareApi.clearGroupMessages({ groupId }),
    "群消息已清空。"
  )
  await loadState()
}

async function deleteGroup(groupId) {
  if (!groupId || !window.confirm("确认解散该群聊吗？")) {
    return
  }
  const result = await runAction(
    async () => lanShareApi.deleteGroup({ groupId }),
    "群聊已解散。"
  )
  if (result) {
    selectedGroupId.value = ""
    selectedSessionId.value = ""
  }
  await loadState()
}

async function deleteSelectedDeviceHistory() {
  if (!selectedDeviceId.value) {
    return
  }
  await deleteDeviceHistory(selectedDeviceId.value)
}

async function copyAccessUrl() {
  await copyText(state.service.accessUrl)
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text || "")
    createMessage.success("已复制。")
  } catch (error) {
    createMessage.error(error?.message || "复制失败。")
  }
}

async function selectSession(sessionId) {
  selectedSessionId.value = sessionId
  if (!selectedSessionId.value) {
    return
  }
  await runAction(async () =>
    lanShareApi.activateSession({ sessionId: selectedSessionId.value })
  )
}

async function deleteSession(sessionId) {
  if (!sessionId) {
    return
  }
  const deletedSession = state.sessions.find(
    (session) => session.id === sessionId
  )
  const result = await runAction(
    async () => lanShareApi.deleteSession({ sessionId }),
    "会话已删除。"
  )
  if (!result) {
    return
  }
  const nextSessions = Array.isArray(result.sessions)
    ? result.sessions
    : state.sessions
  const deviceId = deletedSession?.deviceId || selectedDeviceId.value

  if (selectedSessionId.value === sessionId) {
    const nextSessionId =
      nextSessions.find((session) => {
        return session.mode !== "group" && session.deviceId === deviceId
      })?.id || ""
    selectedSessionId.value = nextSessionId
    if (nextSessionId) {
      await runAction(async () =>
        lanShareApi.activateSession({ sessionId: nextSessionId })
      )
    }
  }
  await loadState()
}

async function openDeviceSessions(deviceId) {
  selectedDeviceId.value = deviceId
  if (isDirectDeviceSession(currentSession.value, deviceId)) {
    await selectSession(selectedSessionId.value)
    return
  }
  selectedSessionId.value = findDirectSessionId(deviceId)
  if (!selectedSessionId.value) await createNewSession()
  else await selectSession(selectedSessionId.value)
}

function switchChatMode(mode) {
  chatMode.value = mode
  if (mode === "group") {
    selectedDeviceId.value = ""
    selectedGroupId.value = selectedGroupId.value || state.groups[0]?.id || ""
    selectGroup(selectedGroupId.value)
    return
  }
  selectedGroupId.value = ""
  selectedSessionId.value = findDirectSessionId(selectedDeviceId.value)
  if (selectedSessionId.value) {
    selectSession(selectedSessionId.value)
  }
}

async function selectGroup(groupId) {
  selectedGroupId.value = groupId
  chatMode.value = "group"
  const groupSession =
    groupSessions.value.find((session) => {
      return session.groupId === groupId && !session.deviceId
    }) || groupSessions.value.find((session) => session.groupId === groupId)

  selectedSessionId.value = groupSession?.id || ""
  if (selectedSessionId.value) {
    await runAction(async () =>
      lanShareApi.activateSession({ sessionId: selectedSessionId.value })
    )
  }
}

async function deleteDeviceHistory(deviceId) {
  if (!deviceId || !window.confirm("删除这个设备的本机会话历史？")) {
    return
  }
  await runAction(
    async () => lanShareApi.deleteDeviceHistory({ deviceId }),
    "设备历史已删除。"
  )
  if (selectedDeviceId.value === deviceId) {
    selectedDeviceId.value = ""
    selectedSessionId.value = ""
  }
  await loadState()
}

function requestDeleteDevice(device) {
  if (!device?.id || deleteDevicePending.value || loading.value) return
  deleteDeviceTarget.value = {
    id: device.id,
    name: device.name || device.autoName || "未命名设备",
    native: Boolean(device.native)
  }
  deleteDeviceError.value = ""
}

function closeDeleteDevice() {
  if (deleteDevicePending.value) return
  deleteDeviceTarget.value = null
  deleteDeviceError.value = ""
}

async function confirmDeleteDevice() {
  if (!deleteDeviceTarget.value || deleteDevicePending.value) return
  const deviceId = deleteDeviceTarget.value.id
  deleteDevicePending.value = true
  deleteDeviceError.value = ""
  try {
    const result = unwrapData(
      await lanShareApi.deleteDevice({
        deviceId,
        deleteHistory: true
      })
    )
    if (selectedDeviceId.value === deviceId) {
      selectedDeviceId.value = ""
      selectedSessionId.value = ""
    }
    applyState(result)
    deleteDeviceTarget.value = null
    createMessage.success("设备及聊天记录已彻底删除。")
  } catch (error) {
    deleteDeviceError.value = error?.message || String(error)
  } finally {
    deleteDevicePending.value = false
  }
}

function openClearAllModal() {
  if (clearAllPending.value || loading.value) return
  clearAllError.value = ""
  clearAllModalOpen.value = true
}

function closeClearAllModal() {
  if (clearAllPending.value) return
  clearAllModalOpen.value = false
  clearAllError.value = ""
}

async function confirmClearAll() {
  if (clearAllPending.value) return
  clearAllPending.value = true
  clearAllError.value = ""
  try {
    const result = unwrapData(await lanShareApi.deleteAllDevicesAndMessages())
    selectedDeviceId.value = ""
    selectedSessionId.value = ""
    selectedGroupId.value = ""
    applyState(result)
    clearAllModalOpen.value = false
    createMessage.success("已彻底删除所有连接设备和聊天消息。")
  } catch (error) {
    clearAllError.value = error?.message || String(error)
  } finally {
    clearAllPending.value = false
  }
}

function openPreviewDialog(file) {
  if (!state.service.running) {
    createMessage.warning("请先启动服务后再预览共享文件。")
    return
  }
  previewDialog.file = {
    ...file,
    sessionId: file.sessionId || currentSessionId.value
  }
  previewDialog.previewKind = previewKind(file)
  previewDialog.previewUrl = fileServiceUrl(file, "preview")
  previewDialog.textContent = ""
  previewDialog.open = true
}

function closePreviewDialog() {
  previewDialog.open = false
  previewDialog.file = null
  previewDialog.previewUrl = ""
  previewDialog.previewKind = "unsupported"
  previewDialog.textContent = ""
}

async function downloadFile(file) {
  if (savingFile.value) return
  savingFile.value = true
  try {
    const targetPath = await systemApi.saveFile({
      title: "保存快传文件",
      defaultPath: file.name
    })
    if (!targetPath) return
    await lanShareApi.saveFile({ fileId: file.id, targetPath })
    savedFile.value = {
      name: file.name,
      path: targetPath,
      directory: targetPath.replace(/[\\/][^\\/]*$/, "") || targetPath
    }
  } catch (error) {
    createMessage.error(`保存失败：${error?.message || error}`)
  } finally {
    savingFile.value = false
  }
}

const savedPathCopied = ref(false)
let copyPathTimer = null

async function copySavedFilePath() {
  if (!savedFile.value?.path) return
  try {
    if (navigator?.clipboard?.writeText) {
      await navigator.clipboard.writeText(savedFile.value.path)
    } else {
      const textarea = document.createElement("textarea")
      textarea.value = savedFile.value.path
      textarea.style.position = "fixed"
      textarea.style.opacity = "0"
      document.body.appendChild(textarea)
      textarea.select()
      document.execCommand("copy")
      document.body.removeChild(textarea)
    }
    savedPathCopied.value = true
    createMessage.success("已复制文件路径")
    clearTimeout(copyPathTimer)
    copyPathTimer = setTimeout(() => {
      savedPathCopied.value = false
    }, 2000)
  } catch (error) {
    createMessage.error(`复制路径失败：${error?.message || error}`)
  }
}

async function openSavedFileDirectory() {
  if (!savedFile.value?.directory) return
  try {
    await systemApi.openPath({ targetPath: savedFile.value.directory })
  } catch (error) {
    createMessage.error(`打开目录失败：${error?.message || error}`)
  }
}

function fileServiceUrl(file, action) {
  return fileUrl(
    state.service,
    file,
    file.sessionId || currentSessionId.value,
    action
  )
}

function previewKind(file) {
  const name = String(file?.name || "").toLowerCase()
  const mimeType = String(file?.mimeType || "").toLowerCase()
  if (mimeType.startsWith("image/")) return "image"
  if (mimeType.startsWith("video/")) return "video"
  if (mimeType.startsWith("audio/")) return "audio"
  if (mimeType === "application/pdf" || name.endsWith(".pdf")) return "pdf"
  if (mimeType.startsWith("text/") || isTextPreviewFile(name, mimeType))
    return "text"
  return "unsupported"
}

function isTextPreviewFile(name, mimeType) {
  const textMimeTypes = [
    "application/json",
    "application/xml",
    "application/javascript",
    "application/x-javascript",
    "application/xhtml+xml",
    "image/svg+xml"
  ]
  const textExtensions = [
    ".txt",
    ".md",
    ".json",
    ".xml",
    ".csv",
    ".log",
    ".js",
    ".ts",
    ".css",
    ".html",
    ".vue",
    ".rs",
    ".py",
    ".java",
    ".c",
    ".cpp",
    ".h",
    ".go",
    ".yaml",
    ".yml",
    ".toml",
    ".ini",
    ".conf",
    ".sql",
    ".sh",
    ".ps1"
  ]
  return (
    textMimeTypes.includes(mimeType) ||
    textExtensions.some((extension) => name.endsWith(extension))
  )
}
</script>

<style scoped lang="less">
.lan-share-view {
  display: flex;
  height: 100%;
  min-height: 0;
  min-width: 0;
  flex-direction: column;
  overflow: hidden;
  background: var(--color-page);
  padding: 10px 14px 14px;
  box-sizing: border-box;

  /* 顶部科技感控制中枢 HUD */
  .cyber-header {
    display: flex;
    min-width: 0;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 10px 16px;
    margin-bottom: 12px;
    border-radius: 10px;
    background: var(--color-panel);
    border: 1px solid var(--color-line);
    box-shadow: var(--shadow-panel);

    .cyber-brand {
      display: flex;
      align-items: center;
      gap: 12px;

      .cyber-logo-box {
        position: relative;
        display: grid;
        width: 36px;
        height: 36px;
        place-items: center;
        border-radius: 8px;
        background: var(--color-primary-soft);
        border: 1px solid var(--color-info-line);
        color: var(--color-primary);
        box-shadow: 0 0 10px var(--color-primary-soft);

        .cyber-logo-ring {
          position: absolute;
          inset: -3px;
          border-radius: 10px;
          border: 1px dashed var(--color-primary);
          opacity: 0.5;
          animation: cyberSpin 15s linear infinite;
        }
      }

      .cyber-brand-text {
        display: flex;
        flex-direction: column;
        gap: 2px;

        .cyber-brand-title {
          display: flex;
          align-items: center;
          gap: 8px;

          .brand-main {
            color: var(--color-text);
            font-size: 16px;
            font-weight: 700;
            letter-spacing: 0.5px;
          }

          .brand-tag {
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 9.5px;
            padding: 1px 6px;
            border-radius: 3px;
            background: var(--color-primary-soft);
            border: 1px solid var(--color-info-line);
            color: var(--color-primary);
            letter-spacing: 1px;
            font-weight: 600;
          }
        }

        .cyber-local-name {
          color: var(--color-text-muted);
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 11px;
          letter-spacing: 0.5px;
        }
      }
    }

    .cyber-header-actions {
      display: flex;
      align-items: center;
      gap: 8px;

      .cyber-status-chip {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 5px 10px;
        border-radius: 6px;
        background: var(--color-panel-soft);
        border: 1px solid var(--color-line);
        font-family:
          ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        font-size: 11px;
        color: var(--color-text-muted);

        &.cyber-status-online {
          background: var(--color-success-soft);
          border-color: var(--color-success-line);
          color: var(--color-success);
        }

        &.cyber-status-hidden {
          background: var(--color-warning-soft);
          border-color: var(--color-warning-line);
          color: var(--color-warning);
        }
      }

      .cyber-header-btn {
        display: inline-flex;
        flex-shrink: 0;
        align-items: center;
        gap: 6px;
        height: 31px;
        padding: 0 11px;
        white-space: nowrap;
        border-radius: 6px;
        border: 1px solid var(--color-line);
        background: var(--color-panel-soft);
        color: var(--color-text);
        font-size: 12.5px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.2s;

        &:hover:not(:disabled) {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
          color: var(--color-primary);
        }

        &--active {
          border-color: var(--color-success-line);
          background: var(--color-success-soft);
          color: var(--color-success);
        }

        &--radar {
          &.is-scanning {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
            color: var(--color-primary);
            box-shadow: 0 0 10px var(--color-primary-soft);
          }
        }

        &--primary {
          border-color: var(--color-primary);
          background: var(--color-primary-solid);
          color: #ffffff !important;
          min-width: 82px;

          svg,
          span {
            color: #ffffff !important;
          }

          &:hover:not(:disabled) {
            background: var(--color-primary);
            color: #ffffff !important;
            box-shadow: 0 2px 10px rgba(0, 0, 0, 0.12);
          }

          &:hover:not(:disabled) svg,
          &:hover:not(:disabled) span {
            color: #ffffff !important;
          }
        }

        &--danger {
          border-color: rgba(239, 68, 68, 0.28);
          color: var(--color-danger, #ef4444);

          &:hover:not(:disabled) {
            border-color: var(--color-danger, #ef4444);
            background: var(--color-danger-soft, rgba(239, 68, 68, 0.1));
            color: var(--color-danger, #ef4444);
          }
        }

        &:disabled {
          opacity: 0.4;
          cursor: not-allowed;
        }
      }

      .cyber-menu-btn {
        display: grid;
        width: 31px;
        height: 31px;
        place-items: center;
        padding: 0;
        border-radius: 6px;
        border: 1px solid var(--color-line);
        background: var(--color-panel-soft);
        color: var(--color-text-muted);
        cursor: pointer;
        transition: all 0.2s;

        &:hover {
          color: var(--color-primary);
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
        }
      }
    }
  }

  /* 主体双栏区域 */
  .cyber-body {
    display: flex;
    min-height: 0;
    min-width: 0;
    flex: 1;
    overflow: hidden;
    gap: 12px;

    /* 侧边节点栏独立卡片 */
    .cyber-sidebar {
      display: flex;
      width: clamp(230px, 24%, 280px);
      min-width: 0;
      flex: none;
      flex-direction: column;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);
      box-shadow: var(--shadow-panel);
      overflow: hidden;

      .cyber-sidebar-tools {
        display: flex;
        flex: none;
        align-items: center;
        justify-content: space-between;
        padding: 0 14px;
        height: 44px;
        border-bottom: 1px solid var(--color-line);

        .cyber-nav-tabs {
          display: flex;
          gap: 16px;
          height: 100%;

          .cyber-nav-tab {
            position: relative;
            display: inline-flex;
            align-items: center;
            gap: 6px;
            height: 100%;
            padding: 0 2px;
            margin-bottom: -1px;
            border: 0;
            border-bottom: 2.5px solid transparent;
            background: transparent;
            color: var(--color-text-muted);
            font-size: 13px;
            font-weight: 500;
            cursor: pointer;
            transition: all 0.2s;

            .tab-label {
              line-height: 1;
            }

            .tab-badge {
              font-family:
                ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
              font-size: 10px;
              line-height: 1.2;
              padding: 1px 6px;
              border-radius: 4px;
              background: var(--color-panel-soft);
              border: 1px solid var(--color-line);
              color: var(--color-text-soft);
              font-weight: 500;
              transition: all 0.2s;
            }

            &:hover {
              color: var(--color-text);
            }

            &--active {
              color: var(--color-primary);
              font-weight: 600;
              border-bottom-color: var(--color-primary);

              .tab-badge {
                background: var(--color-primary-soft);
                border-color: var(--color-info-line);
                color: var(--color-primary);
                font-weight: 600;
              }
            }
          }
        }

        .cyber-icon-action-btn {
          display: grid;
          width: 26px;
          height: 26px;
          place-items: center;
          padding: 0;
          border-radius: 5px;
          border: 1px solid var(--color-line);
          background: var(--color-panel);
          color: var(--color-primary);
          cursor: pointer;
          transition: all 0.2s;

          &:hover {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
          }

          &--danger {
            color: var(--color-danger, #ef4444);

            &:hover {
              border-color: var(--color-danger, #ef4444);
              background: var(--color-danger-soft, rgba(239, 68, 68, 0.1));
            }
          }
        }
      }

      .cyber-search-wrapper {
        display: flex;
        flex: none;
        align-items: center;
        gap: 8px;
        margin: 12px 12px 8px;
        padding: 6px 10px;
        border-radius: 7px;
        background: var(--color-panel);
        border: 1px solid var(--color-line);
        transition: all 0.2s;

        &:focus-within {
          border-color: var(--color-primary);
          box-shadow: 0 0 0 2px var(--color-primary-soft);
        }

        .cyber-search-icon {
          color: var(--color-text-soft);
          flex-shrink: 0;
        }

        .cyber-search-input {
          width: 100%;
          min-width: 0;
          padding: 0;
          border: 0;
          outline: none;
          background: transparent;
          color: var(--color-text);
          font-size: 12px;

          &::placeholder {
            color: var(--color-text-soft);
            font-family: inherit;
            font-size: 12px;
          }
        }
      }

      .cyber-conversations {
        min-height: 0;
        flex: 1;
        padding: 4px 10px 10px;
        overflow-y: auto;
        scrollbar-width: thin;
        scrollbar-color: var(--color-line) transparent;

        .cyber-device-card {
          display: flex;
          width: 100%;
          box-sizing: border-box;
          align-items: center;
          gap: 6px;
          margin-bottom: 6px;
          padding: 8px 10px;
          border-radius: 8px;
          border: 1px solid var(--color-line);
          background: var(--color-panel);
          transition: all 0.2s;

          &:hover {
            border-color: var(--color-line-strong);
            background: var(--color-panel-soft);
          }

          &--active {
            border-color: var(--color-primary);
            background: #f8fafc;
            box-shadow: 0 1px 4px rgba(37, 99, 235, 0.06);
          }

          .cyber-device-select {
            display: flex;
            min-width: 0;
            flex: 1;
            align-items: center;
            gap: 10px;
            padding: 0;
            border: 0;
            background: transparent;
            color: inherit;
            text-align: left;
            cursor: pointer;

            .cyber-device-avatar {
              position: relative;
              display: grid;
              width: 38px;
              height: 38px;
              flex: none;
              place-items: center;
              border-radius: 8px;
              background: var(--color-panel-soft);
              border: 1px solid var(--color-line);
              color: var(--color-text-muted);

              &.is-online {
                color: #10b981;
                border-color: #86efac;
                background: #dcfce7;
              }

              &--group {
                color: var(--color-primary);
                border-color: var(--color-info-line);
                background: var(--color-primary-soft);
              }
            }

            .cyber-device-meta {
              display: flex;
              min-width: 0;
              flex: 1;
              flex-direction: column;
              gap: 3px;

              .cyber-device-head {
                display: flex;
                align-items: center;
                justify-content: space-between;
                gap: 6px;

                .cyber-device-name {
                  overflow: hidden;
                  text-overflow: ellipsis;
                  white-space: nowrap;
                  font-size: 13px;
                  font-weight: 600;
                  color: var(--color-text);
                }

                .cyber-node-tag {
                  font-family:
                    ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                    monospace;
                  font-size: 9.5px;
                  padding: 1px 5px;
                  border-radius: 3px;
                  font-weight: 600;

                  &.tag-client {
                    background: var(--color-primary-soft);
                    border: 1px solid var(--color-info-line);
                    color: var(--color-primary);
                  }

                  &.tag-web {
                    background: var(--color-warning-soft);
                    border: 1px solid var(--color-warning-line);
                    color: var(--color-warning);
                  }

                  &.tag-group {
                    background: var(--color-primary-soft);
                    border: 1px solid var(--color-info-line);
                    color: var(--color-primary);
                  }
                }

                .cyber-unread-badge {
                  min-width: 18px;
                  height: 18px;
                  box-sizing: border-box;
                  padding: 0 4px;
                  border-radius: 9px;
                  background: #ef4444;
                  color: #ffffff;
                  font-size: 10px;
                  line-height: 18px;
                  text-align: center;
                }
              }

              .cyber-device-preview {
                overflow: hidden;
                color: var(--color-text-muted);
                text-overflow: ellipsis;
                white-space: nowrap;
                font-size: 11.5px;
                line-height: 1.35;
              }

              .cyber-device-telemetry {
                display: flex;
                align-items: center;
                gap: 6px;
                font-family:
                  ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
                  monospace;
                font-size: 10.5px;

                .telemetry-ip {
                  color: var(--color-text-soft);
                }

                .telemetry-pairing-action {
                  color: var(--color-primary);
                  text-decoration: underline;
                }
              }
            }
          }

          .cyber-device-delete-btn {
            display: grid;
            width: 24px;
            height: 24px;
            place-items: center;
            border: 0;
            border-radius: 4px;
            background: transparent;
            color: var(--color-text-soft);
            cursor: pointer;
            opacity: 0;
            transition: all 0.15s;

            &:hover:not(:disabled) {
              color: var(--color-danger);
              background: var(--color-danger-soft);
            }
          }

          &:hover .cyber-device-delete-btn {
            opacity: 1;
          }
        }

        .cyber-sidebar-empty {
          display: flex;
          align-items: center;
          flex-direction: column;
          gap: 12px;
          padding: 36px 16px;
          text-align: center;

          .cyber-radar-hud {
            position: relative;
            width: 70px;
            height: 70px;
            display: grid;
            place-items: center;

            .radar-circle {
              position: absolute;
              border-radius: 50%;
              border: 1px solid var(--color-line);

              &--1 {
                width: 46px;
                height: 46px;
                border-style: dashed;
                border-color: var(--color-primary);
                opacity: 0.55;
                animation: cyberSpin 14s linear infinite;
              }

              &--2 {
                width: 68px;
                height: 68px;
                border-color: var(--color-line-strong);
              }
            }

            .radar-sweep-hand {
              position: absolute;
              top: 0;
              left: 50%;
              width: 50%;
              height: 50%;
              transform-origin: 0% 100%;
              background: linear-gradient(
                135deg,
                var(--color-primary),
                transparent 70%
              );
              opacity: 0.25;
              border-radius: 0 100% 0 0;
              animation: cyberSweep 3.5s linear infinite;
              pointer-events: none;
            }

            .radar-center-icon {
              color: var(--color-primary);
            }
          }

          .cyber-empty-title {
            font-family:
              ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
            font-size: 11px;
            letter-spacing: 1.5px;
            color: var(--color-primary);
            font-weight: 600;
          }

          .cyber-empty-hint {
            font-size: 11.5px;
            color: var(--color-text-muted);
            line-height: 1.6;
          }
        }
      }

      .cyber-sidebar-telemetry {
        flex: none;
        padding: 10px 14px;
        border-top: 1px solid var(--color-line);
        background: var(--color-panel);
        display: flex;
        flex-direction: column;
        gap: 5px;

        .telemetry-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 10px;

          .telemetry-label {
            color: var(--color-text-soft);
            font-weight: 500;
          }

          .telemetry-val {
            color: var(--color-text);
            display: inline-flex;
            align-items: center;
            gap: 5px;

            .telemetry-icon {
              color: var(--color-text-muted);
              flex-shrink: 0;
            }
          }
        }
      }
    }

    .cyber-workspace-empty {
      flex: 1;
      display: flex;
      align-items: center;
      justify-content: center;
      border: 1px solid var(--color-line);
      border-radius: 10px;
      background: var(--color-panel);
      box-shadow: var(--shadow-panel);
      padding: 32px;

      .cyber-workspace-empty-hud {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 16px;
        max-width: 360px;
        text-align: center;

        .cyber-radar-hud {
          position: relative;
          width: 80px;
          height: 80px;
          display: grid;
          place-items: center;

          .radar-circle {
            position: absolute;
            border-radius: 50%;
            border: 1px solid var(--color-line);

            &--1 {
              width: 54px;
              height: 54px;
              border-style: dashed;
              border-color: var(--color-primary);
              opacity: 0.55;
              animation: cyberSpin 14s linear infinite;
            }

            &--2 {
              width: 80px;
              height: 80px;
              border-color: var(--color-line-strong);
            }
          }

          .radar-center-icon {
            color: var(--color-primary);
            z-index: 1;
          }
        }

        .cyber-empty-title {
          font-family: var(--font-mono, monospace);
          font-size: 14px;
          font-weight: 700;
          letter-spacing: 0.08em;
          color: var(--color-text);
        }

        .cyber-empty-hint {
          margin: 0;
          font-size: 13px;
          color: var(--color-text-muted);
          line-height: 1.6;
        }
      }
    }
  }
}

/* 弹窗通用样式 */
.drop-settings-modal.base-modal {
  :deep(.base-modal__panel) {
    width: 460px;
    max-width: calc(100vw - 48px);
  }

  .drop-settings-content {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-top: 4px;
    color: var(--color-text);

    .drop-settings-status {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 11px 12px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel-soft);

      .drop-settings-status-dot {
        width: 8px;
        height: 8px;
        flex: none;
        border-radius: 50%;
        background: var(--color-text-soft);
      }

      &.is-online {
        border-color: var(--color-success-line);
        background: var(--color-success-soft);

        .drop-settings-status-dot {
          background: var(--color-success);
          box-shadow: 0 0 0 3px var(--color-success-soft);
        }
      }

      .drop-settings-status-copy {
        display: flex;
        min-width: 0;
        flex-direction: column;
        gap: 3px;

        span {
          font-size: 13px;
        }
        small {
          color: var(--color-text-muted);
          font-size: 11.5px;
        }
      }
    }

    .drop-settings-list {
      display: flex;
      flex-direction: column;
      gap: 7px;
    }

    .drop-settings-item {
      display: flex;
      width: 100%;
      align-items: center;
      gap: 10px;
      padding: 10px 11px;
      border: 1px solid var(--color-line);
      border-radius: 8px;
      background: var(--color-panel);
      color: var(--color-text);
      text-align: left;
      cursor: pointer;

      &:hover:not(:disabled) {
        border-color: var(--color-primary);
        background: var(--color-primary-soft);
      }

      &:disabled {
        cursor: wait;
        opacity: 0.6;
      }

      .drop-settings-item-icon {
        display: grid;
        width: 30px;
        height: 30px;
        flex: none;
        place-items: center;
        border-radius: 7px;
        background: var(--color-primary-soft);
        color: var(--color-primary);
      }

      .drop-settings-item-copy {
        display: flex;
        min-width: 0;
        flex: 1;
        flex-direction: column;
        gap: 3px;

        span {
          font-size: 12.5px;
        }
        small {
          overflow: hidden;
          color: var(--color-text-muted);
          font-size: 11px;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }

      .drop-settings-item-arrow {
        flex: none;
        color: var(--color-text-soft);
      }
    }

    .drop-settings-addresses {
      display: flex;
      flex-direction: column;
      gap: 8px;
      padding-top: 2px;

      .drop-settings-section-title {
        display: flex;
        align-items: center;
        gap: 6px;
        color: var(--color-text-muted);
        font-size: 11.5px;
      }

      .drop-settings-address-list {
        display: flex;
        flex-wrap: wrap;
        gap: 7px;

        .drop-settings-address {
          display: inline-flex;
          align-items: center;
          gap: 8px;
          padding: 7px 9px;
          border: 1px solid var(--color-line);
          border-radius: 6px;
          background: var(--color-panel-soft);
          color: var(--color-text);
          font-family:
            ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
          font-size: 11px;
          cursor: pointer;

          span:last-child {
            color: var(--color-text-muted);
            font-family: inherit;
          }
          &:hover:not(:disabled),
          &.is-active {
            border-color: var(--color-primary);
            background: var(--color-primary-soft);
            color: var(--color-primary);
          }
          &:disabled {
            cursor: default;
            opacity: 0.72;
          }
        }

        .drop-settings-empty {
          color: var(--color-text-soft);
          font-size: 11.5px;
        }
      }
    }

    .drop-settings-danger-card {
      display: flex;
      flex-direction: column;
      gap: 8px;
      padding: 12px 14px;
      border: 1px dashed rgba(239, 68, 68, 0.35);
      border-radius: 8px;
      background: var(--color-danger-soft, rgba(239, 68, 68, 0.05));

      .drop-settings-danger-header {
        display: flex;
        align-items: center;
        gap: 6px;
        color: var(--color-danger, #ef4444);
        font-size: 13px;
        font-weight: 600;
      }

      .drop-settings-danger-desc {
        margin: 0;
        color: var(--color-text-muted);
        font-size: 11.5px;
        line-height: 1.5;
      }

      .drop-settings-danger-btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 6px;
        min-height: 32px;
        padding: 0 12px;
        border: 1px solid var(--color-danger-line, rgba(239, 68, 68, 0.4));
        border-radius: 6px;
        background: var(--color-panel);
        color: var(--color-danger, #ef4444);
        font-size: 12px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.2s;

        &:hover:not(:disabled) {
          background: var(--color-danger, #ef4444);
          color: #ffffff;
        }

        &:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
      }
    }

    .drop-settings-offline {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 7px;
      min-height: 34px;
      border: 1px solid var(--color-line);
      border-radius: 7px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      cursor: pointer;

      &:hover {
        border-color: var(--color-danger-line, var(--color-line-strong));
        color: var(--color-danger);
      }
    }
  }
}

.drop-delete-device-modal {
  .drop-delete-warning {
    color: var(--color-danger);
    font-size: var(--font-size-xs);
    line-height: 1.6;
  }
}

.drop-save-success-modal {
  :deep(.base-modal__panel) {
    width: min(520px, calc(100vw - 48px));
    border-radius: 18px;
    border: 1px solid var(--color-line);
    background: var(--color-panel);
    box-shadow:
      0 24px 64px -12px rgba(15, 23, 42, 0.2),
      0 0 0 1px rgba(0, 0, 0, 0.04);
    overflow: hidden;
  }

  :deep(.base-modal__header) {
    position: relative;
    padding: 16px 20px 0;
    border-bottom: none;
    display: flex;
    justify-content: flex-end;
  }

  :deep(.base-modal__content) {
    padding: 0 32px 32px;
  }

  .drop-save-success-close {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;

    &:hover {
      background: var(--color-panel-soft);
      border-color: var(--color-line-strong);
      color: var(--color-text);
    }

    &:focus-visible {
      outline: 2px solid var(--color-primary);
      outline-offset: 2px;
    }
  }

  .drop-save-success-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;

    .drop-save-success-illustration {
      width: 96px;
      height: 96px;
      display: grid;
      place-items: center;
      margin-top: -6px;

      .drop-save-success-badge-svg {
        width: 100%;
        height: 100%;
        overflow: visible;
      }
    }

    .drop-save-success-title {
      margin: 14px 0 0;
      font-size: 20px;
      font-weight: 700;
      color: var(--color-text);
      line-height: 1.3;
      letter-spacing: -0.01em;
    }

    .drop-save-success-desc {
      margin: 8px 0 0;
      font-size: 14px;
      color: var(--color-text-muted);
      line-height: 1.5;
      max-width: 440px;
      word-break: break-all;

      .drop-save-success-filename {
        color: var(--color-text);
        font-weight: 500;
      }
    }

    .drop-save-success-path-card {
      width: 100%;
      box-sizing: border-box;
      display: flex;
      align-items: center;
      gap: 10px;
      margin-top: 22px;
      padding: 9px 12px 9px 14px;
      border-radius: 10px;
      border: 1px solid var(--color-line);
      background: #f8faff;
      transition: all 0.15s ease;

      &:hover {
        border-color: #93c5fd;
      }

      .drop-save-success-path-icon {
        display: grid;
        place-items: center;
        color: #2563eb;
        flex-shrink: 0;
      }

      .drop-save-success-path-text {
        flex: 1;
        min-width: 0;
        text-align: left;
        font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
        font-size: 13px;
        color: var(--color-text);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        user-select: all;
        cursor: pointer;
      }

      .drop-save-success-copy-btn {
        display: grid;
        place-items: center;
        width: 32px;
        height: 32px;
        border-radius: 7px;
        border: 1px solid var(--color-line);
        background: var(--color-panel);
        color: var(--color-text-muted);
        cursor: pointer;
        flex-shrink: 0;
        transition: all 0.15s ease;

        &:hover {
          background: var(--color-panel-soft);
          border-color: var(--color-line-strong);
          color: var(--color-text);
        }

        &.is-copied {
          border-color: #10b981;
          background: #ecfdf5;
          color: #059669;
        }

        &:focus-visible {
          outline: 2px solid var(--color-primary);
          outline-offset: 2px;
        }
      }
    }

    .drop-save-success-actions {
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 12px;
      margin-top: 24px;
      width: 100%;

      .drop-save-btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 7px;
        height: 40px;
        border-radius: 8px;
        font-size: 14px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s cubic-bezier(0.16, 1, 0.3, 1);

        &--cancel {
          min-width: 104px;
          padding: 0 20px;
          border: 1px solid var(--color-line);
          background: var(--color-panel);
          color: var(--color-text);

          &:hover {
            background: var(--color-panel-soft);
            border-color: var(--color-line-strong);
          }
        }

        &--primary {
          min-width: 144px;
          padding: 0 24px;
          border: none;
          background: #1677ff;
          color: #ffffff;
          box-shadow: 0 2px 8px rgba(22, 119, 255, 0.28);

          &:hover {
            filter: brightness(1.08);
            box-shadow: 0 4px 14px rgba(22, 119, 255, 0.36);
            transform: translateY(-1px);
          }

          &:active {
            transform: translateY(0);
            filter: brightness(0.96);
          }
        }

        &:focus-visible {
          outline: 2px solid var(--color-primary);
          outline-offset: 2px;
        }
      }
    }
  }
}

.drop-group-modal {
  :deep(.base-modal__panel) {
    width: min(580px, calc(100vw - 48px));
    max-height: min(720px, calc(100vh - 48px));
    border-radius: 16px;
    border: 1px solid var(--color-line);
    background: var(--color-panel);
    box-shadow:
      0 24px 60px -12px rgba(15, 23, 42, 0.18),
      0 0 0 1px rgba(0, 0, 0, 0.04);
    overflow: hidden;
  }

  :deep(.base-modal__header) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 22px 26px 16px;
    border-bottom: 1px solid var(--color-line);
  }

  :deep(.base-modal__content) {
    padding: 20px 26px 24px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .drop-group-header {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;

    &-icon {
      display: grid;
      place-items: center;
      width: 42px;
      height: 42px;
      border-radius: 10px;
      background: #eff6ff;
      color: #2563eb;
      flex-shrink: 0;
    }

    &-text {
      display: flex;
      flex-direction: column;
      gap: 3px;
      min-width: 0;
    }

    .drop-group-title {
      margin: 0;
      font-size: 18px;
      font-weight: 700;
      color: var(--color-text);
      letter-spacing: -0.01em;
      line-height: 1.3;
    }

    .drop-group-description {
      margin: 0;
      color: var(--color-text-muted);
      font-size: 13px;
      line-height: 1.4;
    }
  }

  .drop-group-close {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;

    &:hover {
      background: var(--color-panel-soft);
      border-color: var(--color-line-strong);
      color: var(--color-text);
    }

    &:focus-visible {
      outline: 2px solid var(--color-primary);
      outline-offset: 2px;
    }
  }

  .drop-group-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    color: var(--color-text);
  }

  .drop-group-field {
    display: flex;
    flex-direction: column;
    gap: 8px;

    &-label {
      font-size: 13.5px;
      font-weight: 600;
      color: var(--color-text);
      line-height: 1.3;
    }

    .drop-group-name-input {
      :deep(.el-input__wrapper) {
        border-radius: 9px;
        padding: 4px 12px;
        background: var(--color-panel);
        box-shadow: 0 0 0 1px var(--color-line) inset;
        transition: all 0.15s ease;

        &:hover {
          box-shadow: 0 0 0 1px var(--color-line-strong) inset;
        }

        &.is-focus {
          box-shadow:
            0 0 0 1.5px var(--color-primary) inset,
            0 0 0 4px rgba(22, 119, 255, 0.1);
        }
      }

      :deep(.el-input__inner) {
        height: 38px;
        font-size: 14px;
        color: var(--color-text);

        &::placeholder {
          color: var(--color-text-soft);
        }
      }
    }
  }

  .drop-group-notice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 11px 14px;
    border-radius: 10px;
    background: #eff6ff;
    border: 1px solid #bfdbfe;

    &-icon {
      color: #2563eb;
      flex-shrink: 0;
      margin-top: 1px;
    }

    &-text {
      font-size: 12.5px;
      line-height: 1.55;
      color: #1e40af;
    }
  }

  .drop-group-section {
    display: flex;
    flex-direction: column;
    gap: 10px;

    &-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
    }

    &-title {
      font-size: 13.5px;
      font-weight: 600;
      color: var(--color-text);
    }

    .drop-group-count-badge {
      display: inline-flex;
      align-items: center;
      padding: 2.5px 9px;
      border-radius: 9999px;
      font-size: 12px;
      font-weight: 600;
      background: #eff6ff;
      color: #2563eb;
      border: 1px solid #bfdbfe;
    }
  }

  .drop-group-devices {
    display: flex;
    flex-direction: column;
    gap: 9px;
    max-height: 240px;
    overflow-y: auto;
    padding-right: 4px;

    &::-webkit-scrollbar {
      width: 6px;
    }
    &::-webkit-scrollbar-track {
      background: transparent;
    }
    &::-webkit-scrollbar-thumb {
      background: rgba(148, 163, 184, 0.4);
      border-radius: 9999px;

      &:hover {
        background: rgba(100, 116, 139, 0.6);
      }
    }

    .drop-group-device-card {
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 11px 14px;
      border-radius: 10px;
      border: 1px solid var(--color-line);
      background: var(--color-panel);
      cursor: pointer;
      user-select: none;
      transition: all 0.18s cubic-bezier(0.16, 1, 0.3, 1);

      &:hover:not(.is-disabled) {
        border-color: #93c5fd;
        background: var(--color-panel-soft);
      }

      &.is-selected {
        border-color: #2563eb;
        background: #f8faff;
        box-shadow: 0 0 0 1px #2563eb inset;

        .drop-group-device-avatar {
          background: #eff6ff;
          color: #2563eb;
        }
      }

      &.is-disabled {
        opacity: 0.62;
        cursor: not-allowed;
        background: var(--color-panel-soft);
      }

      &:focus-visible {
        outline: 2px solid var(--color-primary);
        outline-offset: 2px;
      }
    }

    .drop-group-checkbox {
      display: grid;
      place-items: center;
      width: 18px;
      height: 18px;
      flex-shrink: 0;
      border: 1.5px solid var(--color-line-strong);
      border-radius: 5px;
      background: var(--color-panel);
      color: #ffffff;
      transition: all 0.15s ease;

      &.is-checked {
        border-color: #2563eb;
        background: #2563eb;
      }

      &.is-disabled {
        border-color: var(--color-line);
        background: rgba(148, 163, 184, 0.15);
      }
    }

    .drop-group-device-avatar {
      display: grid;
      place-items: center;
      width: 36px;
      height: 36px;
      border-radius: 8px;
      background: var(--color-panel-soft);
      color: var(--color-text-muted);
      flex-shrink: 0;
      transition: all 0.18s ease;
    }

    .drop-group-device-info {
      display: flex;
      flex-direction: column;
      gap: 3px;
      min-width: 0;
      flex: 1;

      .drop-group-device-name {
        font-size: 13.5px;
        font-weight: 600;
        color: var(--color-text);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .drop-group-device-ip {
        font-size: 12px;
        color: var(--color-text-muted);
        font-variant-numeric: tabular-nums;
      }
    }

    .drop-group-device-status {
      flex-shrink: 0;

      .drop-group-status-badge {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 3px 9px;
        border-radius: 6px;
        font-size: 11.5px;
        font-weight: 500;
        white-space: nowrap;

        .status-dot {
          font-size: 8px;
          line-height: 1;
        }

        &--online {
          background: #ecfdf5;
          color: #059669;
          border: 1px solid #a7f3d0;

          .status-dot {
            color: #10b981;
          }
        }

        &--offline {
          background: var(--color-panel-soft);
          color: var(--color-text-muted);
          border: 1px solid var(--color-line);

          .status-dot {
            color: var(--color-text-soft);
          }
        }

        &--disabled {
          background: #fef3c7;
          color: #d97706;
          border: 1px solid #fde68a;
        }
      }
    }

    .drop-group-empty {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 32px 16px;
      border: 1px dashed var(--color-line);
      border-radius: 10px;
      background: var(--color-panel-soft);
      text-align: center;

      &-icon {
        color: var(--color-text-soft);
        margin-bottom: 8px;
      }

      &-text {
        font-size: 13.5px;
        font-weight: 600;
        color: var(--color-text);
        margin: 0 0 4px;
      }

      &-subtext {
        font-size: 12px;
        color: var(--color-text-muted);
        margin: 0;
        max-width: 320px;
        line-height: 1.5;
      }
    }
  }

  .drop-group-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 4px;
    padding-top: 16px;
    border-top: 1px solid var(--color-line);

    .drop-group-btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 6px;
      height: 36px;
      padding: 0 18px;
      border-radius: 8px;
      font-size: 13.5px;
      font-weight: 500;
      cursor: pointer;
      transition: all 0.15s ease;

      &--cancel {
        border: 1px solid var(--color-line);
        background: var(--color-panel);
        color: var(--color-text);

        &:hover:not(:disabled) {
          background: var(--color-panel-soft);
          border-color: var(--color-line-strong);
        }
      }

      &--primary {
        border: none;
        background: #1677ff;
        color: #ffffff;
        box-shadow: 0 2px 6px rgba(22, 119, 255, 0.25);

        &:hover:not(:disabled) {
          background: #4096ff;
          box-shadow: 0 4px 12px rgba(22, 119, 255, 0.35);
        }

        &:disabled {
          opacity: 0.55;
          cursor: not-allowed;
          box-shadow: none;
        }
      }

      &:disabled {
        opacity: 0.55;
        cursor: not-allowed;
      }
    }
  }
}

.drop-connect-modal {
  :deep(.base-modal__panel) {
    width: min(650px, calc(100vw - 48px));
    max-height: min(720px, calc(100vh - 48px));
    border-radius: 16px;
    border: 1px solid var(--color-line);
    background: var(--color-panel);
    box-shadow:
      0 24px 60px -12px rgba(15, 23, 42, 0.18),
      0 0 0 1px rgba(0, 0, 0, 0.04);
    overflow: hidden;
  }

  :deep(.base-modal__header) {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 26px 28px 12px;
    border-bottom: none;
  }

  :deep(.base-modal__content) {
    padding: 0 28px 26px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .drop-connect-header {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;

    .drop-connect-title {
      margin: 0;
      font-size: 20px;
      font-weight: 700;
      color: var(--color-text);
      letter-spacing: -0.01em;
      line-height: 1.3;
    }

    .drop-connect-description {
      margin: 0;
      color: var(--color-text-muted);
      font-size: 13.5px;
      line-height: 1.5;
    }
  }

  .drop-connect-close {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    border: 1px solid var(--color-line);
    border-radius: 8px;
    background: var(--color-panel);
    color: var(--color-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;

    &:hover {
      background: var(--color-panel-soft);
      border-color: var(--color-line-strong);
      color: var(--color-text);
    }

    &:focus-visible {
      outline: 2px solid var(--color-primary);
      outline-offset: 2px;
    }
  }

  .drop-connect-content {
    display: flex;
    flex-direction: column;
    min-height: 0;

    .drop-connect-filter-box {
      margin-top: 4px;

      :deep(.el-input) {
        .el-input__wrapper {
          border-radius: 9px;
          padding: 3px 12px;
          background: var(--color-panel);
          box-shadow: 0 0 0 1px var(--color-line) inset;
          transition: all 0.15s ease;

          &:hover {
            box-shadow: 0 0 0 1px var(--color-line-strong) inset;
          }

          &.is-focus {
            box-shadow:
              0 0 0 1.5px var(--color-primary) inset,
              0 0 0 4px rgba(22, 119, 255, 0.1);
          }
        }

        .el-input__inner {
          height: 36px;
          font-size: 13.5px;
          color: var(--color-text);

          &::placeholder {
            color: var(--color-text-soft);
          }
        }
      }

      .drop-connect-filter-icon {
        color: var(--color-text-soft);
        margin-right: 6px;
        flex-shrink: 0;
      }
    }

    .drop-connect-list {
      display: flex;
      flex-direction: column;
      gap: 10px;
      margin: 14px 0 10px;
      max-height: 320px;
      overflow-y: auto;
      padding-right: 2px;

      &::-webkit-scrollbar {
        width: 6px;
      }
      &::-webkit-scrollbar-track {
        background: transparent;
      }
      &::-webkit-scrollbar-thumb {
        background: rgba(148, 163, 184, 0.4);
        border-radius: 9999px;

        &:hover {
          background: rgba(100, 116, 139, 0.6);
        }
      }

      .drop-connect-card {
        display: flex;
        align-items: center;
        gap: 14px;
        padding: 14px 18px;
        border: 1px solid var(--color-line);
        border-radius: 12px;
        background: var(--color-panel);
        transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
        cursor: pointer;

        &.is-ready {
          border-color: #93c5fd;
          background: #f8faff;
        }

        &:hover {
          border-color: var(--color-primary);
          background: var(--color-primary-soft);
          box-shadow: 0 4px 14px rgba(22, 119, 255, 0.08);

          .drop-connect-avatar {
            background: #eff6ff;
            color: #2563eb;
          }

          .drop-connect-btn--waiting {
            background: #1677ff;
            color: #ffffff;
            border-color: #1677ff;
          }
        }

        .drop-connect-avatar {
          display: grid;
          place-items: center;
          width: 44px;
          height: 44px;
          border-radius: 50%;
          background: var(--color-panel-soft);
          color: var(--color-text-muted);
          flex-shrink: 0;
          transition: all 0.2s ease;
        }

        &.is-ready .drop-connect-avatar {
          background: #eff6ff;
          color: #2563eb;
        }

        .drop-connect-info {
          display: flex;
          flex-direction: column;
          gap: 4px;
          min-width: 0;
          flex: 1;

          .drop-connect-name-row {
            display: flex;
            align-items: center;
            gap: 10px;
            flex-wrap: wrap;

            .drop-connect-name {
              font-size: 14.5px;
              font-weight: 600;
              color: var(--color-text);
              overflow: hidden;
              text-overflow: ellipsis;
              white-space: nowrap;
            }

            .drop-connect-badge {
              display: inline-flex;
              align-items: center;
              gap: 5px;
              padding: 2px 9px;
              border-radius: 9999px;
              font-size: 11.5px;
              font-weight: 500;
              line-height: 1.35;

              .badge-dot {
                font-size: 8px;
                line-height: 1;
              }

              &--ready {
                background: #ecfdf5;
                color: #059669;

                .badge-dot {
                  color: #10b981;
                }
              }

              &--waiting {
                background: #fff7ed;
                color: #d97706;

                .badge-dot {
                  color: #f59e0b;
                }
              }
            }
          }

          .drop-connect-ip {
            font-size: 13px;
            color: var(--color-text-muted);
            font-variant-numeric: tabular-nums;
          }
        }

        .drop-connect-btn {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          gap: 6px;
          min-width: 82px;
          height: 35px;
          padding: 0 16px;
          border-radius: 8px;
          font-size: 13.5px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.15s ease;
          flex-shrink: 0;

          &--primary {
            border: 1px solid #1677ff;
            background: #1677ff;
            color: #ffffff;
            box-shadow: 0 2px 6px rgba(22, 119, 255, 0.25);

            &:hover:not(:disabled) {
              background: #4096ff;
              border-color: #4096ff;
              box-shadow: 0 4px 10px rgba(22, 119, 255, 0.35);
            }
          }

          &--waiting {
            border: 1px solid rgba(22, 119, 255, 0.5);
            background: var(--color-panel);
            color: #1677ff;

            &:hover:not(:disabled) {
              background: rgba(22, 119, 255, 0.08);
              border-color: #1677ff;
            }
          }

          &:disabled {
            opacity: 0.65;
            cursor: not-allowed;
            box-shadow: none;
          }
        }
      }

      .drop-connect-empty {
        padding: 32px 16px;
        color: var(--color-text-muted);
        text-align: center;
        font-size: 13.5px;
      }
    }

    .drop-connect-hint-row {
      display: flex;
      align-items: center;
      gap: 7px;
      margin-top: 10px;

      .drop-connect-hint-icon {
        color: var(--color-text-soft);
        flex-shrink: 0;
      }

      .drop-connect-hint {
        color: var(--color-text-soft);
        font-size: 12px;
        line-height: 1.45;
      }
    }

    .drop-connect-actions {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      gap: 12px;
      margin-top: 18px;

      .drop-connect-action-close {
        min-width: 78px;
        height: 36px;
        padding: 0 18px;
        border-radius: 8px;
        border: 1px solid var(--color-line);
        background: var(--color-panel);
        color: var(--color-text);
        font-size: 13.5px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;

        &:hover {
          background: var(--color-panel-soft);
          border-color: var(--color-line-strong);
        }
      }

      .drop-connect-action-scan {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 6px;
        min-width: 96px;
        height: 36px;
        padding: 0 18px;
        border-radius: 8px;
        border: none;
        background: #1677ff;
        color: #ffffff;
        font-size: 13.5px;
        font-weight: 500;
        cursor: pointer;
        box-shadow: 0 2px 6px rgba(22, 119, 255, 0.25);
        transition: all 0.15s ease;

        &:hover:not(:disabled) {
          background: #4096ff;
          box-shadow: 0 4px 12px rgba(22, 119, 255, 0.35);
        }

        &:disabled {
          opacity: 0.6;
          cursor: not-allowed;
          box-shadow: none;
        }

        .is-spinning {
          animation: dropSpin 1s linear infinite;
        }
      }
    }
  }
}

@keyframes dropSpin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

/* 右侧浮动通知弹框 */
.drop-right-toast {
  position: fixed;
  top: 28px;
  right: 28px;
  z-index: 9999;
  display: flex;
  align-items: flex-start;
  gap: 12px;
  min-width: 320px;
  max-width: 440px;
  padding: 14px 16px;
  border-radius: 12px;
  border: 1px solid rgba(239, 68, 68, 0.3);
  background: var(--color-panel);
  box-shadow:
    0 14px 36px -4px rgba(15, 23, 42, 0.16),
    0 4px 12px -2px rgba(239, 68, 68, 0.1),
    0 0 0 1px rgba(0, 0, 0, 0.04);
  backdrop-filter: blur(10px);
  overflow: hidden;

  &__icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: #fef2f2;
    color: #ef4444;
    flex-shrink: 0;
  }

  &__body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  &__title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--color-text);
    line-height: 1.4;
  }

  &__text {
    margin: 0;
    font-size: 13px;
    color: var(--color-text-muted);
    line-height: 1.5;
    word-break: break-word;
  }

  &__close {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin: -2px -4px 0 0;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-soft);
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;

    &:hover {
      background: var(--color-panel-soft);
      color: var(--color-text);
    }
  }

  &__bar {
    position: absolute;
    bottom: 0;
    left: 0;
    height: 2.5px;
    background: #ef4444;
    border-radius: 0 0 0 12px;
    animation: toastProgress 3.5s linear forwards;
  }
}

@keyframes toastProgress {
  from {
    width: 100%;
  }
  to {
    width: 0%;
  }
}

.drop-toast-slide-enter-active,
.drop-toast-slide-leave-active {
  transition: all 0.28s cubic-bezier(0.16, 1, 0.3, 1);
}

.drop-toast-slide-enter-from {
  opacity: 0;
  transform: translateX(40px) scale(0.96);
}

.drop-toast-slide-leave-to {
  opacity: 0;
  transform: translateX(30px) scale(0.98);
}

:root[data-theme="dark"] {
  .drop-save-success-modal {
    .drop-save-success-close {
      background: rgba(255, 255, 255, 0.05);
      border-color: var(--color-line);

      &:hover {
        background: rgba(255, 255, 255, 0.1);
        border-color: var(--color-line-strong);
      }
    }

    .drop-save-success-path-card {
      background: rgba(30, 41, 59, 0.6);
      border-color: rgba(255, 255, 255, 0.08);

      &:hover {
        border-color: rgba(59, 130, 246, 0.4);
      }

      .drop-save-success-path-icon {
        color: #60a5fa;
      }

      .drop-save-success-copy-btn {
        background: rgba(255, 255, 255, 0.06);
        border-color: rgba(255, 255, 255, 0.1);

        &:hover {
          background: rgba(255, 255, 255, 0.12);
          border-color: rgba(255, 255, 255, 0.2);
        }

        &.is-copied {
          border-color: rgba(16, 185, 129, 0.5);
          background: rgba(16, 185, 129, 0.2);
          color: #34d399;
        }
      }
    }

    .drop-save-btn--cancel {
      background: rgba(255, 255, 255, 0.06);
      border-color: rgba(255, 255, 255, 0.12);
      color: #e2e8f0;

      &:hover {
        background: rgba(255, 255, 255, 0.1);
        border-color: rgba(255, 255, 255, 0.2);
      }
    }
  }

  .drop-group-modal {
    .drop-group-header-icon {
      background: rgba(37, 99, 235, 0.2);
      color: #60a5fa;
    }

    .drop-group-close {
      background: rgba(255, 255, 255, 0.05);
      border-color: var(--color-line);

      &:hover {
        background: rgba(255, 255, 255, 0.1);
        border-color: var(--color-line-strong);
      }
    }

    .drop-group-notice {
      background: rgba(37, 99, 235, 0.12);
      border-color: rgba(59, 130, 246, 0.3);

      &-icon {
        color: #60a5fa;
      }

      &-text {
        color: #93c5fd;
      }
    }

    .drop-group-section .drop-group-count-badge {
      background: rgba(37, 99, 235, 0.18);
      color: #60a5fa;
      border-color: rgba(59, 130, 246, 0.35);
    }

    .drop-group-devices {
      .drop-group-device-card {
        background: rgba(30, 41, 59, 0.5);
        border-color: rgba(255, 255, 255, 0.08);

        &:hover:not(.is-disabled) {
          border-color: rgba(59, 130, 246, 0.4);
          background: rgba(37, 99, 235, 0.08);
        }

        &.is-selected {
          border-color: #3b82f6;
          background: rgba(37, 99, 235, 0.14);
          box-shadow: 0 0 0 1px #3b82f6 inset;

          .drop-group-device-avatar {
            background: rgba(37, 99, 235, 0.25);
            color: #60a5fa;
          }
        }

        &.is-disabled {
          background: rgba(15, 23, 42, 0.4);
          border-color: rgba(255, 255, 255, 0.04);
        }
      }

      .drop-group-checkbox {
        background: rgba(30, 41, 59, 0.7);
        border-color: rgba(255, 255, 255, 0.2);

        &.is-checked {
          background: #2563eb;
          border-color: #2563eb;
        }

        &.is-disabled {
          background: rgba(255, 255, 255, 0.05);
          border-color: rgba(255, 255, 255, 0.1);
        }
      }

      .drop-group-device-avatar {
        background: rgba(255, 255, 255, 0.06);
        color: #94a3b8;
      }

      .drop-group-device-status .drop-group-status-badge {
        &--online {
          background: rgba(16, 185, 129, 0.18);
          color: #34d399;
          border-color: rgba(16, 185, 129, 0.3);
        }

        &--disabled {
          background: rgba(245, 158, 11, 0.18);
          color: #fbbf24;
          border-color: rgba(245, 158, 11, 0.3);
        }
      }

      .drop-group-empty {
        background: rgba(15, 23, 42, 0.4);
        border-color: rgba(255, 255, 255, 0.1);
      }
    }

    .drop-group-actions .drop-group-btn--cancel {
      background: rgba(255, 255, 255, 0.06);
      border-color: rgba(255, 255, 255, 0.12);
      color: #e2e8f0;

      &:hover:not(:disabled) {
        background: rgba(255, 255, 255, 0.1);
        border-color: rgba(255, 255, 255, 0.2);
      }
    }
  }

  .drop-connect-modal {
    .drop-connect-close {
      background: rgba(255, 255, 255, 0.05);
      border-color: var(--color-line);

      &:hover {
        background: rgba(255, 255, 255, 0.1);
        border-color: var(--color-line-strong);
      }
    }

    .drop-connect-content {
      .drop-connect-list {
        .drop-connect-card {
          background: rgba(30, 41, 59, 0.5);
          border-color: rgba(255, 255, 255, 0.08);

          &.is-ready {
            border-color: rgba(59, 130, 246, 0.4);
            background: rgba(37, 99, 235, 0.1);
          }

          &:hover {
            border-color: #3b82f6;
            background: rgba(37, 99, 235, 0.15);
          }

          .drop-connect-avatar {
            background: rgba(255, 255, 255, 0.06);
            color: #94a3b8;
          }

          &.is-ready .drop-connect-avatar {
            background: rgba(37, 99, 235, 0.25);
            color: #60a5fa;
          }

          .drop-connect-badge--ready {
            background: rgba(16, 185, 129, 0.18);
            color: #34d399;
          }

          .drop-connect-badge--waiting {
            background: rgba(245, 158, 11, 0.18);
            color: #fbbf24;
          }

          .drop-connect-btn--waiting {
            background: transparent;
            border-color: rgba(96, 165, 250, 0.5);
            color: #60a5fa;

            &:hover:not(:disabled) {
              background: rgba(37, 99, 235, 0.2);
              border-color: #60a5fa;
            }
          }
        }
      }

      .drop-connect-actions {
        .drop-connect-action-close {
          background: rgba(255, 255, 255, 0.06);
          border-color: rgba(255, 255, 255, 0.12);
          color: #e2e8f0;

          &:hover {
            background: rgba(255, 255, 255, 0.1);
            border-color: rgba(255, 255, 255, 0.2);
          }
        }
      }
    }
  }

  .drop-right-toast {
    background: #1e293b;
    border-color: rgba(239, 68, 68, 0.35);
    box-shadow:
      0 16px 40px -4px rgba(0, 0, 0, 0.5),
      0 0 0 1px rgba(255, 255, 255, 0.08);

    &__icon {
      background: rgba(239, 68, 68, 0.2);
      color: #f87171;
    }
  }
}

/* 动效 */
.cyber-pulse-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;

  &--emerald {
    background: var(--color-success);
    box-shadow: 0 0 6px var(--color-success);
    animation: cyberPulse 1.8s ease-in-out infinite;
  }

  &--amber {
    background: var(--color-warning);
    box-shadow: 0 0 6px var(--color-warning);
    animation: cyberPulse 1.8s ease-in-out infinite;
  }

  &--gray {
    background: var(--color-text-soft);
  }
}

@keyframes cyberSpin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

@keyframes cyberSweep {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

@keyframes cyberPulse {
  0%,
  100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.5;
    transform: scale(0.85);
  }
}

.cyber-spin {
  animation: cyberSpin 0.9s linear infinite;
}
</style>
