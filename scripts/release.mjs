import { createHash } from "node:crypto"
import {
  appendFileSync,
  copyFileSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync
} from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

export const releaseTargets = {
  "x86_64-pc-windows-msvc": {
    platform: "windows-x86_64",
    folder: "nsis",
    extension: ".exe",
    name: "Monkey-Thief-windows-setup.exe"
  },
  "aarch64-apple-darwin": {
    platform: "darwin-aarch64",
    folder: "macos",
    extension: ".app.tar.gz",
    name: "Monkey-Thief-macos-aarch64.app.tar.gz"
  },
  "x86_64-apple-darwin": {
    platform: "darwin-x86_64",
    folder: "macos",
    extension: ".app.tar.gz",
    name: "Monkey-Thief-macos-x86_64.app.tar.gz"
  },
  "x86_64-unknown-linux-gnu": {
    platform: "linux-x86_64",
    folder: "appimage",
    extension: ".AppImage.tar.gz",
    name: "Monkey-Thief-linux-x86_64.AppImage.tar.gz"
  }
}

export function prepareRelease(root, environment = process.env) {
  const read = (name) => readFileSync(path.join(root, name), "utf8")
  const pkg = JSON.parse(read("package.json"))
  const lock = JSON.parse(read("package-lock.json"))
  const cargo = read("src-tauri/Cargo.toml").match(
    /\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m
  )?.[1]
  const cargoLock = read("src-tauri/Cargo.lock").match(
    /name = "monkey-thief"\r?\nversion = "([^"]+)"/
  )?.[1]
  const versions = [
    lock.version,
    lock.packages[""].version,
    cargo,
    cargoLock,
    JSON.parse(read("src-tauri/tauri.conf.json")).version
  ]
  if (
    !/^\d+\.\d+\.\d+$/.test(pkg.version) ||
    versions.some((version) => version !== pkg.version)
  ) {
    throw new Error("package、锁文件、Cargo 与 Tauri 版本必须一致")
  }
  const tag = `v${pkg.version}`
  if (
    environment.GITHUB_REF_TYPE === "tag" &&
    environment.GITHUB_REF_NAME !== tag
  ) {
    throw new Error(`发布标签必须为 ${tag}`)
  }
  const notes = read("build/release-notes.md").replaceAll("\r\n", "\n").trim()
  if (
    !notes.startsWith(`## ${pkg.version} 变更说明\n`) ||
    notes.includes("请在发布前删除此行")
  ) {
    throw new Error("请更新当前版本的发布说明")
  }
  return { version: pkg.version, tag, notes }
}

function filesIn(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name)
    return entry.isDirectory() ? filesIn(file) : entry.isFile() ? [file] : []
  })
}

function singleFile(directory, extension) {
  const candidates = filesIn(directory).filter((file) =>
    file.endsWith(extension)
  )
  if (candidates.length !== 1)
    throw new Error(
      `${directory} 应包含一个 ${extension} 文件，实际为 ${candidates.length}`
    )
  return candidates[0]
}

export function stageRelease(root, target, bundleDirectory, outputDirectory) {
  const settings = releaseTargets[target]
  if (!settings) throw new Error(`未知发布目标：${target}`)
  const release = prepareRelease(root)
  const update = singleFile(
    path.join(bundleDirectory, settings.folder),
    settings.extension
  )
  const signature = readFileSync(`${update}.sig`, "utf8").trim()
  if (!signature) throw new Error("更新包签名不能为空")
  mkdirSync(outputDirectory, { recursive: true })
  copyFileSync(update, path.join(outputDirectory, settings.name))
  writeFileSync(
    path.join(outputDirectory, `${settings.name}.sig`),
    `${signature}\n`
  )
  const packages = []
  if (settings.platform.startsWith("darwin")) {
    copyFileSync(
      singleFile(path.join(bundleDirectory, "dmg"), ".dmg"),
      path.join(outputDirectory, settings.name.replace(".app.tar.gz", ".dmg"))
    )
  }
  if (settings.platform.startsWith("linux")) {
    const appImage = singleFile(
      path.join(bundleDirectory, settings.folder),
      ".AppImage"
    )
    const deb = singleFile(path.join(bundleDirectory, "deb"), ".deb")
    const debSignature = readFileSync(`${deb}.sig`, "utf8").trim()
    if (!debSignature) throw new Error("DEB 更新包签名不能为空")
    copyFileSync(
      appImage,
      path.join(
        outputDirectory,
        settings.name.replace(".AppImage.tar.gz", ".AppImage")
      )
    )
    const name = "Monkey-Thief-linux-x86_64.deb"
    copyFileSync(deb, path.join(outputDirectory, name))
    writeFileSync(
      path.join(outputDirectory, `${name}.sig`),
      `${debSignature}\n`
    )
    packages.push({
      platform: "linux-x86_64-deb",
      file: name,
      signature: debSignature
    })
  }
  // 每个平台只写自己的片段，最后统一合并，避免并行覆盖 latest.json。
  writeFileSync(
    path.join(outputDirectory, `${settings.platform}.json`),
    JSON.stringify(
      {
        version: release.version,
        platform: settings.platform,
        file: settings.name,
        signature,
        packages
      },
      null,
      2
    )
  )
}

export function mergeRelease(
  root,
  inputDirectory,
  outputDirectory,
  repository = "hxysj/AI-Manager"
) {
  const release = prepareRelease(root)
  mkdirSync(outputDirectory, { recursive: true })
  const files = filesIn(inputDirectory)
  const platforms = {}
  for (const settings of Object.values(releaseTargets)) {
    const matches = files.filter(
      (file) => path.basename(file) === `${settings.platform}.json`
    )
    if (matches.length !== 1)
      throw new Error(`缺少或重复的平台片段：${settings.platform}`)
    const fragment = JSON.parse(readFileSync(matches[0], "utf8"))
    if (
      fragment.version !== release.version ||
      fragment.platform !== settings.platform ||
      fragment.file !== settings.name
    ) {
      throw new Error(`平台片段与当前版本不一致：${settings.platform}`)
    }
    const binary = path.join(path.dirname(matches[0]), settings.name)
    const signature = readFileSync(`${binary}.sig`, "utf8").trim()
    if (!signature || signature !== fragment.signature)
      throw new Error(`更新签名不一致：${settings.platform}`)
    // 读取实体文件，不能发布只包含元数据的无效更新清单。
    if (!readFileSync(binary).length)
      throw new Error(`更新包为空：${settings.platform}`)
    const platform = {
      signature,
      url: `https://github.com/${repository}/releases/download/${release.tag}/${settings.name}`
    }
    platforms[settings.platform] = platform
    if (settings.platform.startsWith("windows"))
      platforms[`${settings.platform}-nsis`] = platform
    if (settings.platform.startsWith("linux"))
      platforms[`${settings.platform}-appimage`] = platform
    if (settings.platform.startsWith("linux")) {
      const deb = fragment.packages?.find(
        (item) => item.platform === "linux-x86_64-deb"
      )
      if (deb?.file !== "Monkey-Thief-linux-x86_64.deb")
        throw new Error("缺少 DEB 更新片段")
      const file = path.join(path.dirname(binary), deb.file)
      const signature = readFileSync(`${file}.sig`, "utf8").trim()
      if (
        !signature ||
        signature !== deb.signature ||
        !readFileSync(file).length
      )
        throw new Error("DEB 更新包或签名无效")
      platforms["linux-x86_64-deb"] = {
        signature,
        url: `https://github.com/${repository}/releases/download/${release.tag}/${deb.file}`
      }
    }
    for (const file of filesIn(path.dirname(binary)).filter(
      (file) => !file.endsWith(".json")
    )) {
      copyFileSync(file, path.join(outputDirectory, path.basename(file)))
    }
  }
  const pubDate = new Date().toISOString()
  writeFileSync(
    path.join(outputDirectory, "latest.json"),
    JSON.stringify(
      {
        version: release.version,
        notes: release.notes,
        pub_date: pubDate,
        platforms
      },
      null,
      2
    )
  )
  // 保留旧版 Electron Windows 客户端的升级入口。
  const installer = readFileSync(
    path.join(outputDirectory, releaseTargets["x86_64-pc-windows-msvc"].name)
  )
  const sha512 = createHash("sha512").update(installer).digest("base64")
  writeFileSync(
    path.join(outputDirectory, "latest.yml"),
    [
      `version: ${release.version}`,
      "files:",
      "  - url: Monkey-Thief-windows-setup.exe",
      `    sha512: ${sha512}`,
      `    size: ${installer.length}`,
      "path: Monkey-Thief-windows-setup.exe",
      `sha512: ${sha512}`,
      "releaseNotes: |",
      ...release.notes.split("\n").map((line) => `  ${line}`),
      `releaseDate: '${pubDate}'`,
      ""
    ].join("\n")
  )
  return platforms
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const root = path.resolve(fileURLToPath(new URL("..", import.meta.url)))
  const [command, ...args] = process.argv.slice(2)
  if (command === "prepare") {
    const { tag } = prepareRelease(root)
    if (process.env.GITHUB_OUTPUT)
      appendFileSync(process.env.GITHUB_OUTPUT, `tag=${tag}\n`)
    console.log(tag)
  } else if (command === "stage") {
    stageRelease(root, args[0], path.resolve(args[1]), path.resolve(args[2]))
  } else if (command === "merge") {
    mergeRelease(
      root,
      path.resolve(args[0]),
      path.resolve(args[1]),
      process.env.GITHUB_REPOSITORY
    )
  } else {
    throw new Error(
      "使用方式：release.mjs prepare | stage <target> <bundle> <output> | merge <input> <output>"
    )
  }
}
