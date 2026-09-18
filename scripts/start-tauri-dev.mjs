import fs from 'node:fs/promises'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import { spawn } from 'node:child_process'

const preferredPort = 13134

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
  env: { ...process.env, AI_MANAGER_DEV_PORT: String(port) },
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
