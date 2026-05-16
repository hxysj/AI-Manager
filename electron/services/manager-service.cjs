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
const { CodexAccountService } = require("./codex-account-service.cjs")
const { RuntimeProviderService } = require("./runtime-provider-service.cjs")

const execFileAsync = promisify(execFile)

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
    this.codexAccountService = new CodexAccountService(this.storage)
    this.runtimeProviderService = new RuntimeProviderService(this.storage)
    this.state = {
      cliTargets: [],
      skills: [],
      repos: [],
      sessions: [],
      codexAccounts: [],
      codexLoginState: null,
      providers: [],
      runtimeConfigSchemas: {},
      runtimeModels: [],
      runtimeProfiles: [],
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
    await this.codexAccountService.init()
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
    await this.runtimeProviderService.init()
    await this.refreshAll({ emit: false })
    this.codexAccountService.startAutoRefresh()
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
    await this.refreshAll({ preferDetectedPaths: true })
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

  async refreshAll({ emit = true, preferDetectedPaths = false } = {}) {
    const previousSkills = await this.storage.read("skills", [])
    const previousCliTargets = await this.storage.read("cliTargets", [])
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
    const runtimeState = this.runtimeProviderService.getState()
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
      codexAccounts: this.codexAccountService.getState(),
      codexLoginState: this.codexAccountService.getLoginState(),
      ...runtimeState,
      diagnostics: [...diagnostics, ...sessionDiagnostics],
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
      ...new Set([...detectedMap.keys(), ...previousMap.keys()])
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
      path.join(os.tmpdir(), "ai-manager-skill-")
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
              cliNames: ["AI Manager"],
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
    this.state = {
      ...this.state,
      ...this.runtimeProviderService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async deleteProvider(providerId) {
    this.runtimeProviderService.deleteProvider(providerId)
    this.state = {
      ...this.state,
      ...this.runtimeProviderService.getState(),
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
    await this.codexAccountService.enableAccount(
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

  async saveRuntimeModel(input) {
    this.runtimeProviderService.saveModel(input)
    this.state = {
      ...this.state,
      ...this.runtimeProviderService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async switchRuntime(input) {
    this.runtimeProviderService.switchRuntime(input)
    await this.runtimeProviderService.writeCliConfig(
      input.cli,
      this.state.cliTargets.find((item) => item.id === input.cli)
    )
    this.state = {
      ...this.state,
      ...this.runtimeProviderService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  async clearRuntime(cli) {
    this.runtimeProviderService.clearRuntime(cli)
    this.state = {
      ...this.state,
      ...this.runtimeProviderService.getState(),
      refreshedAt: Date.now()
    }
    this.emit("state-changed", this.state)
    return this.state
  }

  buildRuntimeEnv(cli) {
    return this.runtimeProviderService.buildRuntimeEnv(cli)
  }

  async dispose() {
    this.codexAccountService.stopAutoRefresh()
    this.fileWatcherService.stop()
    await this.sessionService.dispose()
    await this.storage.flush()
  }
}

module.exports = {
  ManagerService
}
