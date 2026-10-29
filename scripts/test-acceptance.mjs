import assert from "node:assert/strict"
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { hashFile, verifyAcceptance } from "./verify-platform-acceptance.mjs"
import { manualChecks } from "./platform-acceptance-checks.mjs"

// 夹具只测试证据完整性校验，不作为任何平台的实机通过记录。
const directory = await mkdtemp(
  path.join(tmpdir(), "ai-manager-acceptance-test-")
)
try {
  const shared = {
    schema: 1,
    version: "fixture",
    platform: "linux",
    arch: "x64",
    desktopSession: "wayland"
  }
  const proof = path.join(directory, "fixture-evidence.txt")
  await writeFile(proof, "synthetic verifier fixture")
  const evidence = {
    file: "fixture-evidence.txt",
    sha256: await hashFile(proof)
  }
  const scripts = (await readdir(path.dirname(fileURLToPath(import.meta.url))))
    .filter((name) => /^test-.+\.mjs$/.test(name))
    .map((name) => name.slice(0, -4))
  const automated = {
    ...shared,
    scope: "automated-regression",
    status: "passed",
    checks: [
      ...scripts,
      "rust-all-modules",
      "native-probe-build",
      "renderer-build",
      "renderer-bundle"
    ].map((id) => ({ id, status: "passed", code: 0, evidence }))
  }
  const native = {
    ...shared,
    scope: "native-probes",
    checks: [
      {
        id: "native-ports",
        status: "passed",
        evidence: { listenerFound: true, selfTerminationBlocked: true }
      },
      {
        id: "native-clipboard",
        status: "passed",
        evidence: { mode: "read-only", readable: true }
      },
      {
        id: "native-capture",
        status: "passed",
        evidence: {
          frames: [1, 2, 3].map(() => ({ ...evidence, width: 2, height: 2 }))
        }
      }
    ]
  }
  const manual = {
    ...shared,
    scope: "manual-desktop",
    osRelease: "fixture-os",
    hardware: "fixture-hardware",
    package: evidence,
    checks: manualChecks.map((check) => ({
      ...check,
      status: "passed",
      notes: "synthetic fixture",
      evidence: [evidence]
    }))
  }
  const files = Object.fromEntries(
    ["automated", "native", "manual"].map((key) => [
      key,
      path.join(directory, `${key}.json`)
    ])
  )
  const save = async () => {
    for (const [key, report] of Object.entries({ automated, native, manual }))
      await writeFile(files[key], JSON.stringify(report))
  }
  await save()
  assert.equal((await verifyAcceptance(files)).fullAcceptance, true)
  manual.checks[0].status = "pending"
  await save()
  assert.equal((await verifyAcceptance(files)).fullAcceptance, false)
  manual.checks[0].status = "passed"
  native.platform = "darwin"
  await save()
  assert.ok(
    (await verifyAcceptance(files)).errors.some((error) =>
      error.includes("系统或架构不一致")
    )
  )
  native.platform = shared.platform
  const removed = automated.checks.pop()
  await save()
  assert.equal((await verifyAcceptance(files)).fullAcceptance, false)
  automated.checks.push(removed)
  manual.checks[0].evidence = []
  await save()
  assert.equal((await verifyAcceptance(files)).fullAcceptance, false)
  manual.checks[0].evidence = [evidence]
  await save()
  await writeFile(proof, "changed")
  assert.ok(
    (await verifyAcceptance(files)).errors.some((error) =>
      error.includes("文件校验失败")
    )
  )
  assert.ok((await readFile(files.automated, "utf8")).includes("fixture"))
  console.log("验收记录缺项、未执行、跨平台错配和证据篡改检查通过")
} finally {
  await rm(directory, { recursive: true, force: true })
}
