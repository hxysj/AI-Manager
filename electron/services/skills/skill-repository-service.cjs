const fs = require("node:fs/promises")
const path = require("node:path")
const crypto = require("node:crypto")
const matter = require("gray-matter")
const { slugifyName } = require("../path-utils.cjs")

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath)
    return true
  } catch {
    return false
  }
}

function normalizePathSegment(value) {
  return String(value || "")
    .split("/")
    .map(item => encodeURIComponent(item))
    .join("/")
}

function normalizeRepositoryPath(value) {
  return String(value || "")
    .replace(/\\/g, "/")
    .replace(/^\/+/, "")
    .replace(/\/+$/, "")
}

function parseGitHubSource(value) {
  let source = String(value || "").trim()

  if (!source) {
    throw new Error("仓库地址不能为空")
  }

  if (!source.includes("://") && !source.startsWith("github.com/")) {
    source = `github.com/${source}`
  }

  const url = new URL(source.startsWith("http") ? source : `https://${source}`)

  if (url.hostname !== "github.com") {
    throw new Error("当前只支持 GitHub 仓库地址")
  }

  const segments = url.pathname
    .split("/")
    .map(item => item.trim())
    .filter(Boolean)
  const owner = segments[0]
  const repository = String(segments[1] || "").replace(/\.git$/i, "")

  if (!owner || !repository) {
    throw new Error("GitHub 仓库地址格式不正确")
  }

  let branch = ""
  let rootPath = ""

  if (segments[2] === "tree") {
    branch = segments[3] || ""
    rootPath = segments.slice(4).join("/")
  }

  if (segments[2] === "blob") {
    branch = segments[3] || ""
    const filePath = segments.slice(4).join("/")
    rootPath =
      path.posix.basename(filePath) === "SKILL.md"
        ? path.posix.dirname(filePath)
        : filePath

    if (rootPath === ".") {
      rootPath = ""
    }
  }

  return {
    owner,
    repository,
    branch,
    rootPath: normalizeRepositoryPath(rootPath),
    source: url.href.replace(/\/$/, ""),
    htmlUrl: `https://github.com/${owner}/${repository}`
  }
}

function parseJsonText(text) {
  return String(text || "").trim() ? JSON.parse(text) : null
}

async function fetchJson(url) {
  const response = await fetch(url, {
    headers: {
      Accept: "application/vnd.github+json"
    }
  })
  const text = await response.text()

  if (!response.ok) {
    const payload = parseJsonText(text)
    throw new Error(
      `GitHub 请求失败：${response.status} ${payload?.message || url}`
    )
  }

  return parseJsonText(text)
}

async function downloadFile(url, targetPath) {
  const response = await fetch(url)

  if (!response.ok) {
    throw new Error(`GitHub 下载失败：${response.status} ${url}`)
  }

  await fs.writeFile(targetPath, Buffer.from(await response.arrayBuffer()))
}

function createRawFileUrl(repository, ref, filePath) {
  return (
    `https://raw.githubusercontent.com/${repository.owner}` +
    `/${repository.repository}/${normalizePathSegment(ref)}` +
    `/${normalizePathSegment(filePath)}`
  )
}

async function fetchRawFile(repository, ref, filePath) {
  const response = await fetch(createRawFileUrl(repository, ref, filePath))

  if (!response.ok) {
    throw new Error(`GitHub 文件读取失败：${response.status} ${filePath}`)
  }

  return response.text()
}

async function resolveRepositoryRef(repository) {
  if (repository.branch) {
    return repository.branch
  }

  const data = await fetchJson(
    `https://api.github.com/repos/${repository.owner}/${repository.repository}`
  )
  const branch = String(data?.default_branch || "").trim()

  if (!branch) {
    throw new Error("GitHub 仓库默认分支不存在")
  }

  return branch
}

function parseSkillContent(content, skillFilePath) {
  const parsed = matter(content)
  const metadata = parsed.data || {}
  const skillName = String(metadata.name || "").trim()

  if (!skillName) {
    throw new Error(`Missing required frontmatter field "name" in ${skillFilePath}`)
  }

  return {
    name: skillName,
    description: metadata.description
      ? String(metadata.description).trim()
      : "",
    version: metadata.version ? String(metadata.version).trim() : "",
    author: metadata.author ? String(metadata.author).trim() : "",
    tags: Array.isArray(metadata.tags)
      ? metadata.tags.map(item => String(item).trim()).filter(Boolean)
      : [],
    entry: String(metadata.entry || "SKILL.md").trim() || "SKILL.md",
    homepage: metadata.homepage ? String(metadata.homepage).trim() : "",
    repository: metadata.repository ? String(metadata.repository).trim() : ""
  }
}

function isPathInsideDirectory(filePath, directoryPath) {
  if (!directoryPath) {
    return true
  }

  return filePath === directoryPath || filePath.startsWith(`${directoryPath}/`)
}

function createRepositoryStorageItem(repository) {
  return {
    id: repository.id,
    type: repository.type,
    name: repository.name,
    source: repository.source,
    owner: repository.owner,
    repository: repository.repository,
    branch: repository.branch,
    rootPath: repository.rootPath,
    htmlUrl: repository.htmlUrl,
    createdAt: repository.createdAt,
    updatedAt: repository.updatedAt
  }
}

function createRepositoryRuntimeItem(repository) {
  return {
    ...createRepositoryStorageItem(repository),
    status: "ready",
    skills: [],
    error: "",
    lastSyncedAt: 0
  }
}

function applyRepositoryCache(repository, cache) {
  if (!cache) {
    return repository
  }

  return {
    ...repository,
    status: cache.status || "ready",
    skills: Array.isArray(cache.skills) ? cache.skills : [],
    error: cache.error || "",
    lastSyncedAt: Number(cache.lastSyncedAt || 0),
    updatedAt: Number(cache.updatedAt || repository.updatedAt || 0)
  }
}

class SkillRepositoryService {
  constructor(paths, storage) {
    this.paths = paths
    this.storage = storage
    this.repositories = []
  }

  async init() {
    const caches = await this.storage.read("skillRepositoryCache", [])
    const cacheMap = new Map(caches.map(item => [item.id, item]))

    this.repositories = (await this.storage.read("skillRepositories", [])).map(
      item => applyRepositoryCache(createRepositoryRuntimeItem(item), cacheMap.get(item.id))
    )

  }

  listRepositories() {
    return this.repositories
  }

  async persist() {
    this.storage.scheduleWrite(
      "skillRepositories",
      this.repositories.map(item => createRepositoryStorageItem(item))
    )
    this.storage.scheduleWrite(
      "skillRepositoryCache",
      this.repositories.map(item => ({
        id: item.id,
        status: item.status,
        skills: item.skills,
        error: item.error,
        lastSyncedAt: item.lastSyncedAt,
        updatedAt: item.updatedAt
      }))
    )
  }

  async refreshRepositories() {
    for (const repository of this.repositories) {
      try {
        const scanned = await this.scanRepository(repository)

        repository.branch = scanned.branch
        repository.skills = scanned.skills
        repository.status = "ready"
        repository.error = ""
      } catch (error) {
        repository.status = "error"
        repository.skills = []
        repository.error = error.message || String(error)
      }

      repository.lastSyncedAt = Date.now()
      repository.updatedAt = Date.now()
    }

    await this.persist()
  }

  async addRepository(input) {
    const github = parseGitHubSource(input?.source)
    const branch = String(input?.branch || github.branch || "").trim()
    const name =
      String(input?.name || "").trim() ||
      `${github.owner}/${github.repository}`
    const repository = {
      id: `skill-repo-${crypto.randomUUID()}`,
      type: "github",
      name,
      source: github.source,
      owner: github.owner,
      repository: github.repository,
      branch,
      rootPath: github.rootPath,
      htmlUrl: github.htmlUrl,
      status: "ready",
      skills: [],
      error: "",
      createdAt: Date.now(),
      updatedAt: Date.now(),
      lastSyncedAt: 0
    }

    try {
      const scanned = await this.scanRepository(repository)

      repository.branch = scanned.branch
      repository.skills = scanned.skills
      repository.status = "ready"
      repository.error = ""
    } catch (error) {
      repository.status = "error"
      repository.error = error.message || String(error)
    }

    repository.lastSyncedAt = Date.now()
    repository.updatedAt = Date.now()
    this.repositories = [repository, ...this.repositories]
    await this.persist()

    return repository
  }

  async refreshRepository(repositoryId) {
    const repository = this.repositories.find(item => item.id === repositoryId)

    if (!repository) {
      throw new Error("Skill 仓库不存在")
    }

    try {
      const scanned = await this.scanRepository(repository)

      repository.branch = scanned.branch
      repository.skills = scanned.skills
      repository.status = "ready"
      repository.error = ""
    } catch (error) {
      repository.status = "error"
      repository.skills = []
      repository.error = error.message || String(error)
    }

    repository.lastSyncedAt = Date.now()
    repository.updatedAt = Date.now()
    await this.persist()

    return repository
  }

  async removeRepository(repositoryId) {
    this.repositories = this.repositories.filter(item => item.id !== repositoryId)
    await this.persist()
  }

  findRepositorySkill(repositoryId, skillId) {
    const repository = this.repositories.find(item => item.id === repositoryId)

    if (!repository) {
      throw new Error("Skill 仓库不存在")
    }

    const skill = repository.skills.find(item => item.id === skillId)

    if (!skill) {
      throw new Error("仓库 Skill 不存在")
    }

    return { repository, skill }
  }

  async installSkill(repositoryId, skillId) {
    const { repository, skill } = this.findRepositorySkill(repositoryId, skillId)
    const treeInfo = await this.fetchRepositoryTree(repository)
    const directoryName = slugifyName(skill.name) || path.basename(skill.skillPath)
    const managedPath = path.join(this.paths.skillsDir, directoryName)
    const files = treeInfo.tree.filter(item => {
      return item.type === "blob" && isPathInsideDirectory(item.path, skill.skillPath)
    })

    if (await pathExists(managedPath)) {
      throw new Error(`集中目录已存在同名目录：${managedPath}`)
    }

    if (!files.length) {
      throw new Error("仓库 Skill 目录下没有可下载文件")
    }

    for (const file of files) {
      const relativePath = skill.skillPath
        ? path.posix.relative(skill.skillPath, file.path)
        : file.path
      const targetPath = path.join(managedPath, relativePath)

      await fs.mkdir(path.dirname(targetPath), { recursive: true })
      await downloadFile(
        createRawFileUrl(repository, treeInfo.ref, file.path),
        targetPath
      )
    }
  }

  async scanRepository(repository) {
    const treeInfo = await this.fetchRepositoryTree(repository)
    const rootPath = normalizeRepositoryPath(repository.rootPath)
    const rootExists =
      !rootPath ||
      treeInfo.tree.some(item => isPathInsideDirectory(item.path, rootPath))

    if (!rootExists) {
      throw new Error("仓库链接下的目录不存在")
    }

    const skillFiles = treeInfo.tree
      .filter(item => item.type === "blob")
      .filter(item => path.posix.basename(item.path) === "SKILL.md")
      .filter(item => isPathInsideDirectory(item.path, rootPath))
    const skills = []
    const batchSize = 4

    for (let index = 0; index < skillFiles.length; index += batchSize) {
      const batch = skillFiles.slice(index, index + batchSize)
      const parsedSkills = await Promise.all(
        batch.map(async skillFile => {
          const skillPath = path.posix.dirname(skillFile.path)
          const normalizedSkillPath = skillPath === "." ? "" : skillPath
          const content = await fetchRawFile(
            repository,
            treeInfo.ref,
            skillFile.path
          )
          const parsed = parseSkillContent(content, skillFile.path)

          return {
            id: crypto
              .createHash("sha1")
              .update(`${repository.source}:${treeInfo.ref}:${normalizedSkillPath}`)
              .digest("hex")
              .slice(0, 16),
            name: parsed.name,
            description: parsed.description,
            version: parsed.version,
            author: parsed.author,
            tags: parsed.tags,
            entry: parsed.entry,
            homepage: parsed.homepage,
            repository: parsed.repository,
            skillPath: normalizedSkillPath,
            displayPath:
              rootPath
                ? path.posix.relative(rootPath, normalizedSkillPath) || "."
                : normalizedSkillPath || ".",
            updatedAt: Date.now()
          }
        })
      )

      skills.push(...parsedSkills)
    }

    return {
      branch: treeInfo.ref,
      skills: skills.sort((left, right) => left.name.localeCompare(right.name))
    }
  }

  async fetchRepositoryTree(repository) {
    const ref = await resolveRepositoryRef(repository)
    const data = await fetchJson(
      `https://api.github.com/repos/${repository.owner}` +
        `/${repository.repository}/git/trees/${normalizePathSegment(ref)}?recursive=1`
    )
    const tree = Array.isArray(data?.tree) ? data.tree : []

    if (data?.truncated) {
      throw new Error("GitHub 返回的仓库文件树已截断，无法完整扫描")
    }

    return {
      ref,
      tree
    }
  }
}

module.exports = {
  SkillRepositoryService
}
