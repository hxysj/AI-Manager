import { readFile, writeFile } from "node:fs/promises"
import { fileURLToPath } from "node:url"

const projectRoot = new URL("../", import.meta.url)
const { version } = JSON.parse(
  await readFile(new URL("package.json", projectRoot), "utf8")
)
if (
  typeof version !== "string" ||
  !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)
) {
  throw new Error(
    "package.json 的 version 必须为有效的主版本.次版本.修订版本，例如 1.2.3。"
  )
}

// package.json 是唯一维护入口，其他版本值只作为构建工具需要的同步副本。
const paths = [
  "package-lock.json",
  "src-tauri/Cargo.toml",
  "src-tauri/Cargo.lock",
  "build/release-notes.md"
]
const contents = await Promise.all(
  paths.map((path) => readFile(new URL(path, projectRoot), "utf8"))
)
const packageLock = JSON.parse(contents[0])
packageLock.version = version
packageLock.packages[""].version = version
const replacements = [
  `${JSON.stringify(packageLock, null, 2)}\n`.replace(
    /\n/g,
    contents[0].includes("\r\n") ? "\r\n" : "\n"
  ),
  contents[1].replace(
    /(\[package\][^\[]*?^version\s*=\s*")[^"]+(")/m,
    (_, before, after) => `${before}${version}${after}`
  ),
  contents[2].replace(
    /(\[\[package\]\]\r?\nname = "monkey-thief"\r?\nversion = ")[^"]+(")/,
    (_, before, after) => `${before}${version}${after}`
  ),
  contents[3].replace(
    /^## \d+\.\d+\.\d+ 变更说明(?=\r?$)/m,
    `## ${version} 变更说明`
  )
]

// 先核对所有写入位置，配置缺失时不会只同步一部分文件。
if (!/^\[package\][^\[]*?^version\s*=\s*"[^"]+"/m.test(contents[1])) {
  throw new Error("src-tauri/Cargo.toml 缺少应用的 package.version。")
}
if (
  !/\[\[package\]\]\r?\nname = "monkey-thief"\r?\nversion = "[^"]+"/.test(
    contents[2]
  )
) {
  throw new Error("src-tauri/Cargo.lock 缺少 monkey-thief 应用版本。")
}
if (!/^## \d+\.\d+\.\d+ 变更说明\r?$/m.test(contents[3])) {
  throw new Error("build/release-notes.md 缺少版本变更说明标题。")
}
const tauriConfig = JSON.parse(
  await readFile(new URL("src-tauri/tauri.conf.json", projectRoot), "utf8")
)
if (tauriConfig.version !== "../package.json") {
  throw new Error(
    "Tauri 的 version 必须引用 ../package.json，不能单独维护版本号。"
  )
}

const changed = paths.filter(
  (_, index) => contents[index] !== replacements[index]
)
if (process.argv.includes("--check")) {
  if (changed.length)
    throw new Error(
      `版本配置尚未同步，请运行 npm run sync:version：${changed.join("、")}`
    )
  console.log(`应用版本配置一致：${version}`)
} else {
  for (let index = 0; index < paths.length; index += 1) {
    if (contents[index] === replacements[index]) continue
    await writeFile(
      new URL(paths[index], projectRoot),
      replacements[index],
      "utf8"
    )
    console.log(
      `已同步版本 ${version}：${fileURLToPath(new URL(paths[index], projectRoot))}`
    )
  }
}
