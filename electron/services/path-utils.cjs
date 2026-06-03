const fs = require("node:fs/promises")
const os = require("node:os")
const path = require("node:path")
const usernamePlaceholder = "%USERNAME%"

function getPortableHomePrefix() {
  return path.join(path.dirname(os.homedir()), usernamePlaceholder)
}

function normalizeForCompare(value) {
  return process.platform === "win32" ? value.toLowerCase() : value
}

function serializePortablePath(value) {
  const text = String(value || "").trim()

  if (!text) {
    return ""
  }

  const actualPrefix = path.normalize(os.homedir())
  const portablePrefix = path.normalize(getPortableHomePrefix())
  const normalizedText = path.normalize(text)

  if (normalizeForCompare(normalizedText) === normalizeForCompare(actualPrefix)) {
    return portablePrefix
  }

  if (
    normalizeForCompare(normalizedText).startsWith(
      `${normalizeForCompare(actualPrefix)}${path.sep}`
    )
  ) {
    return path.join(portablePrefix, normalizedText.slice(actualPrefix.length + 1))
  }

  return text
}

function resolvePortablePath(value) {
  const text = String(value || "").trim()

  if (!text) {
    return ""
  }

  const actualPrefix = path.normalize(os.homedir())
  const portablePrefix = path.normalize(getPortableHomePrefix())
  const normalizedText = path.normalize(text)

  if (
    normalizeForCompare(normalizedText) ===
    normalizeForCompare(portablePrefix)
  ) {
    return actualPrefix
  }

  if (
    normalizeForCompare(normalizedText).startsWith(
      `${normalizeForCompare(portablePrefix)}${path.sep}`
    )
  ) {
    return path.join(actualPrefix, normalizedText.slice(portablePrefix.length + 1))
  }

  return text
}

function serializeAppSettingsPaths(input = {}) {
  return {
    ...input,
    dataPath: serializePortablePath(input.dataPath),
    cliConfigPaths: {
      claude: serializePortablePath(input.cliConfigPaths?.claude),
      codex: serializePortablePath(input.cliConfigPaths?.codex)
      // 当前版本暂不启用 Gemini。
      // gemini: serializePortablePath(input.cliConfigPaths?.gemini)
    },
    defaultCliConfigPaths: {
      claude: serializePortablePath(input.defaultCliConfigPaths?.claude),
      codex: serializePortablePath(input.defaultCliConfigPaths?.codex)
      // 当前版本暂不启用 Gemini。
      // gemini: serializePortablePath(input.defaultCliConfigPaths?.gemini)
    }
  }
}

function resolveAppPaths(userDataPath) {
  const workspaceRoot = path.join(userDataPath, "workspace")
  const storageDir = path.join(workspaceRoot, "storage")
  const sessionsDir = path.join(workspaceRoot, "sessions")
  const sessionRecycleDir = path.join(sessionsDir, "recycle")
  const promptsDir = path.join(workspaceRoot, "prompts")
  const profilesDir = path.join(workspaceRoot, "profiles")

  return {
    userDataPath,
    workspaceRoot,
    skillsDir: path.join(workspaceRoot, "skills"),
    promptsDir,
    promptProfilesDir: profilesDir,
    reposDir: path.join(workspaceRoot, "repos"),
    sessionsDir,
    sessionRecycleDir,
    sessionRecycleSessionsDir: path.join(sessionRecycleDir, "sessions"),
    sessionRecycleMetadataDir: path.join(sessionRecycleDir, "metadata"),
    storageDir,
    storageFiles: {
      skillRepositories: path.join(storageDir, "skill-repositories.json"),
      repos: path.join(storageDir, "repos.json"),
      skills: path.join(storageDir, "skills.json"),
      installs: path.join(storageDir, "installs.json"),
      cliTargets: path.join(storageDir, "cli-targets.json"),
      sessions: path.join(storageDir, "sessions.json"),
      usageLogs: path.join(storageDir, "usage-logs.json"),
      usageRequestRecords: path.join(storageDir, "usage-request-records.json"),
      usagePricing: path.join(storageDir, "usage-pricing.json"),
      providers: path.join(storageDir, "providers.json"),
      rules: path.join(storageDir, "rules.json"),
      promptRuntimeState: path.join(storageDir, "prompt-runtime-state.json"),
      runtimeModels: path.join(storageDir, "runtime-models.json"),
      runtimeProfiles: path.join(storageDir, "runtime-profiles.json"),
      runtimeProviderState: path.join(storageDir, "runtime-provider-state.json"),
      runtimeProviderKeys: path.join(storageDir, "runtime-provider-keys.json"),
      claudeProxyConfig: path.join(storageDir, "claude-proxy-config.json"),
      claudeProxyLiveBackup: path.join(
        storageDir,
        "claude-proxy-live-backup.json"
      ),
      claudeProxyRequestLogs: path.join(
        storageDir,
        "claude-proxy-request-logs.json"
      ),
      codexProxyConfig: path.join(storageDir, "codex-proxy-config.json"),
      codexProxyLiveBackup: path.join(
        storageDir,
        "codex-proxy-live-backup.json"
      ),
      codexProxyRequestLogs: path.join(
        storageDir,
        "codex-proxy-request-logs.json"
      ),
      codexAccounts: path.join(storageDir, "codex-accounts.json"),
      codexActiveAccountId: path.join(
        storageDir,
        "codex-active-account-id.json"
      )
    }
  }
}

async function ensureAppDirectories(paths) {
  await Promise.all([
    fs.mkdir(paths.workspaceRoot, { recursive: true }),
    fs.mkdir(paths.skillsDir, { recursive: true }),
    fs.mkdir(paths.promptsDir, { recursive: true }),
    fs.mkdir(path.join(paths.promptsDir, "claude"), { recursive: true }),
    fs.mkdir(path.join(paths.promptsDir, "codex"), { recursive: true }),
    fs.mkdir(paths.promptProfilesDir, { recursive: true }),
    fs.mkdir(paths.reposDir, { recursive: true }),
    fs.mkdir(paths.sessionRecycleSessionsDir, { recursive: true }),
    fs.mkdir(paths.sessionRecycleMetadataDir, { recursive: true }),
    fs.mkdir(paths.storageDir, { recursive: true })
  ])
}

function slugifyName(value) {
  return String(value || "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
}

module.exports = {
  ensureAppDirectories,
  resolvePortablePath,
  resolveAppPaths,
  serializeAppSettingsPaths,
  serializePortablePath,
  slugifyName
}
