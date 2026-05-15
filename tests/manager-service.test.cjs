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

test('ManagerService imports real CLI skills into managed workspace', async () => {
  const root = await createTempDir('ai-manager-import-')
  const userDataPath = path.join(root, 'data')
  const cliSkillsPath = path.join(root, 'cli-skills')
  const cliSkillRoot = path.join(cliSkillsPath, 'demo-skill')
  const cliTarget = {
    id: 'codex',
    type: 'codex',
    name: 'Codex',
    installed: true,
    configPath: path.join(root, 'codex'),
    skillsPath: cliSkillsPath,
    detectedAt: Date.now()
  }
  const service = new ManagerService(userDataPath)

  service.cliDetectionService = {
    detectAll: async () => [cliTarget],
    getAdapter: () => ({
      detect: async () => cliTarget
    })
  }

  await fs.mkdir(cliSkillRoot, { recursive: true })
  await fs.writeFile(
    path.join(cliSkillRoot, 'SKILL.md'),
    [
      '---',
      'name: demo-skill',
      'description: imported from cli',
      'entry: SKILL.md',
      '---',
      '',
      '# demo-skill',
      ''
    ].join('\n'),
    'utf8'
  )

  await service.init()
  await service.importSkillsFromCli()

  const state = service.getState()
  const skill = state.skills.find(item => item.name === 'demo-skill')
  const cliSkillStat = await fs.lstat(cliSkillRoot)

  assert.equal(Boolean(skill), true)
  assert.equal(skill.sourcePath, path.join(userDataPath, 'workspace', 'skills', 'demo-skill'))
  assert.equal(cliSkillStat.isSymbolicLink(), true)
  assert.equal(await fs.readFile(path.join(skill.sourcePath, 'SKILL.md'), 'utf8'), [
    '---',
    'name: demo-skill',
    'description: imported from cli',
    'entry: SKILL.md',
    '---',
    '',
    '# demo-skill',
    ''
  ].join('\n'))

  await service.dispose()
})

test('ManagerService deduplicates skills when importing from all CLI targets', async () => {
  const root = await createTempDir('ai-manager-import-dedupe-')
  const userDataPath = path.join(root, 'data')
  const codexSkillsPath = path.join(root, 'codex-skills')
  const claudeSkillsPath = path.join(root, 'claude-skills')
  const codexSkillRoot = path.join(codexSkillsPath, 'shared-skill')
  const claudeSkillRoot = path.join(claudeSkillsPath, 'shared-skill')
  const cliTargets = [
    {
      id: 'codex',
      type: 'codex',
      name: 'Codex',
      installed: true,
      configPath: path.join(root, 'codex'),
      skillsPath: codexSkillsPath,
      detectedAt: Date.now()
    },
    {
      id: 'claude',
      type: 'claude',
      name: 'Claude',
      installed: true,
      configPath: path.join(root, 'claude'),
      skillsPath: claudeSkillsPath,
      detectedAt: Date.now()
    }
  ]
  const service = new ManagerService(userDataPath)
  const skillContent = [
    '---',
    'name: shared-skill',
    'description: duplicated across cli',
    'entry: SKILL.md',
    '---',
    '',
    '# shared-skill',
    ''
  ].join('\n')

  service.cliDetectionService = {
    detectAll: async () => cliTargets,
    getAdapter: targetId => ({
      detect: async () => cliTargets.find(item => item.id === targetId)
    })
  }

  await fs.mkdir(codexSkillRoot, { recursive: true })
  await fs.mkdir(claudeSkillRoot, { recursive: true })
  await fs.writeFile(path.join(codexSkillRoot, 'SKILL.md'), skillContent, 'utf8')
  await fs.writeFile(path.join(claudeSkillRoot, 'SKILL.md'), skillContent, 'utf8')

  await service.init()
  await service.importSkillsFromCli()

  const state = service.getState()
  const importedSkills = state.skills.filter(item => item.name === 'shared-skill')
  const codexSkillStat = await fs.lstat(codexSkillRoot)
  const claudeSkillStat = await fs.lstat(claudeSkillRoot)
  const managedPath = path.join(userDataPath, 'workspace', 'skills', 'shared-skill')

  assert.equal(importedSkills.length, 1)
  assert.equal(importedSkills[0].sourcePath, managedPath)
  assert.equal(codexSkillStat.isSymbolicLink(), true)
  assert.equal(claudeSkillStat.isSymbolicLink(), true)
  assert.equal(await fs.realpath(codexSkillRoot), await fs.realpath(managedPath))
  assert.equal(await fs.realpath(claudeSkillRoot), await fs.realpath(managedPath))

  await service.dispose()
})
