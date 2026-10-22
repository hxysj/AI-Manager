import fs from 'node:fs/promises'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import { spawn, spawnSync } from 'node:child_process'

const preferredPort = 13134

const shouldAutoPrune = process.argv.includes('--auto-prune') || process.env.AI_MANAGER_AUTO_PRUNE_TARGET === '1'
const targetMaxGb = Number(process.env.AI_MANAGER_TARGET_MAX_GB || '30')
const targetOlderThanDays = Number(process.env.AI_MANAGER_TARGET_OLDER_THAN_DAYS || '1')

if (shouldAutoPrune) {
  if (!Number.isFinite(targetMaxGb) || targetMaxGb <= 0) {
    throw new Error('AI_MANAGER_TARGET_MAX_GB 必须是大于 0 的数字。')
  }
  if (!Number.isFinite(targetOlderThanDays) || targetOlderThanDays < 0) {
    throw new Error('AI_MANAGER_TARGET_OLDER_THAN_DAYS 必须是大于等于 0 的数字。')
  }

  const cleanupScript = path.resolve('scripts/clean-rust-target.mjs')
  const cleanupResult = spawnSync(
    process.execPath,
    [
      cleanupScript,
      '--apply',
      '--max-size-gb',
      String(targetMaxGb),
      '--older-than-days',
      String(targetOlderThanDays),
    ],
    { stdio: 'inherit' },
  )
  if (cleanupResult.error) throw cleanupResult.error
  if (cleanupResult.status !== 0) {
    throw new Error(`Rust 构建缓存清理失败，退出码：${cleanupResult.status}`)
  }
}

function isPortAvailable(port) {
  return new Promise((resolve) => {
    const server = net.createServer()
    server.once('error', () => resolve(false))
    server.listen({ host: '127.0.0.1', port }, () => {
      server.close(() => resolve(true))
    })
  })
}

async function findAvailablePort(startPort) {
  for (let port = startPort; port <= 65535; port += 1) {
    if (await isPortAvailable(port)) return port
  }
  throw new Error(`从 ${startPort} 开始没有可用的开发端口。`)
}

const port = await findAvailablePort(preferredPort)
const configPath = path.join(os.tmpdir(), `ai-manager-tauri-dev-${process.pid}.json`)
const config = {
  build: {
    devUrl: `http://127.0.0.1:${port}`
  }
}

await fs.writeFile(configPath, JSON.stringify(config), 'utf8')
console.log(`Tauri 开发服务使用 http://127.0.0.1:${port}`)

const tauriCli = path.resolve('node_modules/@tauri-apps/cli/tauri.js')
const child = spawn(process.execPath, [tauriCli, 'dev', '--config', configPath], {
  env: {
    ...process.env,
    AI_MANAGER_DEV_PORT: String(port),
    ...(shouldAutoPrune ? { CARGO_INCREMENTAL: '0' } : {}),
  },
  stdio: 'inherit'
})

const cleanup = async () => {
  await fs.rm(configPath, { force: true })
}
child.once('exit', async (code, signal) => {
  await cleanup()
  if (signal) process.kill(process.pid, signal)
  else process.exit(code ?? 1)
})
child.once('error', async (error) => {
  await cleanup()
  console.error(`无法启动 Tauri：${error.message}`)
  process.exit(1)
})
