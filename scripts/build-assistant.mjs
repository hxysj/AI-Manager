import { build } from "esbuild"
import { cp, mkdir, readFile } from "node:fs/promises"
import { createRequire } from "node:module"
import { dirname, join } from "node:path"
import { createHash } from "node:crypto"
import { currentTarget } from "@neon-rs/load"

const require = createRequire(import.meta.url)
const directory = "src-tauri/node/assistant"
await mkdir(directory, { recursive: true })

// 框架与 JS 依赖合并为一个后台入口，安装版无需依赖项目 node_modules。
await build({
  entryPoints: ["src-tauri/node/assistant-service.mjs"],
  outfile: `${directory}/agent.mjs`,
  bundle: true,
  platform: "node",
  format: "esm",
  target: "node22",
  banner: {
    js: 'import { createRequire as __createRequire } from "node:module"; const require = __createRequire(import.meta.url);'
  },
  logLevel: "warning"
})

// libSQL 的平台二进制按原包布局复制，动态 require 在安装目录内解析。
const nativePackage = `@libsql/${currentTarget()}`
const nativeRoot = dirname(require.resolve(`${nativePackage}/package.json`))
const nativeMeta = JSON.parse(
  await readFile(join(nativeRoot, "package.json"), "utf8")
)
if (!nativeMeta.main) throw new Error("libSQL 平台包缺少原生入口")
const nativeDestination = join(directory, "node_modules", nativePackage)
// Windows 会锁住已加载的原生模块；依赖未变化时保留原文件，允许应用运行中重新构建。
let sameNative = true
for (const filename of ["package.json", nativeMeta.main]) {
  try {
    const source = await readFile(join(nativeRoot, filename))
    const destination = await readFile(join(nativeDestination, filename))
    if (createHash("sha256").update(source).digest("hex") !== createHash("sha256").update(destination).digest("hex")) sameNative = false
  } catch (error) {
    if (error.code !== "ENOENT") throw error
    sameNative = false
  }
}
if (!sameNative) await cp(nativeRoot, nativeDestination, { recursive: true })
