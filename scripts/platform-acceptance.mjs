import { spawn } from "node:child_process"
import { createHash } from "node:crypto"
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises"
import { arch, platform, release } from "node:os"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { manualChecks } from "./platform-acceptance-checks.mjs"

const root = fileURLToPath(new URL("..", import.meta.url))
const args = process.argv.slice(2)
const outputIndex = args.indexOf("--output")
if (outputIndex < 0 || !args[outputIndex + 1])
  throw new Error("请通过 --output 指定独立的验收记录目录")
const output = path.resolve(args[outputIndex + 1])
const version = JSON.parse(
  await readFile(path.join(root, "package.json"), "utf8")
).version
for (let index = 0; index < args.length; index += 1) {
  if (args[index] === "--output") index += 1
  else if (!["--renderer-only", "--build"].includes(args[index]))
    throw new Error(`未知验收参数：${args[index]}`)
}
await mkdir(output, { recursive: true })
if ((await readdir(output)).length)
  throw new Error("请使用空的验收目录，避免覆盖已有证据")
const reportPath = path.join(output, "automated-report.json")
const report = {
  schema: 1,
  scope: "automated-regression",
  version,
  platform: platform(),
  arch: arch(),
  osRelease: release(),
  desktopSession:
    platform() === "darwin"
      ? "aqua"
      : process.env.XDG_SESSION_TYPE ||
        (process.env.WAYLAND_DISPLAY
          ? "wayland"
          : process.env.DISPLAY
            ? "x11"
            : "unknown"),
  createdAt: new Date().toISOString(),
  fullAcceptance: false,
  checks: []
}

async function run(id, command, commandArgs) {
  const started = Date.now()
  let log = ""
  const result = await new Promise((resolve) => {
    const child = spawn(command, commandArgs, {
      cwd: root,
      env: process.env,
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"]
    })
    child.stdout.on("data", (bytes) => {
      log += bytes.toString()
    })
    child.stderr.on("data", (bytes) => {
      log += bytes.toString()
    })
    child.on("error", (error) => {
      log += `${error.message}\n`
      resolve({ code: null, signal: null })
    })
    child.on("close", (code, signal) => resolve({ code, signal }))
  })
  const file = `${id}.log`
  await writeFile(path.join(output, file), log)
  report.checks.push({
    id,
    status: result.code === 0 ? "passed" : "failed",
    ...result,
    elapsedMs: Date.now() - started,
    evidence: { file, sha256: createHash("sha256").update(log).digest("hex") }
  })
  // 每项完成即落盘，进程中断时保留已执行结果，不把未运行项目算作通过。
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`)
  console.log(
    `${result.code === 0 ? "通过" : "失败"} ${id} (${Date.now() - started} ms)`
  )
}

const scripts = (await readdir(path.join(root, "scripts")))
  .filter((name) => /^test-.+\.mjs$/.test(name))
  .sort()
for (const script of scripts)
  await run(script.slice(0, -4), process.execPath, [
    path.join(root, "scripts", script)
  ])

if (!args.includes("--renderer-only")) {
  await run("rust-all-modules", "cargo", [
    "test",
    "--locked",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--lib"
  ])
  await run("native-probe-build", "cargo", [
    "check",
    "--locked",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--features",
    "desktop-acceptance",
    "--bin",
    "desktop-acceptance"
  ])
}
if (args.includes("--build")) {
  await run("renderer-build", process.execPath, [
    "scripts/build-translation-agent.mjs"
  ])
  await run("renderer-bundle", process.execPath, [
    "node_modules/vite/bin/vite.js",
    "build"
  ])
}

// 真实桌面、跨设备和安装包行为需要对应环境操作，自动回归报告不能替代这些证据。
const manualReport = {
  schema: 1,
  scope: "manual-desktop",
  version,
  platform: report.platform,
  arch: report.arch,
  osRelease: report.osRelease,
  desktopSession: report.desktopSession,
  hardware: "",
  package: { file: "", sha256: "" },
  fullAcceptance: false,
  checks: manualChecks.map((check) => ({
    ...check,
    status: "pending",
    notes: "",
    evidence: []
  }))
}
await writeFile(
  path.join(output, "manual-report.json"),
  `${JSON.stringify(manualReport, null, 2)}\n`
)
report.finishedAt = new Date().toISOString()
report.status = report.checks.every((check) => check.status === "passed")
  ? "passed"
  : "failed"
await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`)
console.log(`版本 ${version} 自动回归记录：${reportPath}`)
console.log(
  "实机交互项目保留为待验收，请结合 native-report.json 和操作证据确认。"
)
if (report.status !== "passed") process.exitCode = 1
