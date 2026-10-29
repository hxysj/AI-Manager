import { createReadStream } from "node:fs"
import { readFile, readdir, writeFile } from "node:fs/promises"
import { createHash } from "node:crypto"
import path from "node:path"
import { fileURLToPath, pathToFileURL } from "node:url"
import { manualChecks } from "./platform-acceptance-checks.mjs"

export async function hashFile(file) {
  const hash = createHash("sha256")
  for await (const bytes of createReadStream(file)) hash.update(bytes)
  return hash.digest("hex")
}

// 校验人工记录与对应机器上的自动结果；人工操作真实性仍由验收人负责。
export async function verifyAcceptance(files) {
  const entries = await Promise.all(
    ["automated", "native", "manual"].map(async (key) => {
      const file = path.resolve(files[key])
      return [key, { file, report: JSON.parse(await readFile(file, "utf8")) }]
    })
  )
  const reports = Object.fromEntries(entries)
  const errors = []
  const require = (condition, message) => {
    if (!condition) errors.push(message)
  }
  const automated = reports.automated.report
  const native = reports.native.report
  const manual = reports.manual.report
  require(["darwin", "linux"].includes(
    automated.platform
  ), "仅接受 macOS/Linux 实机报告")
  require(["x64", "arm64"].includes(automated.arch), "实机架构无效")
  require(automated.status === "passed", "自动回归存在失败或未完成")
  for (const [key, { report }] of entries) {
    require(report.schema === 1, `${key} 报告版本不支持`)
    require(report.version &&
      report.version === automated.version, `${key} 应用版本不一致`)
    require(report.platform === automated.platform &&
      report.arch === automated.arch, `${key} 系统或架构不一致`)
    require(report.desktopSession ===
      manual.desktopSession, `${key} 桌面会话不一致`)
    require(Array.isArray(report.checks) && report.checks.every(check => check.status === "passed"), `${key} 存在失败或未执行项目`)
  }
  require(automated.scope === "automated-regression", "缺少自动回归报告")
  require(native.scope === "native-probes", "缺少原生探针报告")
  require(manual.scope === "manual-desktop", "缺少人工桌面报告")
  require(Boolean(manual.hardware?.trim()) &&
    Boolean(manual.osRelease?.trim()), "请记录实机硬件和系统版本")
  require(automated.platform === "darwin" ||
    ["x11", "wayland"].includes(
      manual.desktopSession
    ), "Linux 须记录 x11 或 wayland 会话")
  const scriptDirectory = path.dirname(fileURLToPath(import.meta.url))
  const expected = (await readdir(scriptDirectory))
    .filter((name) => /^test-.+\.mjs$/.test(name))
    .map((name) => name.slice(0, -4))
    .concat([
      "rust-all-modules",
      "native-probe-build",
      "renderer-build",
      "renderer-bundle"
    ])
  async function evidence(file, entry, label) {
    if (!entry?.file || !/^[a-f0-9]{64}$/.test(entry?.sha256 || "")) {
      errors.push(`${label} 缺少文件或 SHA256`)
      return
    }
    try {
      require((await hashFile(path.resolve(path.dirname(file), entry.file))) ===
        entry.sha256, `${label} 文件校验失败`)
    } catch (error) {
      errors.push(`${label} 文件不可读：${error.message}`)
    }
  }
  function find(report, id) {
    const matches = (report.checks || []).filter((check) => check.id === id)
    require(matches.length === 1, `${id} 项目缺失或重复`)
    require(matches[0]?.status === "passed", `${id} 未通过`)
    return matches[0]
  }
  for (const id of expected) {
    const check = find(automated, id)
    require(check?.code === 0, `${id} 退出状态无效`)
    await evidence(reports.automated.file, check?.evidence, id)
  }
  for (const id of ["native-ports", "native-clipboard", "native-capture"])
    find(native, id)
  const ports = native.checks?.find((check) => check.id === "native-ports")
  require(ports?.evidence?.listenerFound === true &&
    ports?.evidence?.selfTerminationBlocked ===
      true, "缺少端口与自身进程保护证据")
  const clipboard = native.checks?.find(check => check.id === "native-clipboard")?.evidence
  require((clipboard?.mode === "image-roundtrip" && clipboard.pixelsMatch === true)
    || (manual.desktopSession === "wayland" && clipboard?.mode === "read-only" && clipboard.readable === true), "缺少剪贴板探针结果")
  const capture = native.checks?.find((check) => check.id === "native-capture")
  require(capture?.evidence?.frames?.length === 3, "缺少三帧连续采集证据")
  for (const frame of capture?.evidence?.frames || []) {
    require(frame.width > 0 && frame.height > 0, "采集帧尺寸无效")
    await evidence(reports.native.file, frame, "采集帧")
  }
  await evidence(reports.manual.file, manual.package, "实测安装包")
  for (const { id } of manualChecks) {
    const check = find(manual, id)
    require(Boolean(check?.notes?.trim()), `${id} 缺少操作和预期结果说明`)
    require(Array.isArray(check?.evidence) &&
      check.evidence.length > 0, `${id} 缺少操作证据`)
    for (const entry of check?.evidence || [])
      await evidence(reports.manual.file, entry, id)
  }
  return {
    schema: 1,
    scope: "single-machine-acceptance",
    version: automated.version,
    platform: automated.platform,
    arch: automated.arch,
    desktopSession: manual.desktopSession,
    createdAt: new Date().toISOString(),
    fullAcceptance: errors.length === 0,
    status: errors.length ? "failed" : "passed",
    errors,
    reports: await Promise.all(
      entries.map(async ([kind, { file }]) => ({
        kind,
        file,
        sha256: await hashFile(file)
      }))
    )
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  const args = process.argv.slice(2)
  const files = {}
  for (let index = 0; index < args.length; index += 2) {
    const name = args[index]?.replace(/^--/, "")
    if (
      !["automated", "native", "manual", "output"].includes(name) ||
      !args[index + 1]
    )
      throw new Error(
        "参数：--automated <JSON> --native <JSON> --manual <JSON> --output <新文件>"
      )
    files[name] = args[index + 1]
  }
  if (["automated", "native", "manual", "output"].some((key) => !files[key]))
    throw new Error("请提供自动、原生、人工报告和汇总输出路径")
  const report = await verifyAcceptance(files)
  await writeFile(files.output, `${JSON.stringify(report, null, 2)}\n`, {
    flag: "wx"
  })
  console.log(
    `${report.status === "passed" ? "通过" : "未通过"}：${files.output}`
  )
  for (const error of report.errors) console.error(error)
  if (!report.fullAcceptance) process.exitCode = 1
}
