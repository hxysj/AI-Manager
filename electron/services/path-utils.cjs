const fs = require("node:fs/promises")
const path = require("node:path")

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
      repos: path.join(storageDir, "repos.json"),
      skills: path.join(storageDir, "skills.json"),
      installs: path.join(storageDir, "installs.json"),
      cliTargets: path.join(storageDir, "cli-targets.json"),
      sessions: path.join(storageDir, "sessions.json"),
      providers: path.join(storageDir, "providers.json"),
      rules: path.join(storageDir, "rules.json"),
      promptRuntimeState: path.join(storageDir, "prompt-runtime-state.json"),
      runtimeModels: path.join(storageDir, "runtime-models.json"),
      runtimeProfiles: path.join(storageDir, "runtime-profiles.json"),
      runtimeProviderState: path.join(storageDir, "runtime-provider-state.json"),
      runtimeProviderKeys: path.join(storageDir, "runtime-provider-keys.json"),
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
  resolveAppPaths,
  slugifyName
}
