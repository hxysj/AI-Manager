const assert = require("node:assert/strict")
const fs = require("node:fs/promises")
const net = require("node:net")
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

async function writeJsonl(filePath, records) {
  await fs.mkdir(path.dirname(filePath), { recursive: true })
  await fs.writeFile(
    filePath,
    `${records.map((record) => JSON.stringify(record)).join("\n")}\n`,
    "utf8"
  )
}

async function readJson(filePath) {
  return JSON.parse(await fs.readFile(filePath, "utf8"))
}

async function readText(filePath) {
  return fs.readFile(filePath, "utf8")
}

async function getFreePort() {
  const server = net.createServer()

  await new Promise((resolve, reject) => {
    server.once("error", reject)
    server.listen(0, "127.0.0.1", resolve)
  })

  const port = server.address().port

  await new Promise((resolve, reject) => {
    server.close(error => (error ? reject(error) : resolve()))
  })

  return port
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

function createCodexProvider(input = {}) {
  return {
    id: "codex-provider-1",
    cli: "codex",
    name: "Codex",
    type: "openai",
    baseUrl: "https://api.codex.test/v1",
    runtimeConfig: {
      mainModel: "gpt-5.2"
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

test("Claude 代理接管写入 Anthropic base_url，不追加 /v1", async () => {
  const service = await createService()
  const provider = createProvider()
  const port = await getFreePort()
  const settingsPath = path.join(
    service.appSettings.cliConfigPaths.claude,
    "settings.json"
  )

  try {
    await writeRuntimeStorage(service, provider)
    await writeJson(service.paths.storageFiles.runtimeProviderKeys, {})
    await writeJson(service.paths.storageFiles.claudeProxyConfig, { port })
    await service.runtimeProviderService.init()
    service.runtimeProviderService.keyManager.setProviderKey(provider.id, "sk-test")
    await service.storage.flush()
    await service.claudeProxyService.init()
    await service.claudeProxyService.addProvider({ providerId: provider.id })
    await writeJson(settingsPath, {
      env: {
        ANTHROPIC_MODEL: "old-model"
      },
      includeCoAuthoredBy: false
    })

    await service.claudeProxyService.enable(
      {},
      { id: "claude", configPath: service.appSettings.cliConfigPaths.claude }
    )

    const settings = await readJson(settingsPath)

    assert.equal(
      settings.env.ANTHROPIC_BASE_URL,
      `http://127.0.0.1:${port}`
    )
    assert.equal(settings.env.ANTHROPIC_AUTH_TOKEN, "PROXY_MANAGED")
    assert.equal(settings.env.ANTHROPIC_MODEL, "claude-sonnet-4")
    assert.equal(settings.includeCoAuthoredBy, false)
  } finally {
    await service.claudeProxyService.dispose()
  }
})

test("Codex 代理接管继续写入 OpenAI /v1 base_url", async () => {
  const service = await createService()
  const provider = createCodexProvider()
  const port = await getFreePort()
  const authPath = path.join(
    service.appSettings.cliConfigPaths.codex,
    "auth.json"
  )
  const configPath = path.join(
    service.appSettings.cliConfigPaths.codex,
    "config.toml"
  )

  try {
    await writeJson(service.paths.storageFiles.providers, [provider])
    await writeJson(service.paths.storageFiles.runtimeModels, [])
    await writeJson(service.paths.storageFiles.runtimeProfiles, [])
    await writeJson(service.paths.storageFiles.runtimeProviderKeys, {})
    await writeJson(service.paths.storageFiles.codexProxyConfig, { port })
    await service.runtimeProviderService.init()
    service.runtimeProviderService.keyManager.setProviderKey(provider.id, "sk-test")
    await service.storage.flush()
    await service.codexProxyService.init()
    await service.codexProxyService.addProvider({ providerId: provider.id })
    await writeJson(authPath, { OPENAI_API_KEY: "old" })
    await fs.mkdir(path.dirname(configPath), { recursive: true })
    await fs.writeFile(configPath, 'model_provider = "custom"\n', "utf8")

    await service.codexProxyService.enable(
      {},
      { id: "codex", configPath: service.appSettings.cliConfigPaths.codex }
    )

    assert.equal((await readJson(authPath)).OPENAI_API_KEY, "PROXY_MANAGED")
    assert.match(
      await readText(configPath),
      new RegExp(`base_url = "http://127\\.0\\.0\\.1:${port}/v1"`)
    )
  } finally {
    await service.codexProxyService.dispose()
  }
})

test("Claude 代理接管切换 Provider 时同步模型配置", async () => {
  const service = await createService()
  const firstProvider = createProvider({
    id: "claude-provider-1",
    runtimeConfig: {
      mainModel: "claude-sonnet-4",
      haikuModel: "claude-haiku-4"
    }
  })
  const secondProvider = createProvider({
    id: "claude-provider-2",
    baseUrl: "https://api.second.test",
    runtimeConfig: {
      mainModel: "claude-opus-4",
      sonnetModel: "claude-sonnet-4-5"
    }
  })
  const port = await getFreePort()
  const settingsPath = path.join(
    service.appSettings.cliConfigPaths.claude,
    "settings.json"
  )

  try {
    await writeJson(service.paths.storageFiles.providers, [
      firstProvider,
      secondProvider
    ])
    await writeJson(service.paths.storageFiles.runtimeModels, [])
    await writeJson(service.paths.storageFiles.runtimeProfiles, [])
    await writeJson(service.paths.storageFiles.runtimeProviderKeys, {})
    await writeJson(service.paths.storageFiles.claudeProxyConfig, { port })
    await service.runtimeProviderService.init()
    service.runtimeProviderService.keyManager.setProviderKey(
      firstProvider.id,
      "sk-first"
    )
    service.runtimeProviderService.keyManager.setProviderKey(
      secondProvider.id,
      "sk-second"
    )
    await service.storage.flush()
    await service.claudeProxyService.init()
    await service.claudeProxyService.addProvider({
      providerId: firstProvider.id
    })
    await service.claudeProxyService.addProvider({
      providerId: secondProvider.id
    })
    await writeJson(settingsPath, {
      env: {
        ANTHROPIC_MODEL: "old-model",
        ANTHROPIC_DEFAULT_HAIKU_MODEL: "old-haiku",
        ANTHROPIC_DEFAULT_SONNET_MODEL: "old-sonnet"
      }
    })

    await service.claudeProxyService.enable(
      {},
      { id: "claude", configPath: service.appSettings.cliConfigPaths.claude }
    )

    let settings = await readJson(settingsPath)

    assert.equal(settings.env.ANTHROPIC_MODEL, "claude-sonnet-4")
    assert.equal(settings.env.ANTHROPIC_DEFAULT_HAIKU_MODEL, "claude-haiku-4")
    assert.equal(settings.env.ANTHROPIC_DEFAULT_SONNET_MODEL, undefined)

    await service.claudeProxyService.updateActiveProvider(
      secondProvider.id,
      { id: "claude", configPath: service.appSettings.cliConfigPaths.claude }
    )
    settings = await readJson(settingsPath)

    assert.equal(settings.env.ANTHROPIC_MODEL, "claude-opus-4")
    assert.equal(settings.env.ANTHROPIC_DEFAULT_HAIKU_MODEL, undefined)
    assert.equal(
      settings.env.ANTHROPIC_DEFAULT_SONNET_MODEL,
      "claude-sonnet-4-5"
    )
  } finally {
    await service.claudeProxyService.dispose()
  }
})

test("Claude 和 Codex 代理接管池互相独立", async () => {
  const service = await createService()
  const claudeProvider = createProvider({
    id: "claude-provider-1"
  })
  const codexProvider = createCodexProvider({
    id: "codex-provider-1"
  })

  await writeJson(service.paths.storageFiles.providers, [
    claudeProvider,
    codexProvider
  ])
  await writeJson(service.paths.storageFiles.runtimeModels, [])
  await writeJson(service.paths.storageFiles.runtimeProfiles, [])
  await writeJson(service.paths.storageFiles.runtimeProviderKeys, {})
  await service.runtimeProviderService.init()
  service.runtimeProviderService.keyManager.setProviderKey(
    claudeProvider.id,
    "sk-claude"
  )
  service.runtimeProviderService.keyManager.setProviderKey(
    codexProvider.id,
    "sk-codex"
  )
  await service.storage.flush()
  await service.claudeProxyService.init()
  await service.codexProxyService.init()

  await service.claudeProxyService.addProvider({
    providerId: claudeProvider.id
  })
  await service.codexProxyService.addProvider({
    providerId: codexProvider.id
  })

  assert.deepEqual(
    service.claudeProxyService.getState().failoverProviderIds,
    [claudeProvider.id]
  )
  assert.deepEqual(
    service.codexProxyService.getState().failoverProviderIds,
    [codexProvider.id]
  )
  assert.deepEqual(
    (await readJson(service.paths.storageFiles.claudeProxyConfig))
      .failoverProviderIds,
    [claudeProvider.id]
  )
  assert.deepEqual(
    (await readJson(service.paths.storageFiles.codexProxyConfig))
      .failoverProviderIds,
    [codexProvider.id]
  )
  await assert.rejects(
    () =>
      service.claudeProxyService.addProvider({
        providerId: codexProvider.id
      }),
    /Claude Provider 不存在/
  )
  await assert.rejects(
    () =>
      service.codexProxyService.addProvider({
        providerId: claudeProvider.id
      }),
    /Codex Provider 不存在/
  )
})

test("Skill 使用统计扫描 Claude 和 Codex 聊天文件", async () => {
  const service = await createService()
  const claudeConfigPath = service.appSettings.cliConfigPaths.claude
  const codexConfigPath = service.appSettings.cliConfigPaths.codex
  const claudeSessionPath = path.join(
    claudeConfigPath,
    "projects",
    "project-a",
    "claude-session-1.jsonl"
  )
  const codexSessionPath = path.join(
    codexConfigPath,
    "sessions",
    "2026",
    "05",
    "rollout-2026-05-29T10-00-00-codex-session-1.jsonl"
  )

  await writeJsonl(path.join(claudeConfigPath, "history.jsonl"), [
    {
      display: "普通 Claude 输入",
      timestamp: 1770000001000,
      project: "project-a",
      sessionId: "claude-session-1"
    },
    {
      display: "/skill-alpha 第二次执行",
      timestamp: 1770000003000,
      project: "project-a",
      sessionId: "claude-session-1"
    }
  ])
  await writeJsonl(path.join(codexConfigPath, "history.jsonl"), [
    {
      session_id: "codex-session-1",
      ts: 1770000002,
      text: "普通 Codex 输入"
    }
  ])
  await writeJsonl(claudeSessionPath, [
    {
      type: "assistant",
      timestamp: "2026-02-02T02:40:02.000Z",
      message: {
        role: "assistant",
        content: [
          {
            type: "tool_use",
            input: {
              command: `Get-Content -Path ${path.join(
                claudeConfigPath,
                "skills",
                "skill-alpha",
                "SKILL.md"
              )}`
            }
          }
        ]
      }
    },
    {
      type: "assistant",
      timestamp: "2026-02-02T02:40:04.000Z",
      message: {
        role: "assistant",
        content: [
          {
            type: "tool_use",
            input: {
              command: `Get-Content -Path ${path.join(
                claudeConfigPath,
                "skills",
                "skill-gamma",
                "SKILL.md"
              )}`
            }
          }
        ]
      }
    }
  ])
  await writeJsonl(codexSessionPath, [
    {
      timestamp: "2026-02-02T02:40:01.000Z",
      type: "response_item",
      payload: {
        type: "message",
        role: "developer",
        content: [
          {
            type: "input_text",
            text: `可用 Skill：${path.join(
              codexConfigPath,
              "skills",
              "skill-delta",
              "SKILL.md"
            )}`
          }
        ]
      }
    },
    {
      timestamp: "2026-02-02T02:40:02.000Z",
      type: "response_item",
      payload: {
        type: "function_call",
        name: "shell_command",
        arguments: JSON.stringify({
          command: `Get-Content -Path ${path.join(
            codexConfigPath,
            "skills",
            "skill-beta",
            "SKILL.md"
          )}`
        })
      }
    }
  ])

  service.state.cliTargets = [
    {
      id: "claude",
      type: "claude",
      name: "Claude",
      configPath: claudeConfigPath,
      skillsPath: path.join(claudeConfigPath, "skills"),
      sessionsPath: path.join(claudeConfigPath, "projects"),
      sessionPaths: [path.join(claudeConfigPath, "projects")],
      sessionScanRules: {
        extensions: [".jsonl"],
        names: []
      }
    },
    {
      id: "codex",
      type: "codex",
      name: "Codex",
      configPath: codexConfigPath,
      skillsPath: path.join(codexConfigPath, "skills"),
      sessionsPath: path.join(codexConfigPath, "sessions"),
      sessionPaths: [path.join(codexConfigPath, "sessions")],
      sessionScanRules: {
        extensions: [".jsonl"],
        names: []
      }
    }
  ]
  service.state.skills = [
    {
      name: "skill-alpha",
      description: "Alpha",
      sourcePath: path.join(claudeConfigPath, "skills", "skill-alpha")
    },
    {
      name: "skill-beta",
      description: "Beta",
      sourcePath: path.join(codexConfigPath, "skills", "skill-beta")
    },
    {
      name: "skill-gamma",
      description: "Gamma",
      sourcePath: path.join(claudeConfigPath, "skills", "skill-gamma")
    },
    {
      name: "skill-delta",
      description: "Delta",
      sourcePath: path.join(codexConfigPath, "skills", "skill-delta")
    }
  ]
  service.usageService.logs = [
    {
      requestId: "claude-1",
      appType: "claude",
      providerId: "claude-provider",
      providerName: "Claude Provider",
      model: "claude-sonnet-4",
      inputTokens: 10,
      outputTokens: 5,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      totalCostUsd: 0.01,
      rawPath: claudeSessionPath,
      createdAt: 1770000002000
    },
    {
      requestId: "claude-2",
      appType: "claude",
      providerId: "claude-provider-2",
      providerName: "Claude Provider 2",
      model: "claude-opus-4",
      inputTokens: 30,
      outputTokens: 15,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      totalCostUsd: 0.03,
      rawPath: claudeSessionPath,
      createdAt: 1770000004500
    },
    {
      requestId: "codex-1",
      appType: "codex",
      providerId: "codex-provider",
      providerName: "Codex Provider",
      model: "gpt-5.2",
      inputTokens: 20,
      outputTokens: 10,
      cacheReadTokens: 5,
      cacheCreationTokens: 0,
      totalCostUsd: 0.02,
      rawPath: codexSessionPath,
      createdAt: 1770000002500
    }
  ]

  const result = await service.getSkillUsageStats({ cli: "all" })
  const alpha = result.data.skills.find((item) => item.name === "skill-alpha")
  const beta = result.data.skills.find((item) => item.name === "skill-beta")
  const gamma = result.data.skills.find((item) => item.name === "skill-gamma")
  const delta = result.data.skills.find((item) => item.name === "skill-delta")

  assert.equal(alpha.usageCount, 1)
  assert.equal(alpha.requestCount, 1)
  assert.equal(alpha.actualTokens, 15)
  assert.equal(alpha.lastUsedAt, 1770000002000)
  assert.deepEqual(
    alpha.providers.map((item) => item.providerName),
    ["Claude Provider"]
  )
  assert.equal(beta.usageCount, 1)
  assert.equal(beta.requestCount, 1)
  assert.equal(beta.actualTokens, 30)
  assert.equal(gamma.usageCount, 1)
  assert.equal(gamma.requestCount, 1)
  assert.equal(gamma.actualTokens, 45)
  assert.equal(gamma.lastUsedAt, 1770000004000)
  assert.deepEqual(
    gamma.providers.map((item) => item.providerName),
    ["Claude Provider 2"]
  )
  assert.equal(delta.usageCount, 0)
  assert.equal(delta.requestCount, 0)
  assert.deepEqual(result.data.diagnostics, [])

  const filtered = await service.getSkillUsageStats({
    cli: "claude",
    startAt: 1770000002500
  })
  const filteredAlpha = filtered.data.skills.find(
    (item) => item.name === "skill-alpha"
  )
  const filteredBeta = filtered.data.skills.find(
    (item) => item.name === "skill-beta"
  )

  assert.equal(filteredAlpha.usageCount, 0)
  assert.equal(filteredAlpha.requestCount, 0)
  assert.equal(filteredBeta.usageCount, 0)
})

test("已启用 Prompt 修改后会自动应用到对应 CLI", async () => {
  const service = await createService()
  const configPath = path.join(service.paths.userDataPath, ".codex")

  await service.promptRuntimeService.init()
  service.state.cliTargets = [
    {
      id: "codex",
      configPath
    }
  ]

  await service.saveRule({
    cli: "codex",
    name: "Codex Prompt",
    content: "old prompt"
  })

  const prompt = service.promptRuntimeService.prompts.find(
    item => item.name === "Codex Prompt"
  )

  await service.enableRule(prompt.id)
  await service.saveRule({
    id: prompt.id,
    cli: "codex",
    name: "Codex Prompt",
    content: "new prompt"
  })

  assert.equal(await readText(path.join(configPath, "AGENTS.md")), "new prompt")
  assert.equal(
    service.promptRuntimeService.runtimeState.codex.status,
    "SYNCED"
  )
})
