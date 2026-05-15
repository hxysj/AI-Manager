const fs = require("node:fs/promises")
const path = require("node:path")

function resolveAppPaths(userDataPath) {
  const workspaceRoot = path.join(userDataPath, "workspace")
  const storageDir = path.join(workspaceRoot, "storage")
  const sessionsDir = path.join(workspaceRoot, "sessions")
  const sessionRecycleDir = path.join(sessionsDir, "recycle")

  return {
    userDataPath,
    workspaceRoot,
    skillsDir: path.join(workspaceRoot, "skills"),
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
      runtimeModels: path.join(storageDir, "runtime-models.json"),
      runtimeProfiles: path.join(storageDir, "runtime-profiles.json"),
      runtimeProviderKeys: path.join(storageDir, "runtime-provider-keys.json")
    }
  }
}

async function ensureAppDirectories(paths) {
  await Promise.all([
    fs.mkdir(paths.workspaceRoot, { recursive: true }),
    fs.mkdir(paths.skillsDir, { recursive: true }),
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
