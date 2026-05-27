const assert = require("node:assert/strict")
const fs = require("node:fs/promises")
const os = require("node:os")
const path = require("node:path")
const test = require("node:test")
const { ManagerService } = require("../electron/services/manager-service.cjs")

async function createService() {
  const userDataPath = await fs.mkdtemp(path.join(os.tmpdir(), "aim-test-"))
  const service = new ManagerService(userDataPath, {
    cliConfigPaths: {
      claude: path.join(userDataPath, ".claude"),
      codex: path.join(userDataPath, ".codex"),
      gemini: path.join(userDataPath, ".gemini")
    }
  })

  await fs.mkdir(service.paths.storageDir, { recursive: true })

  return service
}

async function writeJson(filePath, value) {
  await fs.mkdir(path.dirname(filePath), { recursive: true })
  await fs.writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8")
}

async function readJson(filePath) {
  return JSON.parse(await fs.readFile(filePath, "utf8"))
}

function createProvider(input = {}) {
  return {
    id: "provider-1",
    cli: "claude",
    name: "Claude",
    type: "anthropic",
    baseUrl: "https://api.current.test",
    runtimeConfig: {
      mainModel: "claude-sonnet-4"
    },
    ...input
  }
}

async function writeRuntimeStorage(service, provider) {
  await writeJson(service.paths.storageFiles.providers, [provider])
  await writeJson(service.paths.storageFiles.runtimeModels, [])
  await writeJson(service.paths.storageFiles.runtimeProfiles, [
    {
      id: "claude",
      cli: "claude",
      providerId: provider.id,
      model: "claude-sonnet-4"
    }
  ])
}

async function createBackup(provider) {
  const service = await createService()

  await writeRuntimeStorage(service, provider)

  return service.createDataBackup()
}

test("恢复配置不会因为非运行配置变化取消当前 Provider", async () => {
  const service = await createService()
  const currentProvider = createProvider()

  await writeRuntimeStorage(service, currentProvider)
  await service.runtimeProviderService.init()
  await service.codexAccountService.init()
  await service.codexProxyService.init()

  const content = await createBackup(
    createProvider({
      note: "只改变备注"
    })
  )

  await service.restoreDataBackup(content, {
    choices: {
      "json:storage/providers.json:provider-1": "backup"
    }
  })
  await service.storage.flush()

  assert.deepEqual(await readJson(service.paths.storageFiles.runtimeProfiles), [
    {
      id: "claude",
      cli: "claude",
      providerId: "provider-1",
      model: "claude-sonnet-4"
    }
  ])
})

test("恢复配置修改当前 Provider 运行配置时会取消启用", async () => {
  const service = await createService()
  const currentProvider = createProvider()

  await writeRuntimeStorage(service, currentProvider)
  await service.runtimeProviderService.init()
  await service.codexAccountService.init()
  await service.codexProxyService.init()

  const content = await createBackup(
    createProvider({
      baseUrl: "https://api.backup.test"
    })
  )

  await service.restoreDataBackup(content, {
    choices: {
      "json:storage/providers.json:provider-1": "backup"
    }
  })
  await service.storage.flush()

  assert.deepEqual(await readJson(service.paths.storageFiles.runtimeProfiles), [])
})

test("恢复配置不会因为账号展示信息变化取消当前官方账号", async () => {
  const service = await createService()
  const account = {
    id: "account-1",
    accountId: "chatgpt-account-1",
    email: "current@example.com",
    plan: "plus",
    auth: {
      accessToken: "access-current",
      refreshToken: "refresh-current",
      idToken: "id-current"
    }
  }

  await writeJson(service.paths.storageFiles.codexAccounts, [account])
  await writeJson(service.paths.storageFiles.codexActiveAccountId, "account-1")
  await service.runtimeProviderService.init()
  await service.codexAccountService.init()
  await service.codexProxyService.init()

  const backupService = await createService()
  await writeJson(backupService.paths.storageFiles.codexAccounts, [
    {
      ...account,
      plan: "pro"
    }
  ])
  const content = await backupService.createDataBackup()

  await service.restoreDataBackup(content, {
    choices: {
      "json:storage/codex-accounts.json:account-1": "backup"
    }
  })
  await service.storage.flush()

  assert.equal(
    await readJson(service.paths.storageFiles.codexActiveAccountId),
    "account-1"
  )
})

test("恢复配置修改当前官方账号认证内容时会取消启用", async () => {
  const service = await createService()
  const account = {
    id: "account-1",
    accountId: "chatgpt-account-1",
    email: "current@example.com",
    auth: {
      accessToken: "access-current",
      refreshToken: "refresh-current",
      idToken: "id-current"
    }
  }

  await writeJson(service.paths.storageFiles.codexAccounts, [account])
  await writeJson(service.paths.storageFiles.codexActiveAccountId, "account-1")
  await service.runtimeProviderService.init()
  await service.codexAccountService.init()
  await service.codexProxyService.init()

  const backupService = await createService()
  await writeJson(backupService.paths.storageFiles.codexAccounts, [
    {
      ...account,
      auth: {
        ...account.auth,
        accessToken: "access-backup"
      }
    }
  ])
  const content = await backupService.createDataBackup()

  await service.restoreDataBackup(content, {
    choices: {
      "json:storage/codex-accounts.json:account-1": "backup"
    }
  })
  await service.storage.flush()

  assert.equal(await readJson(service.paths.storageFiles.codexActiveAccountId), "")
})
