const test = require('node:test')
const assert = require('node:assert/strict')
const fs = require('node:fs/promises')
const os = require('node:os')
const path = require('node:path')
const { ManagerService } = require('../electron/services/manager-service.cjs')
const { JsonStorage } = require('../electron/services/json-storage.cjs')

async function createTempDir(prefix) {
  return fs.mkdtemp(path.join(os.tmpdir(), prefix))
}

test('ManagerService can initialize a clean workspace', async () => {
  const root = await createTempDir('ai-manager-manager-')
  const service = new ManagerService(root)

  await service.init()

  const state = service.getState()

  assert.equal(Array.isArray(state.cliTargets), true)
  assert.equal(Array.isArray(state.skills), true)
  assert.equal(Array.isArray(state.repos), true)
  assert.equal(typeof state.paths.workspaceRoot, 'string')
  assert.equal(state.paths.workspaceRoot.endsWith(path.join('workspace')), true)

  await service.dispose()
})

test('JsonStorage flush persists scheduled writes', async () => {
  const root = await createTempDir('ai-manager-storage-')
  const storage = new JsonStorage({
    repos: path.join(root, 'repos.json')
  }, 300)

  storage.scheduleWrite('repos', [{ id: 'repo-1', name: 'demo' }])
  await storage.flush()

  const content = JSON.parse(await fs.readFile(path.join(root, 'repos.json'), 'utf8'))

  assert.deepEqual(content, [{ id: 'repo-1', name: 'demo' }])
})
