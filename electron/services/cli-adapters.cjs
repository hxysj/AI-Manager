const fs = require("node:fs/promises")
const os = require("node:os")
const path = require("node:path")
const { execFile } = require("node:child_process")
const { promisify } = require("node:util")

const execFileAsync = promisify(execFile)

async function pathExists(targetPath) {
  if (!targetPath) {
    return false
  }

  try {
    await fs.access(targetPath)
    return true
  } catch {
    return false
  }
}

async function resolveBinaryCandidates(binaryName) {
  const locator = process.platform === "win32" ? "where.exe" : "which"

  try {
    const { stdout } = await execFileAsync(locator, [binaryName], {
      windowsHide: true
    })
    const candidates = stdout
      .split(/\r?\n/)
      .map((item) => item.trim())
      .filter(Boolean)

    return [...new Set(candidates)]
  } catch {
    return []
  }
}

async function resolveNpmGlobalCandidates(binaryName) {
  let prefix = ""

  try {
    const { stdout } = await execFileAsync("npm", ["prefix", "-g"], {
      windowsHide: true,
      shell: process.platform === "win32"
    })
    prefix = stdout
      .split(/\r?\n/)
      .map((item) => item.trim())
      .find(Boolean) || ""
  } catch {
    return []
  }

  if (!prefix) {
    return []
  }

  const candidatePaths = process.platform === "win32"
    ? [
        path.join(prefix, `${binaryName}.cmd`),
        path.join(prefix, binaryName)
      ]
    : [path.join(prefix, binaryName)]

  if (binaryName === "codex") {
    candidatePaths.push(
      path.join(prefix, "node_modules", "@openai", "codex", "bin", "codex.js")
    )
  }

  const candidates = []

  for (const candidatePath of candidatePaths) {
    if (await pathExists(candidatePath)) {
      candidates.push(candidatePath)
    }
  }

  return [...new Set(candidates)]
}

async function runVersionCommand(binaryPath) {
  if (/\.js$/i.test(binaryPath)) {
    return execFileAsync("node", [binaryPath, "--version"], {
      windowsHide: true,
      shell: process.platform === "win32"
    })
  }

  return execFileAsync(binaryPath, ["--version"], {
    windowsHide: true,
    shell: process.platform === "win32"
  })
}

async function detectExecutable(binaryPaths) {
  const executablePath = binaryPaths[0] || ""

  if (!executablePath) {
    return {
      executablePath: "",
      version: undefined
    }
  }

  try {
    const { stdout, stderr } = await runVersionCommand(executablePath)
    const output = `${stdout}\n${stderr}`
      .split(/\r?\n/)
      .map((item) => item.trim())
      .find(Boolean)

    return {
      executablePath,
      version: output
    }
  } catch {}

  return {
    executablePath,
    version: undefined
  }
}

class BaseCliAdapter {
  constructor({
    id,
    type,
    name,
    icon,
    binaryName,
    configDirName,
    configPath,
    sessionsDirName,
    sessionScanRules,
    preferNpmGlobal
  }) {
    this.id = id
    this.type = type
    this.name = name
    this.icon = icon
    this.binaryName = binaryName
    this.configDirName = configDirName
    this.configPath = configPath
    this.sessionsDirName = sessionsDirName
    this.preferNpmGlobal = Boolean(preferNpmGlobal)
    this.sessionScanRules = sessionScanRules || {
      extensions: [".json", ".jsonl", ".transcript"],
      names: []
    }
  }

  getConfigPath() {
    return this.configPath || path.join(os.homedir(), this.configDirName)
  }

  getSkillsPath() {
    return path.join(this.getConfigPath(), "skills")
  }

  getSessionsPath() {
    if (!this.sessionsDirName) {
      return undefined
    }

    return path.join(this.getConfigPath(), this.sessionsDirName)
  }

  getSessionPaths() {
    return [this.getSessionsPath()].filter(Boolean)
  }

  getSessionScanRules() {
    return this.sessionScanRules
  }

  async detect() {
    const configPath = this.getConfigPath()
    const executablePaths = this.preferNpmGlobal
      ? await resolveNpmGlobalCandidates(this.binaryName)
      : await resolveBinaryCandidates(this.binaryName)
    const executable = await detectExecutable(executablePaths)
    const configExists = await pathExists(configPath)
    const installed = Boolean(configExists || executable.executablePath)

    return {
      id: this.id,
      type: this.type,
      name: this.name,
      icon: this.icon,
      installed,
      executablePath: executable.executablePath || undefined,
      configPath,
      skillsPath: this.getSkillsPath(),
      sessionsPath: this.getSessionsPath(),
      sessionPaths: this.getSessionPaths(),
      sessionScanRules: this.getSessionScanRules(),
      version: executable.version,
      detectedAt: Date.now()
    }
  }

  async installSkill(sourcePath, skillName) {
    const targetPath = path.join(this.getSkillsPath(), skillName)
    return { sourcePath, targetPath }
  }

  async uninstallSkill(skillName) {
    return path.join(this.getSkillsPath(), skillName)
  }
}

class ClaudeAdapter extends BaseCliAdapter {
  constructor(configPath) {
    super({
      id: "claude",
      type: "claude",
      name: "Claude",
      icon: "claude.svg",
      binaryName: "claude",
      configDirName: ".claude",
      configPath,
      sessionsDirName: "projects",
      sessionScanRules: {
        extensions: [".jsonl"],
        names: []
      }
    })
  }
}

class CodexAdapter extends BaseCliAdapter {
  constructor(configPath) {
    super({
      id: "codex",
      type: "codex",
      name: "Codex",
      icon: "codex.svg",
      binaryName: "codex",
      configDirName: ".codex",
      configPath,
      sessionsDirName: "sessions",
      preferNpmGlobal: true,
      sessionScanRules: {
        extensions: [".json", ".jsonl", ".transcript"],
        names: []
      }
    })
  }
}

class GeminiAdapter extends BaseCliAdapter {
  constructor(configPath) {
    super({
      id: "gemini",
      type: "gemini",
      name: "Gemini",
      icon: "geminicli.svg",
      binaryName: "gemini",
      configDirName: ".gemini",
      configPath,
      sessionsDirName: "tmp",
      sessionScanRules: {
        extensions: [".json", ".jsonl"],
        names: ["session", "checkpoint"]
      }
    })
  }
}

class OpenCodeAdapter extends BaseCliAdapter {
  constructor(configPath) {
    super({
      id: "opencode",
      type: "opencode",
      name: "OpenCode",
      icon: "opencode.svg",
      binaryName: "opencode",
      configDirName: ".opencode",
      configPath,
      sessionsDirName: "sessions",
      sessionScanRules: {
        extensions: [".json", ".jsonl", ".transcript"],
        names: []
      }
    })
  }
}

function createCliAdapters(cliConfigPaths = {}) {
  return [
    new ClaudeAdapter(cliConfigPaths.claude),
    new CodexAdapter(cliConfigPaths.codex)
    // 当前版本暂不启用 Gemini 和 OpenCode。
    // new GeminiAdapter(cliConfigPaths.gemini),
    // new OpenCodeAdapter(cliConfigPaths.opencode)
  ]
}

module.exports = {
  createCliAdapters
}
