const fs = require('node:fs/promises')
const path = require('node:path')
const crypto = require('node:crypto')
const { execFile } = require('node:child_process')
const { promisify } = require('node:util')
const { slugifyName } = require('./path-utils.cjs')

const execFileAsync = promisify(execFile)

function normalizeRepoInput(input) {
  if (input.type === 'github') {
    const source = String(input.source || '').trim()

    if (!source) {
      throw new Error('GitHub 仓库地址不能为空')
    }

    if (source.startsWith('http://') || source.startsWith('https://')) {
      return source
    }

    return `https://github.com/${source}.git`
  }

  return String(input.source || '').trim()
}

async function directoryExists(targetPath) {
  try {
    const stat = await fs.stat(targetPath)
    return stat.isDirectory()
  } catch {
    return false
  }
}

class RepoService {
  constructor(paths, storage) {
    this.paths = paths
    this.storage = storage
    this.repos = []
  }

  async init() {
    this.repos = await this.storage.read('repos', [])
  }

  listRepos() {
    return this.repos
  }

  async persist() {
    this.storage.scheduleWrite('repos', this.repos)
  }

  async addRepo(input) {
    const type = ['github', 'git', 'local'].includes(input.type) ? input.type : 'local'
    const source = normalizeRepoInput({ ...input, type })
    const name = String(input.name || '').trim() || path.basename(source.replace(/\.git$/i, ''))
    const repoId = crypto.randomUUID()
    let localPath = source

    if (!source) {
      throw new Error('仓库来源不能为空')
    }

    if (type === 'local') {
      if (!(await directoryExists(source))) {
        throw new Error(`本地仓库目录不存在：${source}`)
      }
    } else {
      const targetDir = path.join(
        this.paths.reposDir,
        `${slugifyName(name) || 'repo'}-${repoId.slice(0, 6)}`
      )

      await execFileAsync('git', ['clone', source, targetDir], {
        windowsHide: true
      })
      localPath = targetDir
    }

    const repo = {
      id: repoId,
      name,
      type,
      source,
      localPath,
      createdAt: Date.now(),
      updatedAt: Date.now(),
      lastSyncedAt: Date.now(),
      status: 'ready'
    }

    this.repos = [repo, ...this.repos]
    await this.persist()

    return repo
  }

  async syncRepo(repoId) {
    const repo = this.repos.find(item => item.id === repoId)

    if (!repo) {
      throw new Error('仓库不存在')
    }

    if (repo.type !== 'local') {
      await execFileAsync('git', ['-C', repo.localPath, 'pull'], {
        windowsHide: true
      })
    }

    repo.updatedAt = Date.now()
    repo.lastSyncedAt = Date.now()
    repo.status = 'ready'
    await this.persist()

    return repo
  }

  async removeRepo(repoId) {
    const repo = this.repos.find(item => item.id === repoId)

    if (!repo) {
      return null
    }

    this.repos = this.repos.filter(item => item.id !== repoId)
    await this.persist()

    if (repo.type !== 'local') {
      await fs.rm(repo.localPath, { recursive: true, force: true })
    }

    return repo
  }
}

module.exports = {
  RepoService
}
