import fs from 'node:fs/promises'
import path from 'node:path'
import { spawnSync } from 'node:child_process'

const DEFAULT_MAX_SIZE_GB = 30
const DEFAULT_OLDER_THAN_DAYS = 1
const DAY_MS = 24 * 60 * 60 * 1000

function parseArgs(argv) {
  const options = {
    apply: false,
    maxSizeGb: DEFAULT_MAX_SIZE_GB,
    olderThanDays: DEFAULT_OLDER_THAN_DAYS,
    targetRoot: process.env.CARGO_TARGET_DIR || 'src-tauri/target',
  }

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]
    if (argument === '--apply') {
      options.apply = true
      continue
    }
    if (argument === '--dry-run') {
      options.apply = false
      continue
    }
    if (argument === '--max-size-gb' || argument === '--older-than-days' || argument === '--target-root') {
      const value = argv[index + 1]
      if (!value) throw new Error(`${argument} 缺少参数。`)
      index += 1
      if (argument === '--max-size-gb') options.maxSizeGb = Number(value)
      else if (argument === '--older-than-days') options.olderThanDays = Number(value)
      else options.targetRoot = value
      continue
    }
    if (argument === '--help' || argument === '-h') {
      console.log(`用法：
  node scripts/clean-rust-target.mjs [--dry-run]
  node scripts/clean-rust-target.mjs --apply --max-size-gb 30 --older-than-days ${DEFAULT_OLDER_THAN_DAYS}

参数：
  --apply             执行清理；默认只统计和预览
  --max-size-gb N    target 总大小上限，默认 ${DEFAULT_MAX_SIZE_GB} GB
  --older-than-days N 只处理 N 天未修改的缓存，默认 ${DEFAULT_OLDER_THAN_DAYS} 天
  --target-root PATH  指定 Cargo target 目录
`)
      process.exit(0)
    }
    throw new Error(`未知参数：${argument}`)
  }

  if (!Number.isFinite(options.maxSizeGb) || options.maxSizeGb <= 0) {
    throw new Error('--max-size-gb 必须是大于 0 的数字。')
  }
  if (!Number.isFinite(options.olderThanDays) || options.olderThanDays < 0) {
    throw new Error('--older-than-days 必须是大于等于 0 的数字。')
  }
  return options
}

async function walkFiles(root) {
  const files = []
  const pending = [root]
  while (pending.length > 0) {
    const current = pending.pop()
    let entries
    try {
      entries = await fs.readdir(current, { withFileTypes: true })
    } catch {
      continue
    }
    for (const entry of entries) {
      const fullPath = path.join(current, entry.name)
      if (entry.isDirectory()) {
        pending.push(fullPath)
      } else if (entry.isFile()) {
        try {
          const stat = await fs.stat(fullPath)
          files.push({ path: fullPath, size: stat.size, mtimeMs: stat.mtimeMs })
        } catch {
          // 构建过程中可能有文件被 Cargo 移除，忽略该文件即可。
        }
      }
    }
  }
  return files
}

async function directorySize(root) {
  const files = await walkFiles(root)
  return files.reduce((total, file) => total + file.size, 0)
}

function formatBytes(bytes) {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(2)} MB`
  return `${(bytes / 1024).toFixed(2)} KB`
}

function isOld(file, cutoffMs) {
  return file.mtimeMs < cutoffMs
}

function hasActiveRustBuild() {
  const command = process.platform === 'win32' ? 'tasklist' : 'ps'
  const args = process.platform === 'win32' ? ['/FO', 'CSV', '/NH'] : ['-eo', 'comm=']
  const result = spawnSync(command, args, { encoding: 'utf8', windowsHide: true })
  if (result.error || result.status !== 0) return false
  const buildProcessNames = process.platform === 'win32'
    ? ['cargo.exe', 'rustc.exe']
    : ['cargo', 'rustc']
  const output = result.stdout.toLowerCase()
  return buildProcessNames.some((name) => output.includes(name))
}

async function removeFile(filePath) {
  try {
    await fs.rm(filePath, { force: true })
    return true
  } catch (error) {
    console.warn(`跳过无法删除的缓存：${filePath}（${error.message}）`)
    return false
  }
}

async function removeDirectory(directoryPath) {
  try {
    await fs.rm(directoryPath, { recursive: true, force: true })
    return true
  } catch (error) {
    console.warn(`跳过无法删除的增量缓存：${directoryPath}（${error.message}）`)
    return false
  }
}

async function pruneIncrementalDirectories(debugRoot, cutoffMs, apply) {
  const incrementalRoot = path.join(debugRoot, 'incremental')
  let entries
  try {
    entries = await fs.readdir(incrementalRoot, { withFileTypes: true })
  } catch {
    return { bytes: 0, count: 0 }
  }

  let bytes = 0
  let count = 0
  for (const entry of entries) {
    if (!entry.isDirectory()) continue
    const directoryPath = path.join(incrementalRoot, entry.name)
    let stat
    try {
      stat = await fs.stat(directoryPath)
    } catch {
      continue
    }
    if (!isOld({ mtimeMs: stat.mtimeMs }, cutoffMs)) continue
    const size = await directorySize(directoryPath)
    bytes += size
    count += 1
    if (apply) await removeDirectory(directoryPath)
  }
  return { bytes, count }
}

async function main() {
  const options = parseArgs(process.argv.slice(2))
  if (options.apply && hasActiveRustBuild()) {
    throw new Error('检测到 Cargo 或 rustc 正在运行，请先关闭 Tauri、cargo test 和其他 Rust 构建进程。')
  }
  const targetRoot = path.resolve(options.targetRoot)
  const debugRoot = path.join(targetRoot, 'debug')
  const cutoffMs = Date.now() - options.olderThanDays * DAY_MS
  const maxBytes = options.maxSizeGb * 1024 ** 3

  const beforeFiles = await walkFiles(targetRoot)
  const beforeBytes = beforeFiles.reduce((total, file) => total + file.size, 0)
  console.log(`Rust 构建目录：${targetRoot}`)
  console.log(`当前占用：${formatBytes(beforeBytes)}；上限：${options.maxSizeGb.toFixed(2)} GB`)

  if (beforeBytes <= maxBytes) {
    console.log('当前占用未超过上限，无需清理。')
    return
  }

  const incremental = await pruneIncrementalDirectories(debugRoot, cutoffMs, options.apply)
  let plannedBytes = incremental.bytes
  let removedFiles = 0
  const candidates = beforeFiles
    .filter((file) => file.path.includes(`${path.sep}debug${path.sep}deps${path.sep}`))
    .filter((file) => isOld(file, cutoffMs))
    .sort((left, right) => left.mtimeMs - right.mtimeMs)

  for (const candidate of candidates) {
    if (beforeBytes - plannedBytes <= maxBytes) break
    plannedBytes += candidate.size
    removedFiles += 1
    if (options.apply) await removeFile(candidate.path)
  }

  if (!options.apply) {
    console.log(`预览：将清理约 ${formatBytes(plannedBytes)}（增量目录 ${incremental.count} 个，依赖中间文件 ${removedFiles} 个）。`)
    console.log('当前是预览模式；确认已关闭 Tauri/Cargo 后，再加 --apply 执行。')
    return
  }

  const afterBytes = await directorySize(targetRoot)
  console.log(`已清理约 ${formatBytes(Math.max(0, beforeBytes - afterBytes))}，当前占用：${formatBytes(afterBytes)}。`)
  if (afterBytes > maxBytes) {
    console.warn(`仍高于上限，原因是 ${options.olderThanDays} 天内的产物被保留；可在停止开发进程后降低 --older-than-days。`)
  }
}

main().catch((error) => {
  console.error(`Rust 构建缓存清理失败：${error.message}`)
  process.exitCode = 1
})
