import assert from "node:assert/strict"
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync
} from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"
import { fileURLToPath } from "node:url"
import {
  mergeRelease,
  prepareRelease,
  releaseTargets,
  stageRelease
} from "./release.mjs"

const root = path.resolve(fileURLToPath(new URL("..", import.meta.url)))
const temporary = mkdtempSync(path.join(tmpdir(), "ai-manager-platform-"))
try {
  const version = JSON.parse(
    readFileSync(path.join(root, "package.json"))
  ).version
  // 发布测试使用独立夹具，不要求开发中的 release-notes 已准备发布。
  for (const name of [
    "package.json",
    "package-lock.json",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/tauri.conf.json"
  ]) {
    const output = path.join(temporary, name)
    mkdirSync(path.dirname(output), { recursive: true })
    writeFileSync(output, readFileSync(path.join(root, name)))
  }
  mkdirSync(path.join(temporary, "build"))
  writeFileSync(
    path.join(temporary, "build/release-notes.md"),
    `## ${version} 变更说明\n\n多平台测试。\n`
  )
  assert.equal(prepareRelease(temporary, {}).tag, `v${version}`)
  assert.throws(() =>
    prepareRelease(temporary, {
      GITHUB_REF_TYPE: "tag",
      GITHUB_REF_NAME: "v0.0.0"
    })
  )
  for (const [target, settings] of Object.entries(releaseTargets)) {
    const bundle = path.join(temporary, target)
    const directory = path.join(bundle, settings.folder)
    mkdirSync(directory, { recursive: true })
    writeFileSync(
      path.join(directory, `应用${settings.extension}`),
      `binary:${target}`
    )
    writeFileSync(
      path.join(directory, `应用${settings.extension}.sig`),
      `signature:${target}\n`
    )
    for (const [folder, extension] of settings.platform.startsWith("darwin")
      ? [["dmg", ".dmg"]]
      : settings.platform.startsWith("linux")
        ? [["deb", ".deb"]]
        : []) {
      mkdirSync(path.join(bundle, folder))
      writeFileSync(path.join(bundle, folder, `应用${extension}`), "installer")
      if (extension === ".deb")
        writeFileSync(
          path.join(bundle, folder, `应用${extension}.sig`),
          "deb-signature"
        )
    }
    stageRelease(
      temporary,
      target,
      bundle,
      path.join(temporary, "artifacts", target)
    )
  }
  const platforms = mergeRelease(
    temporary,
    path.join(temporary, "artifacts"),
    path.join(temporary, "release")
  )
  assert.equal(Object.keys(platforms).length, 7)
  for (const settings of Object.values(releaseTargets)) {
    assert.ok(platforms[settings.platform].url.endsWith(settings.name))
  }
  assert.ok(
    readFileSync(path.join(temporary, "release/latest.yml"), "utf8").includes(
      "sha512:"
    )
  )
  const signature = path.join(
    temporary,
    "artifacts/x86_64-unknown-linux-gnu/Monkey-Thief-linux-x86_64.AppImage.sig"
  )
  writeFileSync(signature, "")
  assert.throws(
    () =>
      mergeRelease(
        temporary,
        path.join(temporary, "artifacts"),
        path.join(temporary, "invalid")
      ),
    /签名/
  )
  rmSync(path.dirname(signature), { recursive: true })
  assert.throws(
    () =>
      mergeRelease(
        temporary,
        path.join(temporary, "artifacts"),
        path.join(temporary, "missing")
      ),
    /片段/
  )
  const windows = JSON.parse(
    readFileSync(path.join(root, "src-tauri/tauri.windows.conf.json"))
  )
  const mac = JSON.parse(
    readFileSync(path.join(root, "src-tauri/tauri.macos.conf.json"))
  )
  const linux = JSON.parse(
    readFileSync(path.join(root, "src-tauri/tauri.linux.conf.json"))
  )
  assert.deepEqual(windows.bundle.targets, ["nsis"])
  assert.deepEqual(mac.bundle.targets, ["app", "dmg"])
  assert.deepEqual(linux.bundle.targets, ["appimage", "deb"])
  console.log("多平台发布、更新清单与缺包/缺签名测试通过")
} finally {
  // 临时目录由 mkdtemp 创建，只清理这次测试拥有的文件。
  rmSync(temporary, { recursive: true, force: true })
}
