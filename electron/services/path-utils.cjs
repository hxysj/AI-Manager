const fs = require('node:fs/promises')
const path = require('node:path')

function resolveAppPaths(userDataPath) {
  const workspaceRoot = path.join(userDataPath, 'workspace')
  const storageDir = path.join(workspaceRoot, 'storage')

  return {
    userDataPath,
    workspaceRoot,
    skillsDir: path.join(workspaceRoot, 'skills'),
    reposDir: path.join(workspaceRoot, 'repos'),
    storageDir,
    storageFiles: {
      repos: path.join(storageDir, 'repos.json'),
      skills: path.join(storageDir, 'skills.json'),
      installs: path.join(storageDir, 'installs.json'),
      cliTargets: path.join(storageDir, 'cli-targets.json')
    }
  }
}

async function ensureAppDirectories(paths) {
  await Promise.all([
    fs.mkdir(paths.workspaceRoot, { recursive: true }),
    fs.mkdir(paths.skillsDir, { recursive: true }),
    fs.mkdir(paths.reposDir, { recursive: true }),
    fs.mkdir(paths.storageDir, { recursive: true })
  ])
}

function slugifyName(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
}

module.exports = {
  ensureAppDirectories,
  resolveAppPaths,
  slugifyName
}
