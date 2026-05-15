const fs = require("node:fs/promises")
const path = require("node:path")
const { EventEmitter } = require("node:events")
const {
  resolveAppPaths,
  ensureAppDirectories,
  slugifyName
} = require("./path-utils.cjs")
const { JsonStorage } = require("./json-storage.cjs")
const { CliDetectionService } = require("./cli-detection-service.cjs")
const { MetadataParser } = require("./metadata-parser.cjs")
const { SkillScanner } = require("./skill-scanner.cjs")
const { LinkManager } = require("./link-manager.cjs")
const { RepoService } = require("./repo-service.cjs")
const { FileWatcherService } = require("./file-watcher-service.cjs")
const { SessionService } = require("./session-service.cjs")

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

class ManagerService extends EventEmitter {
  constructor(userDataPath) {
    super()
    this.paths = resolveAppPaths(userDataPath)
    this.storage = new JsonStorage(this.paths.storageFiles)
    this.cliDetectionService = new CliDetectionService()
    this.metadataParser = new MetadataParser()
    this.skillScanner = new SkillScanner()
    this.linkManager = new LinkManager(this.cliDetectionService)
    this.repoService = new RepoService(this.paths, this.storage)
    this.fileWatcherService = new FileWatcherService()
    this.sessionService = new SessionService(this.paths)
    this.sessionService.bindStorage(this.storage)
    this.state = {
      cliTargets: [],
      skills: [],
      repos: [],
      sessions: [],
      diagnostics: [],
      paths: this.toPublicPaths(),
      refreshedAt: 0
    }
  }

  async init() {
    await ensureAppDirectories(this.paths)
    await this.repoService.init()
    await this.sessionService.init()
    await this.refreshAll({ emit: false })
    this.startWatcher()
    this.startSessionWatcher()
  }

  toPublicPaths() {
    return {
      workspaceRoot: this.paths.workspaceRoot,
      skillsDir: this.paths.skillsDir,
      reposDir: this.paths.reposDir,
      sessionRecycleDir: this.paths.sessionRecycleDir,
      storageDir: this.paths.storageDir
    }
  }

  startWatcher() {
    const repoPaths = this.repoService.listRepos().map((item) => item.localPath)

    this.fileWatcherService.restart(
      [this.paths.skillsDir, this.paths.reposDir, ...repoPaths],
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
    })
  }

  getState() {
    return this.state
  }

  async refreshAll({ emit = true } = {}) {
    const previousSkills = await this.storage.read("skills", [])
    const previousCliTargets = await this.storage.read("cliTargets", [])
    const installIndex = await this.storage.read("installs", {})
    const normalizedInstallIndex = { ...installIndex }
    const [detectedCliTargets] = await Promise.all([
      this.cliDetectionService.detectAll()
    ])
    const cliTargets = this.mergeCliTargets(
      previousCliTargets,
      detectedCliTargets
    )
    const { sessions, diagnostics: sessionDiagnostics } =
      await this.sessionService.refresh(cliTargets)
    const repos = this.repoService.listRepos().map((repo) => ({
      ...repo,
      skillCount: 0
    }))
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
      diagnostics: [...diagnostics, ...sessionDiagnostics],
      paths: this.toPublicPaths(),
      refreshedAt: Date.now()
    }

    this.startSessionWatcher()

    if (emit) {
      this.emit("state-changed", this.state)
    }

    return this.state
  }

  mergeCliTargets(previousCliTargets, detectedCliTargets) {
    const detectedMap = new Map(
      detectedCliTargets.map((item) => [item.id, item])
    )
    const previousMap = new Map(
      previousCliTargets.map((item) => [item.id, item])
    )
    const targetIds = [
      ...new Set([...detectedMap.keys(), ...previousMap.keys()])
    ]

    return targetIds
      .map((targetId) => {
        const detected = detectedMap.get(targetId) || {}
        const previous = previousMap.get(targetId) || {}

        if (!previous.id && !detected.installed) {
          return null
        }

        return {
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

    for (const skill of skills) {
      if (skill.installedTargets.length) {
        normalizedInstallIndex[skill.name] = skill.installedTargets
      } else {
        delete normalizedInstallIndex[skill.name]
      }
    }

    this.storage.scheduleWrite("skills", skills)
    this.storage.scheduleWrite("cliTargets", cliTargets)
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
      "这个 Skill 由 AI Manager 创建。"
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

        if (
          path.resolve(sourcePath) !== path.resolve(mountedPath) &&
          (await pathExists(mountedPath))
        ) {
          throw new Error(`目标 CLI 路径已被占用：${mountedPath}`)
        }

        if (managedPaths.has(parsed.name)) {
          mounts.push({
            name: parsed.name,
            description: parsed.description,
            cliId: cliTarget.id,
            cliName: cliTarget.name,
            sourcePath,
            managedPath,
            mountedPath
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
          mountedPath
        })
      }
    }

    return { imports, mounts }
  }

  async previewSkillsFromCli(targetId) {
    const { imports, mounts } = await this.collectCliSkillImports(targetId)
    const itemMap = new Map()

    for (const candidate of [...imports, ...mounts]) {
      const item = itemMap.get(candidate.name) || {
        name: candidate.name,
        description: candidate.description,
        cliNames: [],
        sourcePaths: [],
        alreadyManaged: !imports.find((entry) => entry.name === candidate.name)
      }

      item.cliNames.push(candidate.cliName)
      item.sourcePaths.push(candidate.sourcePath)
      itemMap.set(candidate.name, item)
    }

    return Array.from(itemMap.values()).sort((left, right) =>
      left.name.localeCompare(right.name)
    )
  }

  async importSkillsFromCli(targetId, skillNames) {
    const { imports, mounts } = await this.collectCliSkillImports(targetId)
    const allNames = [...imports, ...mounts].map((item) => item.name)
    const selectedNames = new Set(skillNames || allNames)
    const selectedImports = imports.filter((item) =>
      selectedNames.has(item.name)
    )
    const selectedMounts = mounts.filter((item) => selectedNames.has(item.name))

    if (!selectedImports.length && !selectedMounts.length) {
      await this.refreshAll()
      return
    }

    for (const candidate of selectedImports) {
      await fs.cp(candidate.sourcePath, candidate.managedPath, {
        recursive: true
      })
    }

    for (const candidate of [...selectedImports, ...selectedMounts]) {
      await fs.rm(candidate.sourcePath, { recursive: true, force: true })
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

  async dispose() {
    this.fileWatcherService.stop()
    await this.sessionService.dispose()
    await this.storage.flush()
  }
}

module.exports = {
  ManagerService
}
