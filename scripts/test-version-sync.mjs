import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import {
  mkdtemp,
  mkdir,
  readFile,
  realpath,
  rm,
  stat,
  utimes,
  writeFile
} from "node:fs/promises"
import { basename, dirname, join } from "node:path"
import { tmpdir } from "node:os"

const projectRoot = new URL("../", import.meta.url)
const fixtureParent = await realpath(tmpdir())
const fixtureRoot = await mkdtemp(join(fixtureParent, "ai-manager-version-"))
const generatedPaths = [
  "package-lock.json",
  "src-tauri/Cargo.toml",
  "src-tauri/Cargo.lock",
  "build/release-notes.md"
]
const readGenerated = () =>
  Promise.all(
    generatedPaths.map((path) => readFile(join(fixtureRoot, path), "utf8"))
  )
const runSync = (...args) =>
  spawnSync(
    process.execPath,
    [join(fixtureRoot, "scripts/sync-version.mjs"), ...args],
    {
      cwd: fixtureParent,
      encoding: "utf8"
    }
  )

try {
  // 在隔离目录运行真实脚本，变更版本和错误场景都不修改项目文件。
  for (const path of [
    "package.json",
    "src-tauri/tauri.conf.json",
    "scripts/sync-version.mjs",
    ...generatedPaths
  ]) {
    const destination = join(fixtureRoot, path)
    await mkdir(dirname(destination), { recursive: true })
    await writeFile(destination, await readFile(new URL(path, projectRoot)))
  }
  const pkg = JSON.parse(
    await readFile(join(fixtureRoot, "package.json"), "utf8")
  )
  const original = await readGenerated()
  const nextVersion = "99.88.77"
  pkg.version = nextVersion
  await writeFile(
    join(fixtureRoot, "package.json"),
    `${JSON.stringify(pkg, null, 2)}\n`
  )

  const staleCheck = runSync("--check")
  assert.notEqual(staleCheck.status, 0)
  assert.match(staleCheck.stderr, /版本配置尚未同步/)
  assert.deepEqual(await readGenerated(), original, "检查模式不能改写文件")

  const sync = runSync()
  assert.equal(sync.status, 0, sync.stderr)
  const synced = await readGenerated()
  const lock = JSON.parse(synced[0])
  const originalLock = JSON.parse(original[0])
  assert.equal(lock.version, nextVersion)
  assert.equal(lock.packages[""].version, nextVersion)
  lock.version = originalLock.version
  lock.packages[""].version = originalLock.packages[""].version
  assert.deepEqual(lock, originalLock, "同步不能改动 npm 依赖或其他锁文件信息")
  assert.equal(
    synced[1],
    original[1].replace(/(^version\s*=\s*")[^"]+(")/m, `$1${nextVersion}$2`)
  )
  assert.equal(
    synced[2],
    original[2].replace(
      /(name = "monkey-thief"\r?\nversion = ")[^"]+(")/,
      `$1${nextVersion}$2`
    )
  )
  assert.equal(
    synced[3],
    original[3].replace(/^## [^\r\n]+/, `## ${nextVersion} 变更说明`)
  )
  assert.equal(runSync("--check").status, 0)
  const tauri = JSON.parse(
    await readFile(join(fixtureRoot, "src-tauri/tauri.conf.json"), "utf8")
  )
  assert.equal(tauri.version, "../package.json")

  // 无变化时保留修改时间，避免重复启动触发 Cargo 和前端的无意义重建。
  const oldTime = new Date("2000-01-01T00:00:00Z")
  for (const path of generatedPaths)
    await utimes(join(fixtureRoot, path), oldTime, oldTime)
  assert.equal(runSync().status, 0)
  for (const path of generatedPaths) {
    assert.equal(
      (await stat(join(fixtureRoot, path))).mtimeMs,
      oldTime.getTime(),
      path
    )
  }

  pkg.version = "invalid"
  await writeFile(join(fixtureRoot, "package.json"), JSON.stringify(pkg))
  assert.notEqual(runSync().status, 0)
  assert.deepEqual(await readGenerated(), synced, "无效版本不能改写同步副本")
  pkg.version = "99.88.78"
  await writeFile(join(fixtureRoot, "package.json"), JSON.stringify(pkg))
  const brokenCargo = synced[1].replace(/^version = "[^"]+"\r?\n/m, "")
  await writeFile(join(fixtureRoot, "src-tauri/Cargo.toml"), brokenCargo)
  const broken = await readGenerated()
  const brokenSync = runSync()
  assert.notEqual(brokenSync.status, 0)
  assert.match(brokenSync.stderr, /Cargo\.toml 缺少/)
  assert.deepEqual(
    await readGenerated(),
    broken,
    "目标配置缺失时不能只同步部分文件"
  )
  await writeFile(join(fixtureRoot, "src-tauri/Cargo.toml"), synced[1])

  // 实际执行 npm 的生命周期钩子，只替换耗时的构建主体与辅助打包。
  for (const command of [
    "dev",
    "dev:bounded",
    "dev:renderer",
    "build",
    "build:renderer",
    "dist:win"
  ]) {
    pkg.scripts[command] = "node scripts/test-command.mjs"
  }
  for (const path of [
    "test-command.mjs",
    "build-translation-agent.mjs",
    "build-assistant.mjs"
  ]) {
    await writeFile(
      join(fixtureRoot, "scripts", path),
      "// 隔离版本同步验证中的构建操作。\n"
    )
  }
  let patch = 78
  for (const command of [
    "dev",
    "dev:bounded",
    "dev:renderer",
    "build",
    "build:renderer",
    "dist:win"
  ]) {
    pkg.version = `99.88.${patch++}`
    await writeFile(join(fixtureRoot, "package.json"), JSON.stringify(pkg))
    const result =
      process.platform === "win32"
        ? spawnSync(
            process.env.ComSpec,
            ["/d", "/s", "/c", `npm run ${command}`],
            { cwd: fixtureRoot, encoding: "utf8" }
          )
        : spawnSync("npm", ["run", command], {
            cwd: fixtureRoot,
            encoding: "utf8"
          })
    assert.equal(
      result.status,
      0,
      `${command}\n${result.stdout}\n${result.stderr}`
    )
    assert.equal(runSync("--check").status, 0, `${command} 必须自动同步版本`)
  }
  console.log(
    "版本同步通过：单一入口更新、只读检查、依赖与正文保留、重复执行不写入、错误不部分同步、6 个 npm 启动和构建入口"
  )
} finally {
  // 只清理本次创建且仍位于系统临时目录下的隔离目录。
  const cleanupRoot = await realpath(fixtureRoot)
  assert.equal(dirname(cleanupRoot), fixtureParent)
  assert.ok(basename(cleanupRoot).startsWith("ai-manager-version-"))
  await rm(cleanupRoot, { recursive: true, force: true })
}
