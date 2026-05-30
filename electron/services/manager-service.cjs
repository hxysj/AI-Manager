const fs = require("node:fs/promises")
const os = require("node:os")
const path = require("node:path")
const crypto = require("node:crypto")
const { execFile } = require("node:child_process")
const { EventEmitter } = require("node:events")
const { promisify } = require("node:util")
const {
  resolveAppPaths,
  ensureAppDirectories,
  slugifyName,
  resolvePortablePath,
  serializeAppSettingsPaths,
  serializePortablePath
} = require("./path-utils.cjs")
const { JsonStorage } = require("./json-storage.cjs")
const { CliDetectionService } = require("./cli-detection-service.cjs")
const { MetadataParser } = require("./metadata-parser.cjs")
const { SkillScanner } = require("./skill-scanner.cjs")
const { LinkManager } = require("./link-manager.cjs")
const { RepoService } = require("./repo-service.cjs")
const { FileWatcherService } = require("./file-watcher-service.cjs")
const { SessionService } = require("./session-service.cjs")
const { UsageService } = require("./usage-service.cjs")
const { SkillUsageService } = require("./skill-usage-service.cjs")
const { CodexAccountService } = require("./codex-account-service.cjs")
const { RuntimeProviderService } = require("./runtime-provider-service.cjs")
const { CodexProxyService } = require("./codex-proxy-service.cjs")
const { PromptRuntimeService } = require("./prompt-runtime-service.cjs")

const execFileAsync = promisify(execFile)
const BACKUP_SECRET = crypto
  .createHash("sha256")
  .update("ai-manager-data-backup-v1")
  .digest()

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath)
    return true
  } catch {
    return false
  }
}

function sortByName(items) {
  return [...items].sort((left, right) => left.name.localeCompare(right.name))
}

function sha256(content) {
  return crypto.createHash("sha256").update(content).digest("hex")
}

function normalizeRuleTags(value) {
  if (!Array.isArray(value)) {
    return []
  }

  return value.map((item) => String(item || "").trim()).filter(Boolean)
}

function normalizeRule(input, previous) {
  const cli = String(input.cli || previous?.cli || "").trim()
  const name = String(input.name || previous?.name || "").trim()
  const content = String(input.content || previous?.content || "")

  if (!cli) {
    throw new Error("Rule 必须选择 CLI")
  }

  if (!name) {
    throw new Error("Rule 名称不能为空")
  }

  if (!content.trim()) {
    throw new Error("Rule 内容不能为空")
  }

  return {
    id: previous?.id || input.id || `rule-${crypto.randomUUID()}`,
    cli,
    name,
    description:
      String(input.description || previous?.description || "").trim() ||
      undefined,
    category:
      String(input.category || previous?.category || "").trim() || undefined,
    tags: normalizeRuleTags(input.tags || previous?.tags),
    content,
    enabled:
      input.enabled === undefined
        ? previous?.enabled !== false
        : Boolean(input.enabled),
    createdAt: previous?.createdAt || Date.now(),
    updatedAt: Date.now()
  }
}

async function collectSkillFiles(rootPath) {
  const files = []

  const visit = async (currentPath) => {
    const entries = await fs.readdir(currentPath, { withFileTypes: true })

    for (const entry of entries.sort((left, right) =>
      left.name.localeCompare(right.name)
    )) {
      const entryPath = path.join(currentPath, entry.name)
      const stat = await fs.lstat(entryPath)

      if (stat.isSymbolicLink()) {
        continue
      }

      if (stat.isDirectory()) {
        await visit(entryPath)
        continue
      }

      if (stat.isFile()) {
        const content = await fs.readFile(entryPath)

        files.push({
          path: path.relative(rootPath, entryPath).replace(/\\/g, "/"),
          ext: path.extname(entryPath).toLowerCase(),
          hash: crypto.createHash("sha1").update(content).digest("hex")
        })
      }
    }
  }

  await visit(rootPath)
  return files
}

const skillTextFileExtensions = new Set([
  ".bat",
  ".cjs",
  ".cmd",
  ".css",
  ".csv",
  ".html",
  ".ini",
  ".js",
  ".json",
  ".jsonl",
  ".less",
  ".md",
  ".mjs",
  ".ps1",
  ".py",
  ".scss",
  ".sh",
  ".toml",
  ".ts",
  ".tsx",
  ".txt",
  ".vue",
  ".xml",
  ".yaml",
  ".yml"
])
const skillTextFileNames = new Set([".env", ".gitignore"])
const skillViewIgnoredDirs = new Set([".git", "node_modules"])
const skillPreviewMaxSize = 512 * 1024

function canPreviewSkillFile(fileName, ext, size) {
  return (
    size <= skillPreviewMaxSize &&
    (skillTextFileExtensions.has(ext) || skillTextFileNames.has(fileName))
  )
}

async function collectSkillViewEntries(rootPath) {
  const entries = []

  const visit = async (currentPath) => {
    const children = await fs.readdir(currentPath, { withFileTypes: true })

    for (const child of children.sort((left, right) =>
      left.name.localeCompare(right.name)
    )) {
      const childPath = path.join(currentPath, child.name)
      const relativePath = path
        .relative(rootPath, childPath)
        .replace(/\\/g, "/")
      const stat = await fs.lstat(childPath)

      if (stat.isSymbolicLink()) {
        entries.push({
          path: relativePath,
          name: child.name,
          type: "symlink",
          target: await fs.readlink(childPath)
        })
        continue
      }

      if (stat.isDirectory()) {
        entries.push({
          path: relativePath,
          name: child.name,
          type: "dir"
        })

        if (!skillViewIgnoredDirs.has(child.name)) {
          await visit(childPath)
        }
        continue
      }

      if (stat.isFile()) {
        const ext = path.extname(child.name).toLowerCase()
        const entry = {
          path: relativePath,
          name: child.name,
          type: "file",
          ext,
          size: stat.size,
          previewable: canPreviewSkillFile(child.name, ext, stat.size)
        }

        if (entry.previewable) {
          entry.content = await fs.readFile(childPath, "utf8")
        }

        entries.push(entry)
      }
    }
  }

  await visit(rootPath)
  return entries
}

async function createSkillSignature(skill) {
  const payload = {
    name: skill.name,
    description: skill.description || "",
    files: await collectSkillFiles(skill.sourcePath)
  }

  return crypto.createHash("sha1").update(JSON.stringify(payload)).digest("hex")
}

async function extractZip(zipPath, targetPath) {
  await execFileAsync(
    "powershell.exe",
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      "Expand-Archive -LiteralPath $args[0] -DestinationPath $args[1] -Force",
      zipPath,
      targetPath
    ],
    { windowsHide: true }
  )
}

async function collectDirectoryEntries(rootPath) {
  const entries = []

  const visit = async (currentPath) => {
    const children = await fs.readdir(currentPath, { withFileTypes: true })

    for (const child of children.sort((left, right) =>
      left.name.localeCompare(right.name)
    )) {
      const childPath = path.join(currentPath, child.name)
      const relativePath = path
        .relative(rootPath, childPath)
        .replace(/\\/g, "/")
      const stat = await fs.lstat(childPath)

      if (stat.isSymbolicLink()) {
        entries.push({
          path: relativePath,
          type: "symlink",
          target: await fs.readlink(childPath)
        })
        continue
      }

      if (stat.isDirectory()) {
        entries.push({
          path: relativePath,
          type: "dir"
        })
        await visit(childPath)
        continue
      }

      if (stat.isFile()) {
        entries.push({
          path: relativePath,
          type: "file",
          content: (await fs.readFile(childPath)).toString("base64")
        })
      }
    }
  }

  if (await pathExists(rootPath)) {
    await visit(rootPath)
  }

  return entries
}

async function collectFileEntry(sourcePath, relativePath) {
  if (!(await pathExists(sourcePath))) {
    return null
  }

  return {
    path: relativePath,
    type: "file",
    content: (await fs.readFile(sourcePath)).toString("base64")
  }
}

const ignoredRuntimeBackupPaths = new Set([
  "storage/installs.json",
  "storage/runtime-profiles.json",
  "storage/runtime-provider-state.json",
  "storage/runtime-provider-keys.json",
  "storage/codex-active-account-id.json"
])

function isIgnoredBackupPath(entryPath) {
  const normalizedPath = String(entryPath || "").toLowerCase()
  const fileName = path.basename(normalizedPath)
  return (
    ignoredRuntimeBackupPaths.has(entryPath) ||
    normalizedPath === "logs" ||
    normalizedPath.startsWith("logs/") ||
    normalizedPath.includes("/logs/") ||
    fileName.endsWith(".log") ||
    fileName.endsWith(".logs") ||
    fileName.endsWith("-logs.json") ||
    fileName.endsWith("_logs.json") ||
    fileName === "logs.json"
  )
}

const restoreStorageNames = {
  "storage/skills.json": "Skill 索引",
  "storage/installs.json": "Skill 挂载",
  "storage/usage-logs.json": "用量日志",
  "storage/usage-pricing.json": "模型费用",
  "storage/providers.json": "Provider",
  "storage/runtime-models.json": "模型",
  "storage/codex-accounts.json": "Codex 官方账号",
  "storage/codex-proxy-config.json": "Codex 代理配置",
  "storage/rules.json": "Prompt 索引",
  "storage/prompt-runtime-state.json": "Prompt Runtime 状态"
}

const mergeableRestoreJsonPaths = new Set([
  "storage/skills.json",
  "storage/installs.json",
  "storage/providers.json",
  "storage/runtime-models.json",
  "storage/runtime-profiles.json",
  "storage/runtime-provider-state.json",
  "storage/runtime-provider-keys.json",
  "storage/codex-accounts.json",
  "storage/codex-active-account-id.json",
  "storage/rules.json",
  "storage/prompt-runtime-state.json"
])

function createBackupJsonEntry(entry, value) {
  return {
    ...entry,
    content: Buffer.from(`${JSON.stringify(value, null, 2)}\n`).toString(
      "base64"
    )
  }
}

function mapBackupJsonEntry(entry, mapValue) {
  if (entry.type !== "file") {
    return entry
  }

  return createBackupJsonEntry(entry, mapValue(readBackupEntryJson(entry)))
}

function stripProviderEnabled(entry) {
  if (entry.path !== "storage/providers.json" || entry.type !== "file") {
    return entry
  }

  return mapBackupJsonEntry(entry, (providers) =>
    providers.map(({ enabled, ...provider }) => provider)
  )
}

function serializeSkillBackupPaths(entry) {
  if (entry.path !== "storage/skills.json") {
    return entry
  }

  return mapBackupJsonEntry(entry, (skills) =>
    skills.map(({ installedTargets, installStates, status, ...skill }) => ({
      ...skill,
      sourcePath: serializePortablePath(skill.sourcePath),
      entryPath: serializePortablePath(skill.entryPath)
    }))
  )
}

function serializePromptRuntimeBackupPaths(entry) {
  if (entry.path !== "storage/prompt-runtime-state.json") {
    return entry
  }

  return mapBackupJsonEntry(entry, (runtimeState) =>
    Object.fromEntries(
      Object.entries(runtimeState || {}).map(([cli, state]) => [
        cli,
        {
          ...state,
          runtimePath: serializePortablePath(state?.runtimePath)
        }
      ])
    )
  )
}

function sanitizeRuntimeBackupEntries(entries) {
  return entries
    .filter((entry) => !isIgnoredBackupPath(entry.path))
    .map((entry) =>
      serializePromptRuntimeBackupPaths(
        serializeSkillBackupPaths(stripProviderEnabled(entry))
      )
    )
}

function parseBackup(content) {
  const backup = decryptBackupPayload(String(content || ""))

  if (backup.version !== 1) {
    throw new Error("备份版本不支持")
  }

  if (!Array.isArray(backup.workspaceEntries)) {
    throw new Error("备份数据不完整")
  }

  return {
    ...backup,
    workspaceEntries: sanitizeRuntimeBackupEntries(backup.workspaceEntries)
  }
}

function readBackupEntryText(entry) {
  return Buffer.from(entry.content, "base64").toString("utf8")
}

function readBackupEntryJson(entry) {
  return JSON.parse(readBackupEntryText(entry))
}

function createBackupViewEntry(pathName, typeName, content) {
  return {
    path: pathName,
    type: "file",
    typeName,
    size: Buffer.byteLength(content, "utf8"),
    content
  }
}

function createBackupEntryView(entry) {
  if (entry.type === "dir") {
    return {
      path: entry.path,
      type: entry.type,
      typeName: "目录",
      size: 0,
      content: ""
    }
  }

  if (entry.type === "symlink") {
    return {
      path: entry.path,
      type: entry.type,
      typeName: "链接",
      size: Buffer.byteLength(entry.target || "", "utf8"),
      content: entry.target || ""
    }
  }

  const buffer = Buffer.from(entry.content, "base64")
  const text = buffer.toString("utf8")

  return {
    path: entry.path,
    type: entry.type,
    typeName: restoreStorageNames[entry.path] || "文件",
    size: buffer.length,
    content: isStorageJsonPath(entry.path)
      ? JSON.stringify(JSON.parse(text), null, 2)
      : text
  }
}

function createBackupDataView(content) {
  const backup = parseBackup(content)
  const appSettingsContent = `${JSON.stringify(
    backup.appSettings || {},
    null,
    2
  )}\n`
  const runtimeProviderKeys = backup.runtimeProviderKeys
    ? decryptBackupData(backup.runtimeProviderKeys)
    : {}
  const entries = [
    createBackupViewEntry(
      "app-settings.json",
      "应用设置",
      appSettingsContent
    ),
    ...backup.workspaceEntries.map((entry) => createBackupEntryView(entry))
  ]

  if (backup.runtimeProviderKeys) {
    entries.push(
      createBackupViewEntry(
        "runtime-provider-keys",
        "Runtime 密钥",
        `已加密保存 ${Object.keys(runtimeProviderKeys).length} 个 Provider 密钥，查看器不展开密钥明文。\n`
      )
    )
  }

  return {
    version: backup.version,
    createdAt: backup.createdAt || 0,
    entryCount: entries.length,
    fileCount: entries.filter((entry) => entry.type === "file").length,
    directoryCount: entries.filter((entry) => entry.type === "dir").length,
    entries
  }
}

function createRestoreChoiceKey(entryPath, itemKey) {
  return `json:${entryPath}:${itemKey}`
}

function createRestoreFileKey(entryPath) {
  return `file:${entryPath}`
}

function isStorageJsonPath(entryPath) {
  return entryPath.startsWith("storage/") && entryPath.endsWith(".json")
}

function isMergeableRestoreJsonPath(entryPath) {
  return mergeableRestoreJsonPaths.has(entryPath)
}

function normalizeSkillRestoreValue(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return value
  }

  const { installedTargets, installStates, status, ...nextValue } = value
  return nextValue
}

function normalizePromptRuntimeRestoreValue(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return value
  }

  const { lastSyncAt, runtimePath, ...state } = value
  return state
}

function normalizeCodexAccountRestoreValue(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return value
  }

  const { usage, ...account } = value
  return account
}

function normalizeRuntimeProviderStateRestoreValue(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return value
  }

  const { runtimeHash, ...state } = value
  return state
}

function normalizeRestoreValue(entryPath, value) {
  if (
    [
      "storage/providers.json",
      "storage/skills.json",
      "storage/runtime-models.json",
      "storage/runtime-profiles.json",
      "storage/runtime-provider-state.json",
      "storage/codex-accounts.json",
      "storage/rules.json",
      "storage/prompt-runtime-state.json"
    ].includes(entryPath) &&
    value &&
    typeof value === "object"
  )
    value = JSON.parse(
      JSON.stringify(value, (key, item) =>
        [
          "createdAt",
          "updatedAt",
          "lastUpdatedAt",
          "lastSyncAt",
          "uploadedAt",
          "downloadedAt",
          "lastBackupAt",
          "created_at",
          "updated_at",
          "last_refresh",
          "token_updated_at"
        ].includes(key)
          ? undefined
          : item
      )
    )

  if (
    entryPath === "storage/providers.json" &&
    value &&
    typeof value === "object" &&
    !Array.isArray(value)
  ) {
    const { enabled, ...provider } = value
    return JSON.stringify(provider, null, 2)
  }

  if (entryPath === "storage/skills.json") {
    return JSON.stringify(normalizeSkillRestoreValue(value), null, 2)
  }

  if (entryPath === "storage/prompt-runtime-state.json") {
    return JSON.stringify(normalizePromptRuntimeRestoreValue(value), null, 2)
  }

  if (entryPath === "storage/codex-accounts.json") {
    return JSON.stringify(normalizeCodexAccountRestoreValue(value), null, 2)
  }

  if (entryPath === "storage/runtime-provider-state.json") {
    return JSON.stringify(
      normalizeRuntimeProviderStateRestoreValue(value),
      null,
      2
    )
  }

  return JSON.stringify(value, null, 2)
}

function createRestoreContentHash(entryPath, value) {
  return sha256(normalizeRestoreValue(entryPath, value))
}

function getRestoreItemKey(entryPath, item, index) {
  if (item && typeof item === "object" && !Array.isArray(item)) {
    if (item.id) {
      return String(item.id)
    }

    if (item.providerId && item.name) {
      return `${item.providerId}:${item.name}`
    }

    return String(item.name || item.accountId || item.account_id || index)
  }

  return String(index)
}

function getRestoreItemName(entryPath, itemKey, value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return itemKey
  }

  if (entryPath === "storage/codex-accounts.json") {
    return value.email || value.accountId || value.account_id || itemKey
  }

  return value.name || value.id || itemKey
}

function getRestoreGroupPath(entryPath) {
  const normalizedPath = String(entryPath || "").replace(/\\/g, "/")
  const parts = normalizedPath.split("/").filter(Boolean)

  if (parts[0] === "skills" && parts[1]) {
    return `skills/${parts[1]}`
  }

  if (parts[0] === "prompts" && parts[1]) {
    return "prompts"
  }

  if (parts[0] === "profiles" && parts[1]) {
    return "profiles"
  }

  return parts.length > 1 ? parts.slice(0, -1).join("/") : "根目录"
}

function createRestorePreviewItem(
  entryPath,
  itemKey,
  value,
  status,
  currentValue = null
) {
  const type = restoreStorageNames[entryPath] || "配置项"

  return {
    key: createRestoreChoiceKey(entryPath, itemKey),
    type,
    name: getRestoreItemName(entryPath, itemKey, value),
    path: entryPath,
    groupPath: getRestoreGroupPath(entryPath),
    status,
    currentContent:
      status === "conflict"
        ? normalizeRestoreValue(entryPath, currentValue)
        : "",
    backupContent:
      status === "conflict" ? normalizeRestoreValue(entryPath, value) : ""
  }
}

function createRestoreFilePreviewItem(
  entryPath,
  status,
  currentContent = "",
  backupContent = ""
) {
  return {
    key: createRestoreFileKey(entryPath),
    type: entryPath.startsWith("skills/")
      ? "Skill 文件"
      : entryPath.startsWith("prompts/")
        ? "Prompt 文件"
        : entryPath.startsWith("profiles/")
          ? "Prompt 配置"
          : "文件",
    name: path.basename(entryPath),
    path: entryPath,
    groupPath: getRestoreGroupPath(entryPath),
    status,
    currentContent,
    backupContent
  }
}

function appendJsonRestorePreview(
  entryPath,
  currentValue,
  backupValue,
  preview
) {
  if (Array.isArray(backupValue)) {
    const currentItems = Array.isArray(currentValue) ? currentValue : []
    const currentMap = new Map(
      currentItems.map((item, index) => [
        getRestoreItemKey(entryPath, item, index),
        item
      ])
    )

    backupValue.forEach((item, index) => {
      const itemKey = getRestoreItemKey(entryPath, item, index)
      const currentItem = currentMap.get(itemKey)

      if (!currentItem) {
        preview.added.push(
          createRestorePreviewItem(entryPath, itemKey, item, "added")
        )
        return
      }

      if (
        createRestoreContentHash(entryPath, currentItem) !==
        createRestoreContentHash(entryPath, item)
      ) {
        preview.conflicts.push(
          createRestorePreviewItem(
            entryPath,
            itemKey,
            item,
            "conflict",
            currentItem
          )
        )
      }
    })
    return
  }

  if (backupValue && typeof backupValue === "object") {
    const currentObject =
      currentValue &&
      typeof currentValue === "object" &&
      !Array.isArray(currentValue)
        ? currentValue
        : {}

    for (const [itemKey, value] of Object.entries(backupValue)) {
      if (!(itemKey in currentObject)) {
        preview.added.push(
          createRestorePreviewItem(entryPath, itemKey, value, "added")
        )
        continue
      }

      if (
        createRestoreContentHash(entryPath, currentObject[itemKey]) !==
        createRestoreContentHash(entryPath, value)
      ) {
        preview.conflicts.push(
          createRestorePreviewItem(
            entryPath,
            itemKey,
            value,
            "conflict",
            currentObject[itemKey]
          )
        )
      }
    }
    return
  }

  if (
    createRestoreContentHash(entryPath, currentValue) !==
    createRestoreContentHash(entryPath, backupValue)
  ) {
    preview.conflicts.push(
      createRestorePreviewItem(
        entryPath,
        entryPath,
        backupValue,
        "conflict",
        currentValue
      )
    )
  }
}

function mergeSkillRestoreValue(currentValue, backupValue) {
  const { installedTargets, installStates, status, ...nextBackupValue } =
    backupValue || {}

  return {
    ...nextBackupValue,
    installedTargets: currentValue?.installedTargets || [],
    installStates: currentValue?.installStates || {},
    status: currentValue?.status || "not-installed"
  }
}

function mergePromptRuntimeRestoreValue(currentValue, backupValue) {
  return {
    ...backupValue,
    lastSyncAt: currentValue?.lastSyncAt || backupValue?.lastSyncAt,
    runtimePath: currentValue?.runtimePath || backupValue?.runtimePath
  }
}

function mergeRestoreValue(entryPath, currentValue, backupValue) {
  if (entryPath === "storage/skills.json") {
    return mergeSkillRestoreValue(currentValue, backupValue)
  }

  if (entryPath === "storage/prompt-runtime-state.json") {
    return mergePromptRuntimeRestoreValue(currentValue, backupValue)
  }

  if (entryPath === "storage/codex-accounts.json") {
    return {
      ...backupValue,
      usage: currentValue?.usage || backupValue?.usage
    }
  }

  if (entryPath === "storage/runtime-provider-state.json") {
    return {
      ...backupValue,
      runtimeHash: currentValue?.runtimeHash || backupValue?.runtimeHash
    }
  }

  return backupValue
}

async function collectBackupEntries(paths) {
  const storageFiles = [
    [paths.storageFiles.skills, "storage/skills.json"],
    [paths.storageFiles.installs, "storage/installs.json"],
    [paths.storageFiles.usageLogs, "storage/usage-logs.json"],
    [paths.storageFiles.usagePricing, "storage/usage-pricing.json"],
    [paths.storageFiles.providers, "storage/providers.json"],
    [paths.storageFiles.runtimeModels, "storage/runtime-models.json"],
    [paths.storageFiles.runtimeProfiles, "storage/runtime-profiles.json"],
    [
      paths.storageFiles.runtimeProviderState,
      "storage/runtime-provider-state.json"
    ],
    [
      paths.storageFiles.runtimeProviderKeys,
      "storage/runtime-provider-keys.json"
    ],
    [paths.storageFiles.claudeProxyConfig, "storage/claude-proxy-config.json"],
    [
      paths.storageFiles.claudeProxyRequestLogs,
      "storage/claude-proxy-request-logs.json"
    ],
    [paths.storageFiles.codexProxyConfig, "storage/codex-proxy-config.json"],
    [
      paths.storageFiles.codexProxyRequestLogs,
      "storage/codex-proxy-request-logs.json"
    ],
    [paths.storageFiles.codexAccounts, "storage/codex-accounts.json"],
    [
      paths.storageFiles.codexActiveAccountId,
      "storage/codex-active-account-id.json"
    ],
    [paths.storageFiles.rules, "storage/rules.json"],
    [paths.storageFiles.promptRuntimeState, "storage/prompt-runtime-state.json"]
  ]
  const entries = []

  for (const [sourcePath, relativePath] of storageFiles) {
    const entry = await collectFileEntry(sourcePath, relativePath)

    if (entry) {
      entries.push(entry)
    }
  }

  for (const sourcePath of [
    paths.skillsDir,
    paths.promptsDir,
    paths.promptProfilesDir
  ]) {
    const sourceEntries = await collectDirectoryEntries(sourcePath)
    const rootName = path
      .relative(paths.workspaceRoot, sourcePath)
      .replace(/\\/g, "/")

    entries.push({
      path: rootName,
      type: "dir"
    })
    entries.push(
      ...sourceEntries.map((entry) => ({
        ...entry,
        path: `${rootName}/${entry.path}`
      }))
    )
  }

  return sanitizeRuntimeBackupEntries(entries)
}

function encryptBackupData(value) {
  const iv = crypto.randomBytes(12)
  const cipher = crypto.createCipheriv("aes-256-gcm", BACKUP_SECRET, iv)
  const content = Buffer.concat([
    cipher.update(`AI_MANAGER::RUNTIME_KEYS::${JSON.stringify(value)}`, "utf8"),
    cipher.final()
  ])

  return [
    iv.toString("base64"),
    cipher.getAuthTag().toString("base64"),
    content.toString("base64")
  ].join(".")
}

function decryptBackupData(value) {
  const [ivText, tagText, contentText] = String(value || "").split(".")
  const decipher = crypto.createDecipheriv(
    "aes-256-gcm",
    BACKUP_SECRET,
    Buffer.from(ivText, "base64")
  )
  decipher.setAuthTag(Buffer.from(tagText, "base64"))

  return JSON.parse(
    Buffer.concat([
      decipher.update(Buffer.from(contentText, "base64")),
      decipher.final()
    ])
      .toString("utf8")
      .replace(/^AI_MANAGER::RUNTIME_KEYS::/, "")
  )
}

function encryptBackupPayload(payload) {
  const iv = crypto.randomBytes(12)
  const cipher = crypto.createCipheriv("aes-256-gcm", BACKUP_SECRET, iv)
  const content = Buffer.concat([
    cipher.update(JSON.stringify(payload), "utf8"),
    cipher.final()
  ])

  return JSON.stringify(
    {
      version: 1,
      algorithm: "aes-256-gcm",
      iv: iv.toString("base64"),
      tag: cipher.getAuthTag().toString("base64"),
      content: content.toString("base64")
    },
    null,
    2
  )
}

function decryptBackupPayload(content) {
  const payload = JSON.parse(content)
  const decipher = crypto.createDecipheriv(
    "aes-256-gcm",
    BACKUP_SECRET,
    Buffer.from(payload.iv, "base64")
  )
  decipher.setAuthTag(Buffer.from(payload.tag, "base64"))

  return JSON.parse(
    Buffer.concat([
      decipher.update(Buffer.from(payload.content, "base64")),
      decipher.final()
    ]).toString("utf8")
  )
}

function assertBackupPath(rootPath, targetPath) {
  const relativePath = path.relative(rootPath, targetPath)

  if (relativePath.startsWith("..") || path.isAbsolute(relativePath)) {
    throw new Error("备份路径非法")
  }
}

async function readCurrentFile(rootPath, entryPath) {
  const targetPath = path.resolve(rootPath, entryPath)

  assertBackupPath(rootPath, targetPath)

  if (!(await pathExists(targetPath))) {
    return null
  }

  return fs.readFile(targetPath)
}

async function createRestorePreview(rootPath, entries) {
  const preview = {
    added: [],
    conflicts: []
  }

  for (const entry of entries.filter((item) => item.type === "file")) {
    const currentContent = await readCurrentFile(rootPath, entry.path)

    if (isMergeableRestoreJsonPath(entry.path)) {
      appendJsonRestorePreview(
        entry.path,
        currentContent
          ? JSON.parse(currentContent.toString("utf8"))
          : Array.isArray(readBackupEntryJson(entry))
            ? []
            : {},
        readBackupEntryJson(entry),
        preview
      )
      continue
    }

    if (!currentContent) {
      preview.added.push(createRestoreFilePreviewItem(entry.path, "added"))
      continue
    }

    if (
      sha256(currentContent) !== sha256(Buffer.from(entry.content, "base64"))
    ) {
      preview.conflicts.push(
        createRestoreFilePreviewItem(
          entry.path,
          "conflict",
          currentContent.toString("utf8"),
          Buffer.from(entry.content, "base64").toString("utf8")
        )
      )
    }
  }

  for (const entry of entries.filter((item) => item.type === "symlink")) {
    const targetPath = path.resolve(rootPath, entry.path)

    assertBackupPath(rootPath, targetPath)

    if (!(await pathExists(targetPath))) {
      preview.added.push(createRestoreFilePreviewItem(entry.path, "added"))
      continue
    }

    const stat = await fs.lstat(targetPath)
    const currentTarget = stat.isSymbolicLink()
      ? await fs.readlink(targetPath)
      : ""

    if (currentTarget !== entry.target) {
      preview.conflicts.push(
        createRestoreFilePreviewItem(
          entry.path,
          "conflict",
          currentTarget,
          entry.target
        )
      )
    }
  }

  return {
    ...preview,
    addedCount: preview.added.length,
    conflictCount: preview.conflicts.length
  }
}

function mergeJsonBackupValue(entryPath, currentValue, backupValue, choices) {
  if (Array.isArray(backupValue)) {
    const nextItems = Array.isArray(currentValue) ? [...currentValue] : []
    const nextIndexMap = new Map(
      nextItems.map((item, index) => [
        getRestoreItemKey(entryPath, item, index),
        index
      ])
    )

    backupValue.forEach((item, index) => {
      const itemKey = getRestoreItemKey(entryPath, item, index)
      const nextIndex = nextIndexMap.get(itemKey)

      if (nextIndex === undefined) {
        nextIndexMap.set(itemKey, nextItems.length)
        nextItems.push(item)
        return
      }

      if (choices[createRestoreChoiceKey(entryPath, itemKey)] === "backup") {
        nextItems[nextIndex] = mergeRestoreValue(
          entryPath,
          nextItems[nextIndex],
          item
        )
      }
    })

    return nextItems
  }

  if (backupValue && typeof backupValue === "object") {
    const nextValue =
      currentValue &&
      typeof currentValue === "object" &&
      !Array.isArray(currentValue)
        ? { ...currentValue }
        : {}

    for (const [itemKey, value] of Object.entries(backupValue)) {
      if (
        !(itemKey in nextValue) ||
        choices[createRestoreChoiceKey(entryPath, itemKey)] === "backup"
      ) {
        nextValue[itemKey] = mergeRestoreValue(
          entryPath,
          nextValue[itemKey],
          value
        )
      }
    }

    return nextValue
  }

  return choices[createRestoreChoiceKey(entryPath, entryPath)] === "backup"
    ? backupValue
    : currentValue
}

async function restoreJsonEntry(rootPath, entry, choices) {
  const targetPath = path.resolve(rootPath, entry.path)
  const currentContent = await readCurrentFile(rootPath, entry.path)
  const backupValue = readBackupEntryJson(entry)
  const currentValue = currentContent
    ? JSON.parse(currentContent.toString("utf8"))
    : Array.isArray(backupValue)
      ? []
      : {}

  assertBackupPath(rootPath, targetPath)
  await fs.mkdir(path.dirname(targetPath), { recursive: true })
  await fs.writeFile(
    targetPath,
    `${JSON.stringify(
      mergeJsonBackupValue(entry.path, currentValue, backupValue, choices),
      null,
      2
    )}\n`,
    "utf8"
  )
}

async function restoreDirectoryEntries(rootPath, entries, choices = {}) {
  await fs.mkdir(rootPath, { recursive: true })

  for (const entry of entries.filter((item) => item.type === "dir")) {
    const targetPath = path.resolve(rootPath, entry.path)

    assertBackupPath(rootPath, targetPath)
    await fs.mkdir(targetPath, { recursive: true })
  }

  for (const entry of entries.filter((item) => item.type === "file")) {
    const targetPath = path.resolve(rootPath, entry.path)

    assertBackupPath(rootPath, targetPath)

    if (isMergeableRestoreJsonPath(entry.path)) {
      await restoreJsonEntry(rootPath, entry, choices)
      continue
    }

    const currentContent = await readCurrentFile(rootPath, entry.path)
    const backupContent = Buffer.from(entry.content, "base64")

    if (
      currentContent &&
      sha256(currentContent) !== sha256(backupContent) &&
      choices[createRestoreFileKey(entry.path)] !== "backup"
    ) {
      continue
    }

    await fs.mkdir(path.dirname(targetPath), { recursive: true })
    await fs.writeFile(targetPath, backupContent)
  }

  for (const entry of entries.filter((item) => item.type === "symlink")) {
    const targetPath = path.resolve(rootPath, entry.path)

    assertBackupPath(rootPath, targetPath)

    if (await pathExists(targetPath)) {
      const stat = await fs.lstat(targetPath)
      const currentTarget = stat.isSymbolicLink()
        ? await fs.readlink(targetPath)
        : ""

      if (
        currentTarget !== entry.target &&
        choices[createRestoreFileKey(entry.path)] !== "backup"
      ) {
        continue
      }

      await fs.rm(targetPath, { recursive: true, force: true })
    }

    await fs.mkdir(path.dirname(targetPath), { recursive: true })
    await fs.symlink(entry.target, targetPath)
  }
}

function createRuntimeProfileConfigHash(runtimeProviderService, profile) {
  const provider = runtimeProviderService.providers.find(
    (item) => item.id === profile.providerId
  )

  if (!provider) {
    return ""
  }

  const files = runtimeProviderService.buildCliConfigFiles(
    profile.cli,
    provider,
    profile
  )

  return sha256(
    JSON.stringify(
      files.map((file) => ({
        name: file.name,
        content: file.content || ""
      }))
    )
  )
}

function createRuntimeProfileRestoreSnapshots(runtimeProviderService) {
  return runtimeProviderService.profiles.map((profile) => ({
    cli: profile.cli,
    providerId: profile.providerId,
    configHash: createRuntimeProfileConfigHash(
      runtimeProviderService,
      profile
    )
  }))
}

function createCodexAccountConfigHash(account) {
  return sha256(
    JSON.stringify({
      id: account.id,
      type: account.type || "",
      disabled: Boolean(account.disabled),
      accountId: account.account_id || account.accountId || account.id,
      accessToken: account.auth?.accessToken || account.access_token || "",
      refreshToken: account.auth?.refreshToken || account.refresh_token || "",
      idToken: account.auth?.idToken || account.id_token || ""
    })
  )
}

function createCodexAccountRestoreSnapshot(codexAccountService) {
  const accountId = codexAccountService.activeAccountId || ""
  const account = codexAccountService.accounts.find(
    (item) => item.id === accountId
  )

  if (!account) {
    return null
  }

  return {
    accountId,
    configHash: createCodexAccountConfigHash(account)
  }
}

function createCodexProxyConfigHash(config) {
  return sha256(
    JSON.stringify({
      enabled: Boolean(config.enabled),
      host: config.host || "",
      port: config.port || 0,
      activeProviderId: config.activeProviderId || "",
      failoverProviderIds: config.failoverProviderIds || [],
      accountModel: config.accountModel || "",
      retryCount: config.retryCount || 0,
      streamTimeoutMs: config.streamTimeoutMs || 0,
      requestTimeoutMs: config.requestTimeoutMs || 0
    })
  )
}

function createCodexProxyTargetConfigHash(codexProxyService, targetId) {
  if (!targetId) {
    return ""
  }

  if (targetId.startsWith("account:")) {
    const accountId = targetId.slice("account:".length)
    const account = codexProxyService.codexAccountService.accounts.find(
      (item) => item.id === accountId
    )

    return account
      ? sha256(
          JSON.stringify({
            accountHash: createCodexAccountConfigHash(account),
            proxy: account.proxy || "",
            model: codexProxyService.getState().accountModel || ""
          })
        )
      : ""
  }

  const provider = codexProxyService.runtimeProviderService.providers.find(
    (item) => item.id === targetId
  )

  if (!provider) {
    return ""
  }

  return sha256(
    JSON.stringify({
      id: provider.id,
      cli: provider.cli,
      type: provider.type,
      disabled: provider.enabled === false,
      baseUrl: provider.baseUrl || "",
      proxy: provider.proxy || "",
      model: provider.runtimeConfig?.mainModel || "",
      apiKey:
        codexProxyService.runtimeProviderService.keyManager.getProviderKey(
          provider.id
        ) || ""
    })
  )
}

function createCodexProxyRestoreSnapshot(codexProxyService) {
  const config = codexProxyService.getState()

  if (!config.enabled) {
    return null
  }

  return {
    configHash: createCodexProxyConfigHash(config),
    targetId: config.activeProviderId || "",
    targetHash: createCodexProxyTargetConfigHash(
      codexProxyService,
      config.activeProviderId
    )
  }
}

function createProxyRestoreSnapshots(proxyServices) {
  return Object.fromEntries(
    Object.entries(proxyServices)
      .map(([cli, service]) => [cli, createCodexProxyRestoreSnapshot(service)])
      .filter(([, snapshot]) => Boolean(snapshot))
  )
}

class ManagerService extends EventEmitter {
  constructor(userDataPath, appSettings = {}) {
    super()
    this.appSettings = appSettings
    this.paths = resolveAppPaths(userDataPath)
    this.storage = new JsonStorage(this.paths.storageFiles)
    this.cliDetectionService = new CliDetectionService(
      this.appSettings.cliConfigPaths
    )
    this.metadataParser = new MetadataParser()
    this.skillScanner = new SkillScanner()
    this.linkManager = new LinkManager(this.cliDetectionService)
    this.repoService = new RepoService(this.paths, this.storage)
    this.fileWatcherService = new FileWatcherService()
    this.sessionService = new SessionService(this.paths)
    this.sessionService.bindStorage(this.storage)
    this.usageService = new UsageService()
    this.usageService.bindStorage(this.storage)
    this.skillUsageService = new SkillUsageService(this.sessionService)
    this.codexAccountService = new CodexAccountService(this.storage)
    this.runtimeProviderService = new RuntimeProviderService(this.storage)
    this.claudeProxyService = new CodexProxyService(
      this.storage,
      this.runtimeProviderService,
      this.codexAccountService,
      () => this.state.cliTargets.find((item) => item.id === "claude"),
      { cli: "claude" }
    )
    this.codexProxyService = new CodexProxyService(
      this.storage,
      this.runtimeProviderService,
      this.codexAccountService,
      () => this.state.cliTargets.find((item) => item.id === "codex")
    )
    this.promptRuntimeService = new PromptRuntimeService(this.paths)
    this.state = {
      cliTargets: [],
      skills: [],
      repos: [],
      sessions: [],
      usage: this.usageService.getStats().data,
      codexAccounts: [],
      codexLoginState: null,
      providers: [],
      rules: this.promptRuntimeService.getState(),
      runtimeConfigSchemas: {},
      runtimeModels: [],
      runtimeProfiles: [],
      runtimeProviderState: {},
      claudeProxyState: this.claudeProxyService.getState(),
      codexProxyState: this.codexProxyService.getState(),
      diagnostics: [],
      paths: this.toPublicPaths(),
      appSettings: this.toPublicSettings(false),
      refreshedAt: 0
    }
  }

  async init() {
    await ensureAppDirectories(this.paths)
    await this.repoService.init()
    await this.sessionService.init()
    await this.usageService.init()
    await this.codexAccountService.init()
    await this.promptRuntimeService.init()
    this.codexAccountService.on("changed", (codexAccounts) => {
      this.state = {
        ...this.state,
        codexAccounts,
        refreshedAt: Date.now()
      }
      this.emit("state-changed", this.state)
    })
    this.codexAccountService.on("login-state", (codexLoginState) => {
      this.state = {
        ...this.state,
        codexLoginState,
        refreshedAt: Date.now()
      }
      this.emit("state-changed", this.state)
    })
    this.claudeProxyService.on("changed", (claudeProxyState) => {
      this.state = {
        ...this.state,
        ...this.getRuntimeStateWithProxy(),
        claudeProxyState,
        refreshedAt: Date.now()
      }
      this.emit("state-changed", this.state)
    })
    this.codexProxyService.on("changed", (codexProxyState) => {
      this.state = {
        ...this.state,
        ...this.getRuntimeStateWithProxy(),
        codexProxyState,
        refreshedAt: Date.now()
      }
      this.emit("state-changed", this.state)
    })
    await this.runtimeProviderService.init()
    await this.claudeProxyService.init()
    await this.codexProxyService.init()
    await this.refreshAll({ emit: false })
    this.codexAccountService.startAutoRefresh(() =>
      this.state.cliTargets.find((item) => item.id === "codex")
    )
    this.startWatcher()
    this.startSessionWatcher()
  }

  toPublicPaths() {
    return {
      workspaceRoot: this.paths.workspaceRoot,
      skillsDir: this.paths.skillsDir,
      promptsDir: this.paths.promptsDir,
      promptProfilesDir: this.paths.promptProfilesDir,
      reposDir: this.paths.reposDir,
      sessionRecycleDir: this.paths.sessionRecycleDir,
      storageDir: this.paths.storageDir
    }
  }

  toPublicSettings(restartRequired) {
    return {
      ...this.appSettings,
      restartRequired: Boolean(restartRequired)
    }
  }

  setAppSettings(appSettings, restartRequired = false) {
    this.appSettings = appSettings
    this.state = {
      ...this.state,
      appSettings: this.toPublicSettings(restartRequired)
    }
    this.emit("state-changed", this.state)
  }

  async updateAppSettings(appSettings) {
    this.appSettings = appSettings
    this.cliDetectionService = new CliDetectionService(
      this.appSettings.cliConfigPaths
    )
    this.linkManager = new LinkManager(this.cliDetectionService)
    this.promptRuntimeService = new PromptRuntimeService(this.paths)
    await this.promptRuntimeService.init()
    await this.refreshAll({ preferDetectedPaths: true })
  }

  async createDataBackup() {
    await this.storage.flush()

    return encryptBackupPayload({
      version: 1,
      createdAt: Date.now(),
      appSettings: serializeAppSettingsPaths(this.appSettings),
      workspaceEntries: await collectBackupEntries(this.paths),
      runtimeProviderKeys: encryptBackupData(
        this.runtimeProviderService.exportProviderKeys()
      )
    })
  }

  async previewDataBackupRestore(content) {
    const backup = parseBackup(content)

    return {
      createdAt: backup.createdAt || 0,
      ...(await createRestorePreview(
        this.paths.workspaceRoot,
        backup.workspaceEntries
      ))
    }
  }

  inspectDataBackup(content) {
    return createBackupDataView(content)
  }

  async restoreDataBackup(content, options = {}) {
    const backup = parseBackup(content)
    const choices = options.choices || {}
    const runtimeProfileSnapshots = createRuntimeProfileRestoreSnapshots(
      this.runtimeProviderService
    )
    const codexAccountSnapshot = createCodexAccountRestoreSnapshot(
      this.codexAccountService
    )
    const proxySnapshots = createProxyRestoreSnapshots({
      claude: this.claudeProxyService,
      codex: this.codexProxyService
    })

    this.codexAccountService.stopAutoRefresh()
    this.fileWatcherService.stop()
    await this.sessionService.dispose()
    await this.storage.flush()

    await ensureAppDirectories(this.paths)
    await restoreDirectoryEntries(
      this.paths.workspaceRoot,
      backup.workspaceEntries,
      choices
    )
    await this.storage.writeNow("cliTargets", [])
    if (backup.runtimeProviderKeys) {
      await this.runtimeProviderService.mergeProviderKeys(
        decryptBackupData(backup.runtimeProviderKeys),
        choices
      )
    }
    await this.runtimeProviderService.init()
    await this.codexAccountService.init()
    await this.claudeProxyService.init()
    await this.codexProxyService.init()

    for (const [cli, snapshot] of Object.entries(proxySnapshots)) {
      const proxyService = this.getProxyService(cli)
      const nextProxyConfig = proxyService.getState()
      const nextProxyTargetHash = createCodexProxyTargetConfigHash(
        proxyService,
        snapshot.targetId
      )

      if (
        createCodexProxyConfigHash(nextProxyConfig) !==
          snapshot.configHash ||
        nextProxyTargetHash !== snapshot.targetHash
      ) {
        await this.disableProxy(cli)
      }
    }

    for (const snapshot of runtimeProfileSnapshots) {
      const nextProfile = this.runtimeProviderService.profiles.find(
        (item) =>
          item.cli === snapshot.cli && item.providerId === snapshot.providerId
      )
      const nextHash = nextProfile
        ? createRuntimeProfileConfigHash(
            this.runtimeProviderService,
            nextProfile
          )
        : ""

      if (nextHash && nextHash === snapshot.configHash) {
        continue
      }

      if (!this.getProxyService(snapshot.cli)?.isEnabled()) {
        await this.clearRuntime(snapshot.cli)
      }
    }

    if (codexAccountSnapshot) {
      const nextAccount = this.codexAccountService.accounts.find(
        (item) => item.id === codexAccountSnapshot.accountId
      )
      const nextHash = nextAccount
        ? createCodexAccountConfigHash(nextAccount)
        : ""

      if (
        this.codexAccountService.activeAccountId ===
          codexAccountSnapshot.accountId &&
        nextHash !== codexAccountSnapshot.configHash
      ) {
        await this.clearCodexAccount()
      }
    }

    return {
      appSettings: this.appSettings
    }
  }

  startWatcher() {
    const repoPaths = this.repoService.listRepos().map((item) => item.localPath)
    const promptRuntimePaths = this.promptRuntimeService.getRuntimeWatchPaths(
      this.state.cliTargets
    )
    const runtimeProviderPaths =
      this.runtimeProviderService.getRuntimeWatchPaths(this.state.cliTargets)

    this.fileWatcherService.restart(
      [
        this.paths.skillsDir,
        this.paths.promptsDir,
        this.paths.promptProfilesDir,
        this.paths.reposDir,
        ...repoPaths,
        ...promptRuntimePaths,
        ...runtimeProviderPaths
      ],
      async () => {
        await this.refreshAll()
      }
    )
  }

  startSessionWatcher() {
    this.sessionService.startWatcher(this.state.cliTargets, async () => {
      const { sessions, diagnostics } = await this.sessionService.refresh(
        this.state.cliTargets
      )
      const runtimeState = this.getRuntimeStateWithProxy()
      const codexAccounts = this.codexAccountService.getState()
      const { diagnostics: usageDiagnostics } = await this.usageService.refresh(
        {
          sessions,
          providers: runtimeState.providers,
          runtimeProfiles: runtimeState.runtimeProfiles,
          runtimeProviderState: runtimeState.runtimeProviderState,
          codexAccounts,
          proxyStates: this.getProxyStatePatch(),
          codexProxyState: this.codexProxyService.getState()
        }
      )

      this.state = {
        ...this.state,
        sessions,
        usage: this.usageService.getStats().data,
        diagnostics: [
          ...this.state.diagnostics.filter(
            (item) =>
              item.type !== "session-parse-error" &&
              item.type !== "usage-parse-error"
          ),
          ...diagnostics,
          ...usageDiagnostics
        ],
        refreshedAt: Date.now()
      }
      this.emit("state-changed", this.state)
    })
  }

  getState() {
    return this.state
  }

  getProxyService(cli) {
    if (cli === "claude") {
      return this.claudeProxyService
    }

    if (cli === "codex") {
      return this.codexProxyService
    }

    return null
  }

  getProxyStateKey(cli) {
    return cli === "claude" ? "claudeProxyState" : "codexProxyState"
  }

  getProxyStatePatch() {
    return {
      claudeProxyState: this.claudeProxyService.getState(),
      codexProxyState: this.codexProxyService.getState()
    }
  }

  getRuntimeStateWithProxy() {
    const runtimeState = this.runtimeProviderService.getState()

    for (const cli of ["claude", "codex"]) {
      const proxyService = this.getProxyService(cli)

      if (!proxyService?.isEnabled()) {
        continue
      }

      runtimeState.runtimeProviderState = {
        ...runtimeState.runtimeProviderState,
        [cli]: {
          ...(runtimeState.runtimeProviderState[cli] || {}),
          activeProviderId: proxyService.getState().activeProviderId,
          status: "PROXY_MANAGED"
        }
      }
    }

    return runtimeState
  }

  async refreshAll({ emit = true, preferDetectedPaths = false } = {}) {
    const previousSkills = await this.storage.read("skills", [])
    const previousCliTargets = (await this.storage.read("cliTargets", [])).map(
      (item) => ({
        ...item,
        configPath: resolvePortablePath(item.configPath),
        skillsPath: resolvePortablePath(item.skillsPath),
        sessionsPath: resolvePortablePath(item.sessionsPath),
        sessionPaths: Array.isArray(item.sessionPaths)
          ? item.sessionPaths.map((value) => resolvePortablePath(value))
          : item.sessionPaths
      })
    )
    const installIndex = await this.storage.read("installs", {})
    const normalizedInstallIndex = { ...installIndex }
    const [detectedCliTargets] = await Promise.all([
      this.cliDetectionService.detectAll()
    ])
    const cliTargets = this.mergeCliTargets(
      previousCliTargets,
      detectedCliTargets,
      { preferDetectedPaths }
    )
    const { sessions, diagnostics: sessionDiagnostics } =
      await this.sessionService.refresh(cliTargets)
    const repos = this.repoService.listRepos().map((repo) => ({
      ...repo,
      skillCount: 0
    }))
    await this.runtimeProviderService.refreshDrift(cliTargets)
    const runtimeState = this.runtimeProviderService.getState()
    const codexAccounts = this.codexAccountService.getState()
    const { diagnostics: usageDiagnostics } = await this.usageService.refresh({
      sessions,
      providers: runtimeState.providers,
      runtimeProfiles: runtimeState.runtimeProfiles,
      runtimeProviderState: runtimeState.runtimeProviderState,
      codexAccounts,
      proxyStates: this.getProxyStatePatch(),
      codexProxyState: this.codexProxyService.getState()
    })
    await this.promptRuntimeService.refreshDrift(cliTargets)
    const rules = this.promptRuntimeService.getState()
    const scannedItems = await this.skillScanner.scanMany([
      { rootPath: this.paths.skillsDir, repoId: null },
      ...repos.map((repo) => ({
        rootPath: repo.localPath,
        repoId: repo.id
      }))
    ])
    const diagnostics = []
    const parsedSkills = []
    const usedNames = new Set()

    for (const scannedItem of scannedItems) {
      try {
        const parsed = await this.metadataParser.parse(
          scannedItem.skillRoot,
          scannedItem.repoId
        )

        if (usedNames.has(parsed.name)) {
          diagnostics.push({
            type: "duplicate-skill-name",
            message: `发现重复 Skill 名称：${parsed.name}`,
            sourcePath: parsed.sourcePath
          })
          continue
        }

        usedNames.add(parsed.name)
        parsedSkills.push(parsed)
      } catch (error) {
        diagnostics.push({
          type: "metadata-error",
          message: error.message,
          sourcePath: scannedItem.skillRoot
        })
      }
    }

    const previousSkillMap = new Map(
      previousSkills.map((item) => [item.name, item])
    )
    const scannedSkillMap = new Map(
      parsedSkills.map((item) => [item.name, item])
    )

    for (const [skillName, targetIds] of Object.entries(installIndex)) {
      if (!targetIds?.length || scannedSkillMap.has(skillName)) {
        continue
      }

      for (const targetId of targetIds) {
        try {
          await this.linkManager.uninstallSkill(skillName, targetId)
        } catch (error) {
          diagnostics.push({
            type: "cleanup-error",
            message: `清理失效链接失败：${error.message}`,
            sourcePath: `${skillName} -> ${targetId}`
          })
        }
      }

      delete normalizedInstallIndex[skillName]
      diagnostics.push({
        type: "orphan-skill-cleaned",
        message: `Skill 源目录已删除，已自动清理挂载：${skillName}`,
        sourcePath: previousSkillMap.get(skillName)?.sourcePath || skillName
      })
    }

    const repoMap = new Map(repos.map((item) => [item.id, item]))
    const skills = []

    for (const skill of sortByName(parsedSkills)) {
      const installStates = {}
      const installedTargets = []

      for (const cliTarget of cliTargets) {
        const state = await this.linkManager.getInstallState(skill, cliTarget)
        installStates[cliTarget.id] = state

        if (["installed", "broken-link"].includes(state.state)) {
          installedTargets.push(cliTarget.id)
        }
      }

      if (skill.repoId && repoMap.has(skill.repoId)) {
        repoMap.get(skill.repoId).skillCount += 1
      }

      skills.push({
        ...skill,
        installedTargets,
        installStates,
        status: this.resolveSkillStatus(installStates),
        repoName:
          skill.repoId && repoMap.has(skill.repoId)
            ? repoMap.get(skill.repoId).name
            : "Managed"
      })
    }

    await this.persistSkills(skills, cliTargets, normalizedInstallIndex)

    this.state = {
      cliTargets,
      skills,
      repos,
      sessions,
      usage: this.usageService.getStats().data,
      codexAccounts,
      codexLoginState: this.codexAccountService.getLoginState(),
      ...runtimeState,
      ...this.getProxyStatePatch(),
      rules,
      diagnostics: [...diagnostics, ...sessionDiagnostics, ...usageDiagnostics],
      paths: this.toPublicPaths(),
      appSettings: this.toPublicSettings(false),
      refreshedAt: Date.now()
    }

    this.startSessionWatcher()

    if (emit) {
      this.emit("state-changed", this.state)
    }

    return this.state
  }

  mergeCliTargets(
    previousCliTargets,
    detectedCliTargets,
    { preferDetectedPaths = false } = {}
  ) {
    const detectedMap = new Map(
      detectedCliTargets.map((item) => [item.id, item])
    )
    const previousMap = new Map(
      previousCliTargets.map((item) => [item.id, item])
    )
    const targetIds = [
      ...new Set([...detectedMap.keys()])
    ]

    return targetIds
      .map((targetId) => {
        const detected = detectedMap.get(targetId) || {}
        const previous = previousMap.get(targetId) || {}

        if (!previous.id && !detected.installed) {
          return null
        }

        const merged = {
          ...detected,
          ...previous,
          installed:
            detected.installed === undefined
              ? previous.installed
              : detected.installed,
          executablePath: detected.executablePath || previous.executablePath,
          sessionsPath: detected.sessionsPath || previous.sessionsPath,
          sessionPaths: detected.sessionPaths || previous.sessionPaths,
          sessionScanRules:
            detected.sessionScanRules || previous.sessionScanRules,
          version: detected.version || previous.version,
          detectedAt: detected.detectedAt || previous.detectedAt
        }

        if (preferDetectedPaths) {
          merged.configPath = detected.configPath || previous.configPath
          merged.skillsPath = detected.skillsPath || previous.skillsPath
          merged.sessionsPath = detected.sessionsPath || previous.sessionsPath
          merged.sessionPaths = detected.sessionPaths || previous.sessionPaths
          merged.sessionScanRules =
            detected.sessionScanRules || previous.sessionScanRules
        }

        return merged
      })
      .filter(Boolean)
  }

  resolveSkillStatus(installStates) {
    const states = Object.values(installStates).map((item) => item.state)

    if (states.includes("broken-link")) {
      return "broken-link"
    }

    if (states.includes("installed")) {
      return "installed"
    }

    if (states.every((item) => item === "disabled")) {
      return "disabled"
    }

    return "not-installed"
  }

  async persistSkills(skills, cliTargets, installIndex) {
    const normalizedInstallIndex = { ...installIndex }
    const serializedCliTargets = cliTargets.map((item) => ({
      ...item,
      configPath: serializePortablePath(item.configPath),
      skillsPath: serializePortablePath(item.skillsPath),
      sessionsPath: serializePortablePath(item.sessionsPath),
      sessionPaths: Array.isArray(item.sessionPaths)
        ? item.sessionPaths.map((value) => serializePortablePath(value))
        : item.sessionPaths
    }))

    for (const skill of skills) {
      if (skill.installedTargets.length) {
        normalizedInstallIndex[skill.name] = skill.installedTargets
      } else {
        delete normalizedInstallIndex[skill.name]
      }
    }

    this.storage.scheduleWrite("skills", skills)
    this.storage.scheduleWrite("cliTargets", serializedCliTargets)
    this.storage.scheduleWrite("installs", normalizedInstallIndex)
  }

  async installSkill(skillName, targetId) {
    const skill = this.state.skills.find((item) => item.name === skillName)

    if (!skill) {
      throw new Error("Skill 不存在")
    }

    await this.linkManager.installSkill(skill, targetId)
    await this.refreshAll()
  }

  async uninstallSkill(skillName, targetId) {
    await this.linkManager.uninstallSkill(skillName, targetId)
    await this.refreshAll()
  }

  async repairSkill(skillName, targetId) {
    const skill = this.state.skills.find((item) => item.name === skillName)

    if (!skill) {
      throw new Error("Skill 不存在")
    }

    if (!(await pathExists(skill.sourcePath))) {
      throw new Error("Skill 源目录不存在，当前无法修复")
    }

    await this.linkManager.repairSkill(skill, targetId)
    await this.refreshAll()
  }

  async getSkillFiles(skillName) {
    const skill = this.state.skills.find((item) => item.name === skillName)

    if (!skill) {
      throw new Error("Skill 不存在")
    }

    if (!(await pathExists(skill.sourcePath))) {
      throw new Error("Skill 源目录不存在")
    }

    return {
      sourcePath: skill.sourcePath,
      entries: await collectSkillViewEntries(skill.sourcePath)
    }
  }

  async createSkill(input) {
    const skillName = String(input.name || "").trim()

    if (!skillName) {
      throw new Error("Skill 名称不能为空")
    }

    if (this.state.skills.find((item) => item.name === skillName)) {
      throw new Error(`Skill 名称已存在：${skillName}`)
    }

    const directoryName = slugifyName(skillName) || `skill-${Date.now()}`
    const skillRoot = path.join(this.paths.skillsDir, directoryName)

    if (await pathExists(skillRoot)) {
      throw new Error("同名目录已存在，请修改 Skill 名称")
    }

    const tags = Array.isArray(input.tags) ? input.tags.filter(Boolean) : []
    const toYamlScalar = (value) => JSON.stringify(String(value))
    const frontmatterLines = [
      "---",
      `name: ${toYamlScalar(skillName)}`,
      input.description
        ? `description: ${toYamlScalar(input.description)}`
        : null,
      input.author ? `author: ${toYamlScalar(input.author)}` : null,
      tags.length ? "tags:" : null,
      ...tags.map((item) => `  - ${toYamlScalar(item)}`),
      "entry: prompt.md",
      "---",
      "",
      `# ${skillName}`,
      "",
      "这个 Skill 由 Monkey Thief 创建。"
    ].filter((item) => item !== null)

    await fs.mkdir(skillRoot, { recursive: true })
    await fs.writeFile(
      path.join(skillRoot, "SKILL.md"),
      `${frontmatterLines.join("\n")}\n`,
      "utf8"
    )
    await fs.writeFile(
      path.join(skillRoot, "prompt.md"),
      `# ${skillName}\n\n在这里补充你的 Skill 提示词。\n`,
      "utf8"
    )

    await this.refreshAll()
  }

  async importSkillFromZip(zipPath) {
    const sourceZipPath = String(zipPath || "").trim()

    if (!sourceZipPath) {
      throw new Error("请选择 Skill zip 压缩包")
    }

    if (path.extname(sourceZipPath).toLowerCase() !== ".zip") {
      throw new Error("只能导入 zip 压缩包")
    }

    if (!(await pathExists(sourceZipPath))) {
      throw new Error("zip 压缩包不存在")
    }

    const tempRoot = await fs.mkdtemp(
      path.join(os.tmpdir(), "monkey-thief-skill-")
    )

    try {
      await extractZip(sourceZipPath, tempRoot)
      const scannedItems = await this.skillScanner.scanRoot(tempRoot)

      if (!scannedItems.length) {
        throw new Error("zip 压缩包中未找到 SKILL.md")
      }

      const parsedSkills = []
      const seenNames = new Set()

      for (const scannedItem of scannedItems) {
        const parsed = await this.metadataParser.parse(scannedItem.skillRoot)

        if (seenNames.has(parsed.name)) {
          throw new Error(`zip 压缩包中存在重复 Skill 名称：${parsed.name}`)
        }

        seenNames.add(parsed.name)
        parsedSkills.push({
          ...parsed,
          sourcePath: scannedItem.skillRoot
        })
      }

      const importItems = []

      for (const parsed of parsedSkills) {
        const directoryName =
          slugifyName(parsed.name) || path.basename(parsed.sourcePath)
        const managedPath = path.join(this.paths.skillsDir, directoryName)
        const existingSkill = this.state.skills.find(
          (item) => item.name === parsed.name
        )

        if (existingSkill) {
          const incomingSignature = await createSkillSignature(parsed)
          const existingSignature = await createSkillSignature(existingSkill)

          if (incomingSignature === existingSignature) {
            continue
          }

          throw new Error(
            `Skill 名称已存在，请先处理同名 Skill：${parsed.name}`
          )
        }

        if (await pathExists(managedPath)) {
          throw new Error(`集中目录已存在同名目录：${managedPath}`)
        }

        importItems.push({
          sourcePath: parsed.sourcePath,
          managedPath
        })
      }

      if (!importItems.length) {
        throw new Error("zip 压缩包中的 Skill 已存在，无需重复导入")
      }

      for (const item of importItems) {
        await fs.cp(item.sourcePath, item.managedPath, {
          recursive: true
        })
      }

      await this.refreshAll()
    } finally {
      await fs.rm(tempRoot, { recursive: true, force: true })
    }
  }

  async collectCliSkillImports(targetId) {
    const detectedTargets = targetId
      ? [await this.cliDetectionService.getAdapter(targetId).detect()]
      : await this.cliDetectionService.detectAll()
    const cliTargets = detectedTargets.filter(
      (item) => item.installed && item.skillsPath
    )
    const managedPaths = new Map(
      this.state.skills.map((item) => [item.name, item.sourcePath])
    )
    const imports = []
    const mounts = []

    for (const cliTarget of cliTargets) {
      const skillsPathStat = await fs
        .lstat(cliTarget.skillsPath)
        .catch((error) => {
          if (error.code === "ENOENT") {
            return null
          }

          throw error
        })

      if (!skillsPathStat?.isDirectory()) {
        continue
      }

      const entries = await fs.readdir(cliTarget.skillsPath, {
        withFileTypes: true
      })

      for (const entry of entries) {
        if (!entry.isDirectory()) {
          continue
        }

        const sourcePath = path.join(cliTarget.skillsPath, entry.name)
        const sourceStat = await fs.lstat(sourcePath)

        if (sourceStat.isSymbolicLink()) {
          continue
        }

        if (!(await pathExists(path.join(sourcePath, "SKILL.md")))) {
          continue
        }

        const parsed = await this.metadataParser.parse(sourcePath)
        const directoryName = slugifyName(parsed.name) || entry.name
        const managedPath =
          managedPaths.get(parsed.name) ||
          path.join(this.paths.skillsDir, directoryName)
        const mountedPath = path.join(cliTarget.skillsPath, parsed.name)

        if (managedPaths.has(parsed.name)) {
          mounts.push({
            name: parsed.name,
            description: parsed.description,
            cliId: cliTarget.id,
            cliName: cliTarget.name,
            sourcePath,
            managedPath,
            mountedPath,
            signature: await createSkillSignature({
              ...parsed,
              sourcePath
            })
          })
          continue
        }

        if (await pathExists(managedPath)) {
          throw new Error(`集中目录已存在同名目录：${managedPath}`)
        }

        managedPaths.set(parsed.name, managedPath)
        imports.push({
          name: parsed.name,
          description: parsed.description,
          cliId: cliTarget.id,
          cliName: cliTarget.name,
          sourcePath,
          managedPath,
          mountedPath,
          signature: await createSkillSignature({
            ...parsed,
            sourcePath
          })
        })
      }
    }

    return { imports, mounts }
  }

  async previewSkillsFromCli(targetId) {
    const { imports, mounts } = await this.collectCliSkillImports(targetId)
    const managedSignatures = new Map()
    const candidateGroups = new Map()

    for (const skill of this.state.skills) {
      managedSignatures.set(skill.name, await createSkillSignature(skill))
    }

    for (const candidate of [...imports, ...mounts]) {
      const nameGroups = candidateGroups.get(candidate.name) || new Map()
      const group = nameGroups.get(candidate.signature) || {
        name: candidate.name,
        description: candidate.description,
        signature: candidate.signature,
        cliNames: [],
        sourcePaths: [],
        items: []
      }

      group.cliNames.push(candidate.cliName)
      group.sourcePaths.push(candidate.sourcePath)
      group.items.push(candidate)
      nameGroups.set(candidate.signature, group)
      candidateGroups.set(candidate.name, nameGroups)
    }

    const candidates = []
    const conflicts = []

    for (const [name, signatureGroups] of candidateGroups) {
      const groups = Array.from(signatureGroups.values())
      const managedSignature = managedSignatures.get(name)
      const managedSkill = this.state.skills.find((item) => item.name === name)
      const managedGroups = managedSignature
        ? groups.filter((group) => group.signature === managedSignature)
        : []
      const newGroups = managedSignature
        ? groups.filter((group) => group.signature !== managedSignature)
        : groups

      for (const group of managedGroups) {
        candidates.push({
          ...group,
          id: group.sourcePaths[0],
          alreadyManaged: true
        })
      }

      if (!newGroups.length) {
        continue
      }

      if (managedSkill) {
        conflicts.push({
          name,
          options: [
            {
              id: `managed:${managedSkill.sourcePath}`,
              name: managedSkill.name,
              description: managedSkill.description,
              signature: managedSignature,
              cliNames: ["Monkey Thief"],
              sourcePaths: [managedSkill.sourcePath],
              alreadyManaged: true
            },
            ...newGroups.map((group) => ({
              ...group,
              id: group.sourcePaths[0],
              alreadyManaged: true
            }))
          ]
        })
        continue
      }

      if (newGroups.length === 1) {
        candidates.push({
          ...newGroups[0],
          id: newGroups[0].sourcePaths[0],
          alreadyManaged: false
        })
        continue
      }

      conflicts.push({
        name,
        options: newGroups.map((group) => ({
          ...group,
          id: group.sourcePaths[0],
          alreadyManaged: managedSignatures.has(name)
        }))
      })
    }

    return {
      candidates: candidates.sort((left, right) =>
        left.name.localeCompare(right.name)
      ),
      conflicts: conflicts.sort((left, right) =>
        left.name.localeCompare(right.name)
      )
    }
  }

  async importSkillsFromCli(targetId, payload) {
    const { imports, mounts } = await this.collectCliSkillImports(targetId)
    const allCandidates = [...imports, ...mounts]
    const selectedSources = new Set()
    const replacementSources = new Map()

    if (Array.isArray(payload)) {
      for (const name of payload) {
        for (const candidate of allCandidates.filter(
          (item) => item.name === name
        )) {
          selectedSources.add(candidate.sourcePath)
        }
      }
    } else if (payload?.sourcePaths) {
      for (const sourcePath of payload.sourcePaths) {
        selectedSources.add(sourcePath)
      }

      for (const choice of payload.choices || []) {
        if (choice.id.startsWith("managed:")) {
          for (const candidate of mounts.filter(
            (item) => item.name === choice.name
          )) {
            selectedSources.add(candidate.sourcePath)
          }

          continue
        }

        for (const sourcePath of choice.sourcePaths || [choice.id]) {
          const selected = allCandidates.find(
            (item) => item.sourcePath === sourcePath
          )

          if (selected) {
            selectedSources.add(selected.sourcePath)
            replacementSources.set(selected.name, selected.sourcePath)
          }
        }
      }
    } else {
      for (const candidate of allCandidates) {
        selectedSources.add(candidate.sourcePath)
      }
    }

    const selectedImports = imports.filter((item) =>
      selectedSources.has(item.sourcePath)
    )
    const selectedMounts = mounts.filter((item) =>
      selectedSources.has(item.sourcePath)
    )

    if (!selectedImports.length && !selectedMounts.length) {
      await this.refreshAll()
      return
    }

    for (const [skillName, sourcePath] of replacementSources) {
      const source = allCandidates.find(
        (item) => item.sourcePath === sourcePath
      )

      if (source) {
        await fs.rm(source.managedPath, { recursive: true, force: true })
        await fs.cp(source.sourcePath, source.managedPath, {
          recursive: true
        })
      }
    }

    for (const candidate of selectedImports) {
      await fs.cp(candidate.sourcePath, candidate.managedPath, {
        recursive: true
      })
    }

    for (const candidate of [...selectedImports, ...selectedMounts]) {
      await fs.rm(candidate.sourcePath, { recursive: true, force: true })
      if (
        path.resolve(candidate.sourcePath) !==
        path.resolve(candidate.mountedPath)
      ) {
        await fs.rm(candidate.mountedPath, { recursive: true, force: true })
      }
      await fs.symlink(candidate.managedPath, candidate.mountedPath, "junction")
    }

    await this.refreshAll()
  }

  async addRepo(input) {
    await this.repoService.addRepo(input)
    this.startWatcher()
    await this.refreshAll()
  }

  async syncRepo(repoId) {
    await this.repoService.syncRepo(repoId)
    await this.refreshAll()
  }

  async syncAllRepos() {
    for (const repo of this.repoService.listRepos()) {
      await this.repoService.syncRepo(repo.id)
    }

    await this.refreshAll()
  }

  async removeRepo(repoId) {
    const repo = this.repoService.listRepos().find((item) => item.id === repoId)

    if (!repo) {
      return
    }

    const repoSkills = this.state.skills.filter(
      (item) => item.repoId === repoId
    )

    for (const skill of repoSkills) {
      for (const targetId of skill.installedTargets) {
        await this.linkManager.uninstallSkill(skill.name, targetId)
      }
    }

    await this.repoService.removeRepo(repoId)
    this.startWatcher()
    await this.refreshAll()
  }

  async openFolder(folderPath) {
    return folderPath
  }

  async loadSessionMessages(sessionId) {
    return this.sessionService.loadMessages(sessionId)
  }

  async searchSessions(query) {
    return this.sessionService.search(query)
  }

  getUsageStats(input) {
    return this.usageService.getStats(input)
  }

  async getSkillUsageStats(input = {}) {
    return this.skillUsageService.getStats({
      ...input,
      cliTargets: this.state.cliTargets,
      managedSkills: this.state.skills,
      usageLogs: this.usageService.getStats().data.logs
    })
  }

  getUsagePricing() {
    return {
      status: "ok",
      data: this.usageService.getPricingConfig(),
      message: ""
    }
  }

  saveUsagePricing(input) {
    const pricingConfig = this.usageService.savePricingConfig(input)

    this.state = {
      ...this.state,
      usage: this.usageService.getStats().data,
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)

    return {
      status: "ok",
      data: pricingConfig,
      message: ""
    }
  }

  async syncUsage(input = {}) {
    const { sessions } = await this.sessionService.refresh(this.state.cliTargets)
    const runtimeState = this.getRuntimeStateWithProxy()
    const codexAccounts = this.codexAccountService.getState()
    const { diagnostics: usageDiagnostics } = await this.usageService.refresh({
      sessions,
      providers: runtimeState.providers,
      runtimeProfiles: runtimeState.runtimeProfiles,
      runtimeProviderState: runtimeState.runtimeProviderState,
      codexAccounts,
      proxyStates: this.getProxyStatePatch(),
      codexProxyState: this.codexProxyService.getState()
    })

    this.state = {
      ...this.state,
      sessions,
      usage: this.usageService.getStats().data,
      diagnostics: [
        ...this.state.diagnostics.filter(
          (item) => item.type !== "usage-parse-error"
        ),
        ...usageDiagnostics
      ],
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.usageService.getStats(input)
  }

  async deleteSession(sessionId) {
    await this.sessionService.moveToRecycle(sessionId)
    this.state = {
      ...this.state,
      sessions: this.sessionService.sessions,
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
  }

  async listRecycledSessions() {
    return this.sessionService.listRecycle()
  }

  async restoreSession(sessionId) {
    await this.sessionService.restoreFromRecycle(sessionId)
    const { sessions, diagnostics } = await this.sessionService.refresh(
      this.state.cliTargets
    )
    this.state = {
      ...this.state,
      sessions,
      diagnostics: [
        ...this.state.diagnostics.filter(
          (item) => item.type !== "session-parse-error"
        ),
        ...diagnostics
      ],
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
  }

  async purgeSession(sessionId) {
    await this.sessionService.purgeFromRecycle(sessionId)
  }

  async saveProvider(input) {
    this.runtimeProviderService.saveProvider(input)
    await this.runtimeProviderService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  persistRules() {
    this.storage.scheduleWrite("rules", [])
  }

  async saveRule(input) {
    await this.promptRuntimeService.savePrompt(input)
    await this.promptRuntimeService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      rules: this.promptRuntimeService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async deleteRule(ruleId) {
    const targetId = String(ruleId || "").trim()

    await this.promptRuntimeService.deletePrompt(targetId)
    this.state = {
      ...this.state,
      rules: this.promptRuntimeService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async toggleRule(input) {
    const rule = this.rules.find((item) => item.id === input.ruleId)

    if (!rule) {
      throw new Error("Rule 不存在")
    }

    return this.saveRule({
      ...rule,
      enabled: input.enabled
    })
  }

  async moveRule(input) {
    const ruleId = String(input.ruleId || "").trim()
    const direction = String(input.direction || "").trim()
    const currentIndex = this.rules.findIndex((item) => item.id === ruleId)

    if (currentIndex < 0) {
      throw new Error("Rule 不存在")
    }

    const nextIndex = direction === "up" ? currentIndex - 1 : currentIndex + 1

    if (nextIndex < 0 || nextIndex >= this.rules.length) {
      return this.state
    }

    const nextRules = [...this.rules]
    const [rule] = nextRules.splice(currentIndex, 1)
    nextRules.splice(nextIndex, 0, rule)
    this.rules = nextRules
    this.persistRules()
    this.state = {
      ...this.state,
      rules: [...this.rules],
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async saveRule(input) {
    await this.promptRuntimeService.savePrompt(input)
    this.state = {
      ...this.state,
      rules: this.promptRuntimeService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async deleteRule(ruleId) {
    await this.promptRuntimeService.deletePrompt(String(ruleId || "").trim())
    await this.promptRuntimeService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      rules: this.promptRuntimeService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async toggleRule(input) {
    if (input.enabled === false) {
      throw new Error("全局 Prompt 必须保持单 active，请切换到其他 Prompt")
    }

    return this.enableRule(input.ruleId)
  }

  async enableRule(ruleId) {
    const prompt = this.promptRuntimeService.prompts.find(
      (item) => item.id === ruleId
    )

    if (!prompt) {
      throw new Error("Prompt 不存在")
    }

    await this.promptRuntimeService.enablePrompt(
      prompt.id,
      this.state.cliTargets.find((item) => item.id === prompt.cli)
    )
    this.state = {
      ...this.state,
      rules: this.promptRuntimeService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async moveRule() {
    return this.state
  }

  async importRule(input) {
    await this.promptRuntimeService.importGlobalPrompt(
      input,
      this.state.cliTargets.find((item) => item.id === input.cli)
    )
    await this.promptRuntimeService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      rules: this.promptRuntimeService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async previewImportRule(input) {
    return this.promptRuntimeService.previewImportGlobalPrompt(
      input,
      this.state.cliTargets.find((item) => item.id === input.cli)
    )
  }

  async resolveRuleImportConflict(input) {
    await this.promptRuntimeService.resolveImportConflict(input)
    await this.promptRuntimeService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      rules: this.promptRuntimeService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async compareRule(input) {
    return this.promptRuntimeService.comparePrompt(
      input.ruleId,
      this.state.cliTargets.find((item) => item.id === input.cli)
    )
  }

  async resolveRuleDrift(input) {
    await this.promptRuntimeService.resolveDrift(
      input,
      this.state.cliTargets.find((item) => item.id === input.cli)
    )
    this.state = {
      ...this.state,
      rules: this.promptRuntimeService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async saveRule(input) {
    await this.promptRuntimeService.savePrompt(input)
    const prompt = this.promptRuntimeService.prompts.find(
      (item) => item.id === input.id
    )

    if (
      prompt &&
      this.promptRuntimeService.profiles[prompt.cli]?.activePromptId ===
        prompt.id
    ) {
      await this.promptRuntimeService.enablePrompt(
        prompt.id,
        this.state.cliTargets.find((item) => item.id === prompt.cli)
      )
    } else {
      await this.promptRuntimeService.refreshDrift(this.state.cliTargets)
    }
    this.state = {
      ...this.state,
      rules: this.promptRuntimeService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async toggleRule(input) {
    if (input.enabled === false) {
      await this.promptRuntimeService.disablePrompt(
        {
          cli: input.cli,
          promptId: input.ruleId
        },
        this.state.cliTargets.find((item) => item.id === input.cli)
      )
      await this.promptRuntimeService.refreshDrift(this.state.cliTargets)
      this.state = {
        ...this.state,
        rules: this.promptRuntimeService.getState(),
        refreshedAt: Date.now()
      }
      this.emit("state-changed", this.state)
      return this.state
    }

    return this.enableRule(input.ruleId)
  }

  async enableRule(ruleId) {
    const prompt = this.promptRuntimeService.prompts.find(
      (item) => item.id === ruleId
    )

    if (!prompt) {
      throw new Error("Prompt 不存在")
    }

    await this.promptRuntimeService.enablePrompt(
      prompt.id,
      this.state.cliTargets.find((item) => item.id === prompt.cli)
    )
    this.state = {
      ...this.state,
      rules: this.promptRuntimeService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async deleteProvider(providerId) {
    this.runtimeProviderService.deleteProvider(providerId)
    await this.runtimeProviderService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async startCodexOfficialLogin(input) {
    const result = await this.codexAccountService.startLogin(input)
    this.state = {
      ...this.state,
      codexAccounts: this.codexAccountService.getState(),
      codexLoginState: this.codexAccountService.getLoginState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return result
  }

  async cancelCodexOfficialLogin() {
    this.codexAccountService.cancelLogin()
    this.state = {
      ...this.state,
      codexLoginState: this.codexAccountService.getLoginState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async importCodexAuthJson(input) {
    await this.codexAccountService.importAuthJson(input)
    this.state = {
      ...this.state,
      codexAccounts: this.codexAccountService.getState(),
      codexLoginState: this.codexAccountService.getLoginState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async enableCodexAccount(input) {
    if (this.codexProxyService.isEnabled()) {
      throw new Error("请先关闭 Codex 代理接管")
    }

    await this.codexAccountService.enableAccount(
      input.accountId,
      this.state.cliTargets.find((item) => item.id === "codex")
    )
    this.runtimeProviderService.clearRuntime("codex")
    await this.runtimeProviderService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      codexAccounts: this.codexAccountService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async clearCodexAccount() {
    this.codexAccountService.clearActiveAccount()
    this.state = {
      ...this.state,
      codexAccounts: this.codexAccountService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async refreshCodexAccount(input) {
    await this.codexAccountService.refreshAccountUsage(
      input.accountId,
      this.state.cliTargets.find((item) => item.id === "codex"),
      {
        syncAuth: input.syncAuth !== false
      }
    )
    this.state = {
      ...this.state,
      codexAccounts: this.codexAccountService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async disableCodexAccount(input) {
    this.codexAccountService.disableAccount(input.accountId)
    await this.runtimeProviderService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      codexAccounts: this.codexAccountService.getState(),
      ...this.getProxyStatePatch(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async restoreCodexAccount(input) {
    this.codexAccountService.restoreAccount(input.accountId)
    await this.runtimeProviderService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      codexAccounts: this.codexAccountService.getState(),
      ...this.getProxyStatePatch(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async updateCodexAccountProxy(input) {
    this.codexAccountService.updateAccountProxy(input.accountId, input.proxy)
    this.state = {
      ...this.state,
      codexAccounts: this.codexAccountService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async deleteCodexAccount(input) {
    await this.codexAccountService.deleteAccount(
      input.accountId,
      this.state.cliTargets.find((item) => item.id === "codex")
    )
    this.state = {
      ...this.state,
      codexAccounts: this.codexAccountService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async getCodexAccountDetail(input) {
    const account = await this.codexAccountService.getAccountDetail(
      input.accountId,
      this.state.cliTargets.find((item) => item.id === "codex")
    )

    return {
      status: "ok",
      data: account,
      message: ""
    }
  }

  async enableProxy(cli, input) {
    const proxyService = this.getProxyService(cli)
    const cliTarget = this.state.cliTargets.find((item) => item.id === cli)

    if (!proxyService) {
      throw new Error("该 CLI 不支持代理接管")
    }

    const targetId =
      proxyService.getState().failoverProviderIds[0] || ""
    const previousProfile =
      this.runtimeProviderService.profiles.find((item) => item.cli === cli) ||
      null
    const previousAccountId =
      cli === "codex" ? this.codexAccountService.activeAccountId || "" : ""

    if (!targetId) {
      throw new Error("请先把目标加入代理接管池")
    }

    await proxyService.enable(
      {
        ...input,
        previousAccountId,
        previousProfile
      },
      cliTarget
    )
    this.runtimeProviderService.clearRuntime(cli)
    if (cli === "codex") this.codexAccountService.clearActiveAccount()
    await this.runtimeProviderService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      codexAccounts: this.codexAccountService.getState(),
      ...this.getProxyStatePatch(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async disableProxy(cli) {
    const proxyService = this.getProxyService(cli)

    if (!proxyService) {
      throw new Error("该 CLI 不支持代理接管")
    }

    const result = await proxyService.disable(
      this.state.cliTargets.find((item) => item.id === cli)
    )
    if (cli === "codex" && result.previousAccountId) {
      await this.codexAccountService.enableAccount(
        result.previousAccountId,
        this.state.cliTargets.find((item) => item.id === "codex")
      )
      this.runtimeProviderService.clearRuntime("codex")
    } else if (result.previousProfile) {
      this.runtimeProviderService.switchRuntime(result.previousProfile)
    } else {
      this.runtimeProviderService.clearRuntime(cli)
    }
    await this.runtimeProviderService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      codexAccounts: this.codexAccountService.getState(),
      ...this.getProxyStatePatch(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async addProxyProvider(cli, input) {
    const proxyService = this.getProxyService(cli)

    if (!proxyService) {
      throw new Error("该 CLI 不支持代理接管")
    }

    await proxyService.addProvider(input)
    this.state = {
      ...this.state,
      [this.getProxyStateKey(cli)]: proxyService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async removeProxyProvider(cli, input) {
    const proxyService = this.getProxyService(cli)

    if (!proxyService) {
      throw new Error("该 CLI 不支持代理接管")
    }

    await proxyService.removeProvider(input)
    this.state = {
      ...this.state,
      [this.getProxyStateKey(cli)]: proxyService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async activateProxyProvider(cli, input) {
    const proxyService = this.getProxyService(cli)

    if (!proxyService) {
      throw new Error("该 CLI 不支持代理接管")
    }

    const targetId = input.accountId
      ? `account:${String(input.accountId || "").trim()}`
      : String(input.providerId || "").trim()

    await proxyService.updateActiveProvider(
      targetId,
      this.state.cliTargets.find((item) => item.id === cli)
    )
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      [this.getProxyStateKey(cli)]: proxyService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async updateCodexProxyAccountModel(input) {
    await this.codexProxyService.updateAccountModel(
      input,
      this.state.cliTargets.find((item) => item.id === "codex")
    )
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      codexProxyState: this.codexProxyService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async enableClaudeProxy(input) {
    return this.enableProxy("claude", input)
  }

  async disableClaudeProxy() {
    return this.disableProxy("claude")
  }

  async addClaudeProxyProvider(input) {
    return this.addProxyProvider("claude", input)
  }

  async removeClaudeProxyProvider(input) {
    return this.removeProxyProvider("claude", input)
  }

  async activateClaudeProxyProvider(input) {
    return this.activateProxyProvider("claude", input)
  }

  async enableCodexProxy(input) {
    return this.enableProxy("codex", input)
  }

  async disableCodexProxy() {
    return this.disableProxy("codex")
  }

  async addCodexProxyProvider(input) {
    return this.addProxyProvider("codex", input)
  }

  async removeCodexProxyProvider(input) {
    return this.removeProxyProvider("codex", input)
  }

  async activateCodexProxyProvider(input) {
    return this.activateProxyProvider("codex", input)
  }

  async saveCodexProxyAccountModel(input) {
    return this.updateCodexProxyAccountModel(input)
  }

  async saveRuntimeModel(input) {
    this.runtimeProviderService.saveModel(input)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async switchRuntime(input) {
    const proxyService = this.getProxyService(input.cli)

    if (proxyService?.isEnabled()) {
      throw new Error(`请先关闭 ${proxyService.cliName} 代理接管`)
    }

    this.runtimeProviderService.switchRuntime(input)
    await this.runtimeProviderService.writeCliConfig(
      input.cli,
      this.state.cliTargets.find((item) => item.id === input.cli)
    )
    if (input.cli === "codex") this.codexAccountService.clearActiveAccount()
    await this.runtimeProviderService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      codexAccounts: this.codexAccountService.getState(),
      ...this.getProxyStatePatch(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async clearRuntime(cli) {
    const proxyService = this.getProxyService(cli)

    if (proxyService?.isEnabled()) {
      throw new Error(`请先关闭 ${proxyService.cliName} 代理接管`)
    }

    this.runtimeProviderService.clearRuntime(cli)
    await this.runtimeProviderService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  buildRuntimeEnv(cli) {
    return this.runtimeProviderService.buildRuntimeEnv(cli)
  }

  async compareRuntime(input) {
    return this.runtimeProviderService.compareRuntime(
      input.cli,
      this.state.cliTargets.find((item) => item.id === input.cli)
    )
  }

  async getRuntimeConfig(input) {
    return this.runtimeProviderService.getRuntimeConfig(
      input.cli,
      this.state.cliTargets.find((item) => item.id === input.cli)
    )
  }

  async resolveRuntimeDrift(input) {
    await this.runtimeProviderService.resolveDrift(
      input,
      this.state.cliTargets.find((item) => item.id === input.cli)
    )
    await this.runtimeProviderService.refreshDrift(this.state.cliTargets)
    this.state = {
      ...this.state,
      ...this.getRuntimeStateWithProxy(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async dispose() {
    this.codexAccountService.stopAutoRefresh()
    this.fileWatcherService.stop()
    await this.claudeProxyService.dispose()
    await this.codexProxyService.dispose()
    await this.sessionService.dispose()
    await this.storage.flush()
  }
}

module.exports = {
  ManagerService
}
