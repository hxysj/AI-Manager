const fs = require("node:fs/promises")
const path = require("node:path")
const crypto = require("node:crypto")
const { slugifyName } = require("./path-utils.cjs")

const SUPPORTED_PROMPT_CLIS = {
  claude: {
    id: "claude",
    name: "Claude",
    icon: "claude.svg",
    runtimeFileName: "CLAUDE.md"
  },
  codex: {
    id: "codex",
    name: "Codex",
    icon: "codex.svg",
    runtimeFileName: "AGENTS.md"
  }
}

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath)
    return true
  } catch {
    return false
  }
}

async function readJsonFile(filePath, fallback) {
  try {
    return JSON.parse(await fs.readFile(filePath, "utf8"))
  } catch (error) {
    if (error.code === "ENOENT") {
      return fallback
    }

    throw error
  }
}

async function writeJsonFile(filePath, payload) {
  await fs.mkdir(path.dirname(filePath), { recursive: true })
  await fs.writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`, "utf8")
}

function sha256(content) {
  return crypto.createHash("sha256").update(content).digest("hex")
}

function now() {
  return Date.now()
}

function normalizeCli(cli) {
  const target = String(cli || "").trim()

  if (!SUPPORTED_PROMPT_CLIS[target]) {
    throw new Error("Prompt 仅支持 Claude 和 Codex")
  }

  return target
}

function buildRuntimeContent(content) {
  return String(content || "").trim()
}

function normalizePromptContent(content) {
  return String(content || "")
    .replace(/\r\n/g, "\n")
    .replace(/[ \t]+/g, " ")
    .trim()
}

function createBigrams(value) {
  const text = normalizePromptContent(value)

  if (text.length < 2) {
    return text ? [text] : []
  }

  const bigrams = []

  for (let index = 0; index < text.length - 1; index += 1) {
    bigrams.push(text.slice(index, index + 2))
  }

  return bigrams
}

function calculateSimilarity(left, right) {
  const leftText = normalizePromptContent(left)
  const rightText = normalizePromptContent(right)

  if (!leftText && !rightText) {
    return 1
  }

  if (!leftText || !rightText) {
    return 0
  }

  if (leftText === rightText) {
    return 1
  }

  const leftBigrams = createBigrams(leftText)
  const rightBigrams = createBigrams(rightText)
  const rightCounts = new Map()
  let intersection = 0

  for (const item of rightBigrams) {
    rightCounts.set(item, (rightCounts.get(item) || 0) + 1)
  }

  for (const item of leftBigrams) {
    const count = rightCounts.get(item) || 0

    if (count > 0) {
      intersection += 1
      rightCounts.set(item, count - 1)
    }
  }

  return (2 * intersection) / (leftBigrams.length + rightBigrams.length)
}

class PromptRuntimeService {
  constructor(paths) {
    this.paths = paths
    this.prompts = []
    this.profiles = {
      claude: { activePromptId: "" },
      codex: { activePromptId: "" }
    }
    this.runtimeState = {}
  }

  async init() {
    await this.ensureDirectories()
    await this.loadProfiles()
    await this.loadRuntimeState()
    await this.loadPrompts()
  }

  async ensureDirectories() {
    await Promise.all([
      fs.mkdir(path.join(this.paths.promptsDir, "claude"), { recursive: true }),
      fs.mkdir(path.join(this.paths.promptsDir, "codex"), { recursive: true }),
      fs.mkdir(this.paths.promptProfilesDir, { recursive: true })
    ])
  }

  getPromptDir(cli) {
    return path.join(this.paths.promptsDir, cli)
  }

  getMetadataPath(cli, promptId) {
    return path.join(this.getPromptDir(cli), `${promptId}.json`)
  }

  getPromptPath(cli, fileName) {
    return path.join(this.getPromptDir(cli), fileName)
  }

  getProfilePath(cli) {
    return path.join(this.paths.promptProfilesDir, `${cli}-profile.json`)
  }

  getRuntimePath(cli, cliTarget) {
    const configPath = cliTarget?.configPath

    if (!configPath) {
      return ""
    }

    return path.join(configPath, SUPPORTED_PROMPT_CLIS[cli].runtimeFileName)
  }

  async loadProfiles() {
    for (const cli of Object.keys(SUPPORTED_PROMPT_CLIS)) {
      this.profiles[cli] = await readJsonFile(this.getProfilePath(cli), {
        activePromptId: ""
      })
    }
  }

  async saveProfile(cli) {
    await writeJsonFile(this.getProfilePath(cli), this.profiles[cli])
  }

  async loadRuntimeState() {
    this.runtimeState = await readJsonFile(
      this.paths.storageFiles.promptRuntimeState,
      {}
    )
  }

  async saveRuntimeState() {
    await writeJsonFile(
      this.paths.storageFiles.promptRuntimeState,
      this.runtimeState
    )
  }

  async loadPrompts() {
    const prompts = []

    for (const cli of Object.keys(SUPPORTED_PROMPT_CLIS)) {
      const promptDir = this.getPromptDir(cli)
      const entries = await fs.readdir(promptDir, { withFileTypes: true })

      for (const entry of entries) {
        if (!entry.isFile() || !entry.name.endsWith(".json")) {
          continue
        }

        const metadata = await readJsonFile(
          path.join(promptDir, entry.name),
          null
        )

        if (!metadata?.id || !metadata.fileName) {
          continue
        }

        const content = await fs
          .readFile(this.getPromptPath(cli, metadata.fileName), "utf8")
          .catch(() => "")

        prompts.push({
          ...metadata,
          content,
          metadataFileName: entry.name,
          storageDir: promptDir
        })
      }
    }

    this.prompts = prompts.sort((left, right) => {
      if (left.cli !== right.cli) {
        return left.cli.localeCompare(right.cli)
      }

      return right.updatedAt - left.updatedAt
    })
  }

  async savePrompt(input) {
    const cli = normalizeCli(input.cli)
    const previous = this.prompts.find(item => item.id === input.id)
    const name = String(input.name || previous?.name || "").trim()
    const content = String(input.content || "")

    if (!name) {
      throw new Error("Prompt 名称不能为空")
    }

    if (!content.trim()) {
      throw new Error("Prompt 内容不能为空")
    }

    const duplicate = this.prompts.find(item => {
      return (
        item.cli === cli &&
        item.id !== previous?.id &&
        item.name.trim().toLowerCase() === name.toLowerCase()
      )
    })

    if (duplicate) {
      throw new Error("当前 CLI 已存在同名 Prompt")
    }

    const promptId =
      previous?.id ||
      this.createPromptId(cli, name) ||
      `prompt-${crypto.randomUUID().slice(0, 8)}`
    const fileName = previous?.fileName || `${promptId}.md`
    const metadata = {
      id: promptId,
      name,
      description:
        String(input.description || previous?.description || "").trim() ||
        undefined,
      cli,
      fileName,
      createdAt: previous?.createdAt || now(),
      updatedAt: now()
    }

    await fs.mkdir(this.getPromptDir(cli), { recursive: true })
    await fs.writeFile(
      this.getPromptPath(cli, fileName),
      `${content.trim()}\n`,
      "utf8"
    )
    await writeJsonFile(this.getMetadataPath(cli, promptId), metadata)

    if (
      previous?.metadataFileName &&
      previous.metadataFileName !== `${promptId}.json`
    ) {
      await fs.rm(
        path.join(this.getPromptDir(cli), previous.metadataFileName),
        {
          force: true
        }
      )
    }

    await this.loadPrompts()
    return this.getState()
  }

  async deletePrompt(promptId) {
    const prompt = this.prompts.find(item => item.id === promptId)

    if (!prompt) {
      return this.getState()
    }

    if (this.profiles[prompt.cli]?.activePromptId === prompt.id) {
      throw new Error("当前 Prompt 已启用，请先切换到其他 Prompt 后再删除")
    }

    await fs.rm(
      path.join(
        this.getPromptDir(prompt.cli),
        prompt.metadataFileName || `${prompt.id}.json`
      ),
      { force: true }
    )
    await fs.rm(this.getPromptPath(prompt.cli, prompt.fileName), {
      force: true
    })
    await this.loadPrompts()
    return this.getState()
  }

  async enablePrompt(promptId, cliTarget) {
    const prompt = this.prompts.find(item => item.id === promptId)

    if (!prompt) {
      throw new Error("Prompt 不存在")
    }

    const runtimePath = this.getRuntimePath(prompt.cli, cliTarget)

    if (!runtimePath) {
      throw new Error("未找到对应 CLI 的全局配置目录")
    }

    const runtimeContent = buildRuntimeContent(prompt.content)
    await fs.mkdir(path.dirname(runtimePath), { recursive: true })
    await fs.writeFile(runtimePath, runtimeContent, "utf8")

    this.profiles[prompt.cli] = {
      activePromptId: prompt.id
    }
    this.runtimeState[prompt.cli] = {
      activePromptId: prompt.id,
      runtimeHash: sha256(runtimeContent),
      lastSyncAt: now(),
      runtimePath,
      status: "SYNCED"
    }
    await this.saveProfile(prompt.cli)
    await this.saveRuntimeState()
    return this.getState()
  }

  async disablePrompt(input, cliTarget) {
    const prompt = this.prompts.find(item => item.id === input.promptId)
    const cli = normalizeCli(input.cli || prompt?.cli)

    if (prompt && this.profiles[cli]?.activePromptId !== prompt.id) {
      return this.getState()
    }

    const runtimePath = this.getRuntimePath(cli, cliTarget)

    if (runtimePath) {
      await fs.rm(runtimePath, { force: true })
    }

    this.profiles[cli] = {
      activePromptId: ""
    }
    this.runtimeState[cli] = {
      ...(this.runtimeState[cli] || {}),
      activePromptId: "",
      runtimePath,
      status: "NO_ACTIVE"
    }
    await this.saveProfile(cli)
    await this.saveRuntimeState()
    return this.getState()
  }

  async importGlobalPrompt(input, cliTarget) {
    const cli = normalizeCli(input.cli)
    const content = await this.readGlobalPromptContent(cli, cliTarget)
    const duplicate = this.findSameContentPrompt(cli, content)

    if (duplicate) {
      throw new Error("当前全局 Prompt 内容已存在于规则库中，无需重复导入")
    }

    return this.savePrompt({
      cli,
      name: input.name,
      description: input.description,
      content
    })
  }

  async previewImportGlobalPrompt(input, cliTarget) {
    const cli = normalizeCli(input.cli)
    const runtimePath = this.getRuntimePath(cli, cliTarget)
    const content = await this.readGlobalPromptContent(cli, cliTarget)
    const samePrompt = this.findSameContentPrompt(cli, content)

    if (samePrompt) {
      return {
        status: "SAME_CONTENT",
        prompt: samePrompt,
        runtimeContent: content,
        runtimePath
      }
    }

    const scopedPrompts = this.prompts.filter(item => item.cli === cli)

    if (!scopedPrompts.length) {
      return {
        status: "NEW",
        runtimeContent: content,
        runtimePath
      }
    }

    const similarPrompt = this.findSimilarContentPrompt(cli, content)

    if (!similarPrompt || similarPrompt.similarity <= 0.8) {
      return {
        status: "NEW",
        runtimeContent: content,
        runtimePath
      }
    }

    return {
      status: "DIFF",
      prompt: similarPrompt.prompt,
      similarity: similarPrompt.similarity,
      managerContent: similarPrompt.prompt.content,
      runtimeContent: content,
      runtimePath
    }
  }

  async resolveImportConflict(input) {
    if (input.source === "manager") {
      return this.getState()
    }

    if (input.source !== "runtime") {
      throw new Error("请选择要保存的 Prompt 版本")
    }

    const prompt = this.prompts.find(item => item.id === input.ruleId)

    if (!prompt) {
      throw new Error("Prompt 不存在")
    }

    await this.savePrompt({
      ...prompt,
      content: input.runtimeContent
    })
    return this.getState()
  }

  async readGlobalPromptContent(cli, cliTarget) {
    const runtimePath = this.getRuntimePath(cli, cliTarget)

    if (!runtimePath || !(await pathExists(runtimePath))) {
      throw new Error("未找到可导入的全局 Prompt 文件")
    }

    const content = await fs.readFile(runtimePath, "utf8")

    if (!content.trim()) {
      throw new Error("全局 Prompt 文件为空，无法导入")
    }

    return normalizePromptContent(content)
  }

  findSameContentPrompt(cli, content) {
    const normalizedContent = normalizePromptContent(content)

    return this.prompts.find(item => {
      return (
        item.cli === cli &&
        normalizePromptContent(item.content) === normalizedContent
      )
    })
  }

  findSimilarContentPrompt(cli, content) {
    return this.prompts
      .filter(item => item.cli === cli)
      .map(item => ({
        prompt: item,
        similarity: calculateSimilarity(item.content, content)
      }))
      .filter(item => item.similarity < 1)
      .sort((left, right) => right.similarity - left.similarity)[0]
  }

  async resolveDrift(input, cliTarget) {
    const cli = normalizeCli(input.cli)
    const activePrompt = this.prompts.find(
      item => item.id === this.profiles[cli]?.activePromptId
    )

    if (!activePrompt) {
      throw new Error("当前 CLI 没有启用的 Prompt")
    }

    const runtimePath = this.getRuntimePath(cli, cliTarget)

    if (input.source === "runtime") {
      const runtimeContent = await fs.readFile(runtimePath, "utf8")
      await this.savePrompt({
        ...activePrompt,
        content: runtimeContent
      })
    }

    return this.enablePrompt(activePrompt.id, cliTarget)
  }

  async comparePrompt(promptId, cliTarget) {
    const prompt = this.prompts.find(item => item.id === promptId)

    if (!prompt) {
      throw new Error("Prompt 不存在")
    }

    const runtimePath = this.getRuntimePath(prompt.cli, cliTarget)
    const runtimeContent =
      runtimePath && (await pathExists(runtimePath))
        ? await fs.readFile(runtimePath, "utf8")
        : ""

    return {
      prompt,
      managerContent: buildRuntimeContent(prompt.content),
      runtimeContent,
      runtimePath
    }
  }

  async refreshDrift(cliTargets) {
    for (const cli of Object.keys(SUPPORTED_PROMPT_CLIS)) {
      const cliTarget = cliTargets.find(item => item.id === cli)
      const activePrompt = this.prompts.find(
        item => item.id === this.profiles[cli]?.activePromptId
      )
      const runtimePath = this.getRuntimePath(cli, cliTarget)
      const previousState = this.runtimeState[cli] || {}

      if (!activePrompt) {
        this.runtimeState[cli] = {
          ...previousState,
          activePromptId: "",
          runtimePath,
          status: "NO_ACTIVE"
        }
        continue
      }

      const managerContent = buildRuntimeContent(activePrompt.content)

      if (!runtimePath || !(await pathExists(runtimePath))) {
        this.runtimeState[cli] = {
          ...previousState,
          activePromptId: activePrompt.id,
          runtimePath,
          status: "DIRTY_MANAGER"
        }
        continue
      }

      const runtimeContent = await fs.readFile(runtimePath, "utf8")
      const managerHash = sha256(managerContent)
      const runtimeHash = sha256(runtimeContent)
      let status = "SYNCED"

      if (runtimeHash !== managerHash) {
        if (!previousState.runtimeHash) {
          status = "MODIFIED_EXTERNALLY"
        } else if (
          runtimeHash !== previousState.runtimeHash &&
          managerHash !== previousState.runtimeHash
        ) {
          status = "CONFLICT"
        } else if (managerHash !== previousState.runtimeHash) {
          status = "DIRTY_MANAGER"
        } else {
          status = "MODIFIED_EXTERNALLY"
        }
      }

      this.runtimeState[cli] = {
        ...previousState,
        activePromptId: activePrompt.id,
        runtimeHash:
          status === "SYNCED" ? runtimeHash : previousState.runtimeHash,
        runtimePath,
        status
      }
    }

    await this.saveRuntimeState()
  }

  getRuntimeWatchPaths(cliTargets) {
    return Object.keys(SUPPORTED_PROMPT_CLIS)
      .map(cli =>
        this.getRuntimePath(
          cli,
          cliTargets.find(item => item.id === cli)
        )
      )
      .filter(Boolean)
  }

  createPromptId(cli, name) {
    const baseId = slugifyName(name)

    if (!baseId) {
      return ""
    }

    const usedIds = new Set(this.prompts.map(item => item.id))

    if (!usedIds.has(baseId)) {
      return baseId
    }

    for (let index = 2; index < 1000; index += 1) {
      const nextId = `${baseId}-${index}`

      if (!usedIds.has(nextId)) {
        return nextId
      }
    }

    return `${cli}-${baseId}-${crypto.randomUUID().slice(0, 8)}`
  }

  getState() {
    return {
      supportedClis: Object.values(SUPPORTED_PROMPT_CLIS),
      prompts: this.prompts,
      profiles: this.profiles,
      runtimeState: this.runtimeState
    }
  }
}

module.exports = {
  PromptRuntimeService,
  SUPPORTED_PROMPT_CLIS
}
