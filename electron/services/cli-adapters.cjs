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

async function resolveBinary(binaryName) {
  const locator = process.platform === "win32" ? "where.exe" : "which"

  try {
    const { stdout } = await execFileAsync(locator, [binaryName], {
      windowsHide: true
    })
    const firstLine = stdout
      .split(/\r?\n/)
      .map((item) => item.trim())
      .find(Boolean)

    return firstLine || null
  } catch {
    return null
  }
}

async function detectVersion(binaryPath) {
  if (!binaryPath) {
    return undefined
  }

  try {
    const { stdout, stderr } = await execFileAsync(binaryPath, ["--version"], {
      windowsHide: true
    })
    const output = `${stdout}\n${stderr}`
      .split(/\r?\n/)
      .map((item) => item.trim())
      .find(Boolean)

    return output || undefined
  } catch {
    return undefined
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
    sessionScanRules
  }) {
    this.id = id
    this.type = type
    this.name = name
    this.icon = icon
    this.binaryName = binaryName
    this.configDirName = configDirName
    this.configPath = configPath
    this.sessionsDirName = sessionsDirName
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
    const executablePath = await resolveBinary(this.binaryName)
    const configExists = await pathExists(configPath)
    const installed = Boolean(configExists || executablePath)

    return {
      id: this.id,
      type: this.type,
      name: this.name,
      icon: this.icon,
      installed,
      executablePath: executablePath || undefined,
      configPath,
      skillsPath: this.getSkillsPath(),
      sessionsPath: this.getSessionsPath(),
      sessionPaths: this.getSessionPaths(),
      sessionScanRules: this.getSessionScanRules(),
      version: await detectVersion(executablePath),
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
    new CodexAdapter(cliConfigPaths.codex),
    new GeminiAdapter(cliConfigPaths.gemini),
    new OpenCodeAdapter(cliConfigPaths.opencode)
  ]
}

module.exports = {
  createCliAdapters
}
