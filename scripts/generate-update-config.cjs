const fs = require("node:fs")
const path = require("node:path")

const rootPath = path.resolve(__dirname, "..")
const envPath = path.join(rootPath, ".env")
const outputPath = path.join(
  rootPath,
  "electron",
  "update-config.generated.cjs"
)

function parseEnvLine(line) {
  const trimmed = line.trim()

  if (!trimmed || trimmed.startsWith("#")) {
    return null
  }

  const separatorIndex = trimmed.indexOf("=")

  if (separatorIndex === -1) {
    return null
  }

  const key = trimmed.slice(0, separatorIndex).trim()
  let value = trimmed.slice(separatorIndex + 1).trim()

  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    value = value.slice(1, -1)
  }

  return {
    key,
    value
  }
}

function loadLocalEnv() {
  if (!fs.existsSync(envPath)) {
    return
  }

  const lines = fs.readFileSync(envPath, "utf8").split(/\r?\n/)

  for (const line of lines) {
    const entry = parseEnvLine(line)

    if (entry && process.env[entry.key] === undefined) {
      process.env[entry.key] = entry.value
    }
  }
}

loadLocalEnv()

const githubToken = String(process.env.AI_MANAGER_GITHUB_TOKEN || "").trim()

if (!githubToken) {
  throw new Error("暂无可用更新源，请联系开发人员解决问题吧！")
}

fs.writeFileSync(
  outputPath,
  `module.exports = ${JSON.stringify(
    {
      githubToken
    },
    null,
    2
  )}\n`,
  "utf8"
)

console.log("已生成 electron/update-config.generated.cjs")
