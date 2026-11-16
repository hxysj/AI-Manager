# 服务商配置中心改造与迁移说明

更新日期：2026-11-16。本文描述当前实现，替代先前按旧 Provider 表改造、复用旧账号表或整库回滚的草案。实际建表定义以 [schema.sql](../src-tauri/src/core/service_provider_store/schema.sql) 为准。

## 1. 实现目标和边界

服务商配置成为共享资源。每个服务商维护一套上游地址、凭据、模型目录和价格，各 Agent 通过独立授权、默认模型和别名使用它。上游来源保留 OpenAI 兼容 API 与 Codex 官方账号两类；普通 API 明确区分 Responses 和 Chat Completions。

旧 `providers`、`provider_keys` 表结构、记录和旧功能实现继续保留。新模式隐藏并拦截旧配置入口，停止旧网关服务；回滚后恢复旧入口。此次没有物理删除旧实现，因为它们是回滚所需的执行路径。旧数据不会因为新配置变更而被回写。

新增 15 张 `service_` 表，运行模式、代际和 schema 版本存入既有 `app_metadata`。服务商模式下，配置、账号凭据、路由和请求账本均从新表读取；助手、翻译和图片选择器使用新表投影，兼容原有来源 ID。

## 2. 架构和授权


```mermaid
flowchart LR
  A[Codex / Claude Code / Claude Desktop / 外部工具] --> B[共享本地网关与 Token 鉴权]
  B --> C[Agent 授权与模型路由]
  C --> D[OpenAI Responses / Chat 服务商]
  C --> E[Codex 官方账号适配]
  B --> F[请求与上游尝试账本]
  C --> G[新 service_ 配置表]
  H[旧 Provider 与旧 Key] -.单向复制.-> G
```

共享网关绑定 `127.0.0.1:15724`，避开旧 Codex、Claude 和 Claude Desktop 的默认端口 15721～15723。端口由后端常量统一定义，界面和客户端生成器从后端状态读取。若端口被其他程序占用，启用失败并显示错误，不抢占其他程序。

| Agent 标识 | 使用方式 | 入站协议 |
| --- | --- | --- |
| codex | 接管指定 Codex 配置目录，或应用内文本调用 | Responses |
| claude | 接管 Claude Code 配置目录 | Messages |
| claude-desktop | 使用现有 Desktop 路径解析和配置生成器 | Messages |
| gateway | 为外部工具签发独立调用 Token | Responses、Chat、Messages、Gemini |

授权链为：调用 Token → 实例 → 服务商 → Agent 授权 → 同一服务商内的模型。Token 不能选择其他服务商；关闭 Agent 授权、禁用或删除服务商、撤销 Token、实例未同步、Token 过期均拒绝调用。API Key 与官方 access token 只在出站时使用，不下发给 Agent。

Token 只保存 SHA-256 哈希和可识别前缀；工具 Token 明文只在生成时展示一次。接管文件需要包含本地 Token，因此加密文件备份也包含它。状态列表排除凭据；文件备份对照是用户主动查看时解密的本机接口。

网关接受 Bearer、x-api-key 和 x-goog-api-key 请求头中的本地 Token，分别兼容现有 OpenAI、Messages 和 Gemini 客户端。每种方式都执行相同的实例与授权检查。

模型解析顺序：已启用的 Agent 显式别名 → 同服务商内同名物理模型 → 请求未传模型时的实例模型。显式别名目标禁用时直接拒绝，不能退回同名物理模型。默认模型、别名、实例和账本模型均用复合外键约束所属服务商。

## 3. 配置契约

新增普通服务商时填写名称、API 基础地址、上游协议、API Key 和模型。支持代理、自定义请求头、备注与多个逻辑 Key；保存 API Key 留空表示保留原值。认证头、Host、Content-Length 和连接控制头不能放进自定义请求头。

服务商图标沿用旧 Provider 的 `icon` 配置和现有 `AiIcon` 渲染组件，支持内置图标名称与原有自定义图片地址。首次迁移同步图标；已有新模式数据库自动从 schema 1 升级至 2，仅给 `service_provider_configs` 增加 `icon` 字段，并依据迁移映射补齐缺失图标。升级不改写旧表，不覆盖新模式已保存的非空图标；之后保存时省略 `icon` 会保留现有值，空字符串表示主动清空。

普通上游必须是完整 HTTP/HTTPS 地址，禁止 URL 用户名密码、查询参数、片段及共享网关自身地址。官方账号的上游固定使用项目现有 Codex 官方适配逻辑，其地址字段不用于任意转发。

模型 JSON 可填写名称数组；已有模型按名称重新保存时保留其价格和目录元数据。完整模型示例：

```json
[
  {
    "upstreamModelId": "upstream-model",
    "displayName": "常用模型",
    "enabled": true,
    "contextWindow": 200000,
    "catalog": {},
    "inputPriceMicros": 2000000,
    "cachedInputPriceMicros": 1000000,
    "outputPriceMicros": 8000000,
    "currency": "USD"
  }
]
```

价格是“每百万 Token 的货币微单位”，2000000 表示 2.00 USD / 百万 Token。未知价格填写 null，不能用 0 代替。模型对象保存按提交字段更新；禁用已有模型用 enabled=false，从数组中省略模型不会删除历史模型。

每个服务商分别勾选可用 Agent，并为每个授权设置默认模型。新建时需先保存模型，再编辑选择默认模型或在应用时选择。别名示例：

```json
[
  {
    "agentType": "claude",
    "clientModelName": "sonnet",
    "modelId": "新表中已有模型的ID",
    "enabled": true
  }
]
```

提交 routes 数组会替换该服务商的全部别名，因此编辑时应保留需要的其他别名。Agent 参数仅支持 Codex 的 modelReasoningEffort、serviceTierFast、modelContextWindowEnabled、modelAutoCompactTokenLimit；其他 Agent 当前使用空对象。未知参数或无效值明确拒绝。

保存使用 provider.revision 比较更新，过期页面不能覆盖新修订。配置或模型目录更新后，已接管实例会进入 pending；需要解除接管、检查恢复结果后重新应用。外部工具 Token 不依赖客户端文件，每次请求仍重新校验服务商、Agent 和模型。

## 4. 迁移规则

迁移是可重复执行的单向复制。用户在“设置 → 系统设置”开启“共享服务商模式”时，后端自动执行复制，复制成功后才提交新模式；迁移失败保持旧模式。单独的 migrate 接口仍可用于诊断，复制本身不改客户端文件。

1. 初始化新表，先记录一次迁移运行。
2. 在 IMMEDIATE 事务中直接读取受支持结构的旧表；不调用旧 Provider 初始化或保存接口。
3. 仅复制旧 Codex API 服务商和 Codex 官方账号；其他 Agent 来源记为 skipped。
4. 复制名称、地址、代理、请求头、备注、启用状态、Key 版本、官方账号凭据及额度、模型目录、默认模型和已支持的 Codex 参数。
5. 迁移后默认仅 Codex 授权开启，Claude Code、Claude Desktop 关闭；工具授权需自行开启。
6. 保留旧存储 ID。旧官方账号存储 ID 通常为 account:原ID，新表保留它；前端公开账号 ID 去掉一层前缀，并提供 serviceProviderId。后台同时接受历史公开 ID 和带前缀的存储 ID。
7. 已复制或凭据不可读的来源再次迁移只记 already_copied，不覆盖新模式修改，也不复活已软删除配置。
8. 源 Key 缺失、解密失败或账号凭据不完整记为 unreadable；保留可审查的配置，用户在新入口补录凭据。该状态不会在后续迁移中自动重试覆盖。
9. 单项业务数据导致整次复制失败时回滚复制事务，运行记录保留 failed；旧表始终不变。

旧地址以 /chat/completions 结尾时识别为 Chat，其余迁为 Responses，需在迁移报告后人工核对。旧 Usage 数据原地保留并继续查询，不复制进新网关账本。旧本地 Token、客户端启用态和代理实例不迁入新表。

## 5. 启用、接管与回滚

### 5.1 操作顺序

1. 打开“设置 → 系统设置”，开启“共享服务商模式”。
2. 后端先同步尚未入库的旧会话用量，再启动共享网关并自动复制 Codex API 服务商与官方账号。有在途请求则拒绝切换；迁移失败时保持旧模式，关闭本次启动的新监听。
3. 成功后自动进入服务商列表，沿用原 Provider 列表的紧凑卡片、图标操作与启用状态。顶部按全部、服务商、官方账号筛选；已禁用项通过右上角显示按钮查看。迁移报告位于右上角“网关与实例管理”中，检查 copied、skipped、unreadable 和失败原因；重复启用不覆盖新表修改。
4. 核对上游协议、Key、官方账号、模型与价格；补录不可读凭据，勾选所需 Agent。
5. 在“应用 / 测试 Agent”选择 Agent、模型、配置目录。可先做兼容测试，再应用到客户端。
6. 外部工具使用 gateway 授权，生成独立 Token 并配置后端返回的地址。
7. “用量”始终使用原 Usage 页面，原有筛选、图表、分页、模型费用配置和报表导出继续使用。历史记录与新网关记录统一查询；实例区保留 synced、pending、conflict 状态。

仅启用新模式不会把所有客户端自动指向新网关；必须按实际使用的 Agent 应用。应用内助手与翻译统一经过网关，需要服务商的 Codex 授权及已配置模型。图片工作台继续使用独立图片链路和图片统计。

### 5.2 接管文件

| Agent | 管理内容 |
| --- | --- |
| Codex | config.toml 的模型、目录引用、独立 model_providers.ai_manager_service 节点；另写 .ai_manager_service_models.json |
| Claude Code | settings.json 的模型及相关 env；清除冲突的 ANTHROPIC_API_KEY，写入本地 AUTH_TOKEN、地址和模型 |
| Claude Desktop | 现有路径解析得到的两份 claude_desktop_config.json、configLibrary 下专用 profile 和 _meta.json |

Codex 不写 auth.json；独立 provider 节点使用本地 Bearer、requires_openai_auth=false、supports_websockets=false。保留已有 MCP、权限和其他业务配置。Fast、1M 窗口开关关闭时清除对应接管字段，避免遗留选项继续生效。

写入前把原文件存在性、完整内容和哈希，以及待写内容和哈希持久化至新表；内容复用项目现有凭据加密机制。该机制不是操作系统凭据库。先准备记录，再原子写文件，最后标记实例同步。写入前后检查外部变化和配置修订；失败执行恢复，无法自动恢复的文件保留 conflict。

### 5.3 回滚

在“设置 → 系统设置”关闭“共享服务商模式”。流程先暂停新请求，检查在途请求，再撤销实例 Token、逐文件恢复，全部成功后才切换 backend 和 generation，并恢复旧服务入口。失败时开关继续显示实际的新模式状态。

当前文件等于程序写入后的哈希时，可恢复原内容；原文件不存在时删除接管文件。当前内容已经等于原始内容时直接确认恢复。其他变化均保留 conflict，不覆盖外部修改。用户可“查看加密备份”，对照 before/after 手动处理后重试。

回滚仅恢复接管文件和运行模式，不导入迁移前整库快照；新配置、新账本和迁移记录保留。新模式中的修改不会反向同步到旧服务商，回滚看到的是原有旧配置。

回滚中部分实例可能已恢复，其他实例仍有冲突，此时 backend 保持 service，界面展示实际状态。已恢复实例不会自动重新接管。托盘快捷切换先恢复默认目录，再应用目标服务商；新应用失败时保留恢复后的文件状态并报告错误，不自动补偿切回上一服务商。

后台独立 Provider 进程的旧随机代理端口不会在回滚后重建；这些独立客户端需重新启动。默认旧网关按原启用配置重新启动。有接管实例时禁止切换数据根目录、CLI 配置目录及恢复业务备份，避免备份丢失或路径错配。

应用启动时把中断请求记为 unknown，恢复未完成接管，撤销内部临时 Token，把未完成模式切换记为 interrupted。不能把崩溃中断的上游请求当成已确认成功或零用量。

## 6. 请求、用量和费用

- requests 记录一次入站业务请求；鉴权拒绝也记录，模型列表和 health 查询不进入业务账本。
- attempts 记录一次实际发起的上游尝试；当前没有自动重试或自动故障转移，通常一个业务请求至多一个尝试。
- 转换失败或授权拒绝可以有 request 而没有 attempt，不能算成上游消费。
- 输入、输出、缓存、推理 Token 均单独存储。上游未返回的字段为 NULL；真实 0 与未知区分。
- 缓存输入是输入 Token 的子集，推理 Token 是输出细分，不再次加到总 Token。
- 发送时冻结服务商名称、Key 版本/官方凭据代际、物理模型和价格；后续改名、换 Key、改价不重算历史归属。
- 收到上游响应头即保存状态和 request ID；结算按 finished_at 幂等，只更新一次。
- 客户端在响应头到达前取消或处理任务中断时，请求上下文退出会关闭遗留在途记录，状态记为 unknown，未观测用量保持 NULL，避免阻塞解除接管和模式切换。流式上下文持续到转发任务结束。
- 统计支持服务商、Agent、模型、Key/凭据版本、本地 Token、本地日期六个维度。分组按币种分别汇总，已知 Token 和已计价次数同时展示。
- 汇总表的费用是“已计价部分之和”，应结合 pricedCount / attemptCount 判断覆盖率；不能当作完整账单。缺字段的单次费用保留 NULL。
- 悬浮窗和 Usage 页面使用同一查询接口，按原有应用、日期范围汇总新旧记录，切换模式不清零当日统计。

Usage 兼容查询只读取并投影新账本，不删除、搬移或重新归属原 Usage 记录。查询不依赖当前 backend，因此回滚后仍可查看新模式期间的调用。客户端会话日志同步在新模式继续工作，旧 Key 绑定写入在新模式跳过。

为避免重复计数，Claude Messages 转换响应使用由网关请求 ID 派生的 `msg_aim_...` 标识，与 Claude 会话按响应 ID 对应；Desktop 回合全部由这些响应组成时保留会话行并标记来源，由网关账本参与统计。Codex token_count 没有响应 ID，按已接管配置目录与对应接管/恢复时段选用网关账本；未接管目录与接管前、恢复后的会话继续参与原统计。接管期间应通过网关使用该目录，手动改回直连前应先解除接管。Windows 普通路径与 `\\?\` 路径统一比较。

新日志的未知 Token、费用保持 NULL，原表格显示“未知”。汇总保留已知小计，全部未知时显示未知，并通过现有卡片提示说明缺失请求数。CNY 估算按 Usage 的汇率转换为 USD 后参与原费用切换。未配置服务商单价时继续使用 Usage 原有模型费用；修改模型费用前冻结历史网关估算，只更新新账本中的估算和费用来源快照。

费用按以下规则计算，结果是整数货币微单位并向上取整：

```text
非缓存输入 = input_tokens - cached_input_tokens
估算费用微单位 = ceil((非缓存输入 × input_price_micros
                   + 缓存输入 × cached_input_price_micros
                   + output_tokens × output_price_micros) / 1000000)
```

需要完整输入、输出、缓存用量及对应价格；缓存为 0 时不要求缓存单价。账本是网关观测与估算，官方额度、供应商结算账单及图片统计各自保留，不冒充上游账单。配置中心的网关诊断显示最近 100 条请求明细，原 Usage 页面按原分页与导出流程查询全部兼容记录；没有新增自动清理逻辑。

## 7. 业务备份与凭据

业务备份新增服务商配置、模型、Agent 授权和别名四张表；不包含新 Key、官方 token、本地调用 Token、实例、接管文件备份、迁移记录及请求账本。导出的新服务商和授权均关闭，恢复后补录凭据并明确启用。

恢复按父子表顺序进行，以 provider+upstream_model、provider+agent、provider+agent+alias 的自然键合并，保留本地主键并重映射模型外键。已有配置保留本机启用及软删除状态，revision 递增，不允许改变来源类型。模式、generation 等本机 app_metadata 不随业务备份恢复。

业务备份仍保留兼容旧业务表的范围；新模式恢复过滤旧 Provider 和旧凭据旁路，不会把旧来源写回新运行链路。需要灾难恢复凭据和完整审计数据时，应在应用关闭、SQLite WAL 一致的条件下备份整个配置数据根目录；这与页面业务备份和“回滚旧模式”是不同操作。

## 8. 接口和代码位置

统一沿用项目现有 IPC 请求层，新增 service-provider 命名空间：state、detail、save、delete、migrate、switch-backend、activate、detach、quick-switch、fetch-models、test、issue-token、key-select、key-revoke、import-account、login、cancel-login、refresh-quota、file-backup、period-usage。

| 位置 | 职责 |
| --- | --- |
| src-tauri/src/core/service_provider_store.rs | 新存储、校验、Key/账号加密、目录投影和统计 |
| src-tauri/src/core/service_provider_store/schema.sql | 15 张新表的唯一建表定义 |
| src-tauri/src/core/service_provider_store/migration.rs | 旧表只读、单向幂等复制 |
| src-tauri/src/api/service_provider.rs | 配置操作、接管、恢复和模式切换 |
| src-tauri/src/api/multi_protocol_gateway/service.rs | 鉴权、路由、attempt、结算、模型获取和兼容测试 |
| src-tauri/src/core/state.rs | 生命周期、旧入口隔离与状态事件 |
| src-tauri/src/core/database.rs 和 database/restore.rs | 业务备份范围与外键映射 |
| src/features/providers/components/ServiceProviderCenter.vue | 新配置中心、迁移报告与网关诊断 |
| src/features/settings/components/SystemSettings.vue | 共享服务商模式开关 |
| src-tauri/src/api/usage/service.rs | 新账本到原 Usage 的只读投影、去重与费用兼容 |
| src/features/usage/index.vue | 继续使用原 Usage 页面、筛选和导出 |
| src/api/modules/service-providers.js | 复用原请求层的 API 包装 |

## 9. 当前协议能力和验证范围

复用既有协议转换与 SSE 内核，提供 Responses / Chat / Messages / Gemini 入站到 Responses / Chat 上游的转换。原生 Responses 请求保留原始字段；跨协议无法表达的语义明确拒绝。仅 HTTP，未提供 WebSocket。

/responses/compact 和 /v1/responses/compact 只允许 Responses 上游原生透传，Chat 上游返回 422，stream=true 返回 400。真实 Agent 若使用其他专用能力，需在对应客户端版本继续验收；“OpenAI 兼容”不能保证所有扩展字段等价。

配置中心的兼容测试验证“工具调用 → 工具结果 → SSE 文本”，记录服务商修订、Agent 版本、适配器版本与能力。它会发起真实测试请求并进入账本，可能消耗上游额度；不启动真实 Codex、Claude Code 或 Desktop 进程。

自动验证使用临时数据库和本机模拟上游，覆盖旧表写保护、幂等迁移、凭据版本、官方账号 ID、授权隔离、协议矩阵、compact、未知用量、统计币种、文件冲突、回滚、备份和前端交互。没有操作真实用户数据库、执行真实 OAuth 登录或调用真实计费上游；不宣称完成真实客户端联调或安装包验证。

额外回归覆盖：没有旧 Provider 表时的主状态生成和刷新、模型获取期间修订变化、刷新目录保留本地价格与启用状态、客户端等待响应头时取消后可以回滚。同一官方身份轮换令牌保留额度缓存，更换官方身份则清空旧额度；额度查询结果按发起查询时的凭据代际写入，不能覆盖重新导入后的账号。

复验命令：

```powershell
cd src-tauri
cargo test --lib service_ --offline -- --test-threads=1
cargo test --lib api::proxy::tests --offline -- --test-threads=1
cargo test --lib api::multi_protocol_gateway::tests --offline -- --test-threads=1
cargo test --lib core::database::tests --offline -- --test-threads=1
cargo test --lib api::data::tests --offline -- --test-threads=1
cargo test --lib api::claude_desktop::tests --offline -- --test-threads=1
cargo test --lib api::codex_account::tests --offline -- --test-threads=1
cargo test --lib api::image_workbench::tests --offline -- --test-threads=1
cd ..
node scripts/test-service-providers.mjs
```

## 10. 完整新表字段字典

以下逐字段列出实际 SQL 类型、空值规则、默认值、字段约束与含义，并列出表级外键和唯一索引。所有表为 SQLite STRICT；布尔值使用整数 0/1，所有时间戳使用 UTC Unix 毫秒。没有 DEFAULT 的必填字段须由写入方提供；可空且无 DEFAULT 的字段默认 NULL。SQL 枚举以本节列出的约束为准，Agent 类型等业务白名单还受写入 API 校验。

### 10.1 service_provider_configs

用途：服务商主配置。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `name` | TEXT | 否 | 无 | 界面显示名称 | `CHECK(length(trim(name))>0)` |
| `icon` | TEXT | 否 | `''` | 沿用旧 Provider 的图标名称或自定义图片地址；空串使用默认展示 | — |
| `kind` | TEXT | 否 | 无 | 来源类型：api 或 codex_account | `CHECK(kind IN ('api','codex_account'))` |
| `upstream_protocol` | TEXT | 否 | 无 | 上游协议；普通 API 可为 responses/chat，官方账号固定 responses | `CHECK(upstream_protocol IN ('responses','chat'))` |
| `base_url` | TEXT | 否 | `''` | 普通 API 的基础地址；官方账号不使用任意自定义上游 | — |
| `proxy` | TEXT | 否 | `''` | 出站代理地址；空串按项目默认网络行为 | — |
| `headers_json` | TEXT | 否 | `'{}'` | 额外出站请求头 JSON 对象；认证和连接控制头由程序管理 | `CHECK(json_valid(headers_json) AND json_type(headers_json)='object')` |
| `note` | TEXT | 否 | `''` | 用户备注 | — |
| `enabled` | INTEGER | 否 | `1` | 启用状态；还需通过服务商、Agent、实例和凭据的其他校验 | `CHECK(enabled IN (0,1))` |
| `revision` | INTEGER | 否 | `1` | 乐观并发修订号，更新递增 | `CHECK(revision>0)` |
| `deleted_at` | INTEGER | 是 | 无 | 软删除时间；NULL 表示未删除 | — |
| `created_at` | INTEGER | 否 | 无 | 创建时间，Unix 毫秒 | — |
| `updated_at` | INTEGER | 否 | 无 | 最后更新时间，Unix 毫秒 | — |

表级约束与索引：

```sql
CHECK(kind<>'codex_account' OR upstream_protocol='responses')
```

### 10.2 service_provider_keys

用途：普通 API Key 的逻辑槽位及版本。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `provider_id` | TEXT | 否 | 无 | 所属服务商主配置 ID | `REFERENCES service_provider_configs(id) ON DELETE RESTRICT` |
| `logical_id` | TEXT | 否 | 无 | 同一服务商内逻辑 Key 槽位标识，轮换凭据时保持不变 | — |
| `name` | TEXT | 否 | `''` | 界面显示名称 | — |
| `note` | TEXT | 否 | `''` | 用户备注 | — |
| `secret_ciphertext` | TEXT | 否 | `''` | 当前 Key 的加密值；旧版本或撤销后清空 | — |
| `encryption_version` | INTEGER | 否 | `1` | 凭据包装版本标识，当前仅接受 1；不表示操作系统凭据保护 | `CHECK(encryption_version=1)` |
| `fingerprint` | TEXT | 否 | 无 | Key 原文 SHA-256；用于版本识别，不是可调用凭据 | — |
| `is_current` | INTEGER | 否 | `1` | 是否为该逻辑 Key 的当前版本 | `CHECK(is_current IN (0,1))` |
| `is_active` | INTEGER | 否 | `0` | 是否为此服务商实际出站使用的 Key | `CHECK(is_active IN (0,1))` |
| `status` | TEXT | 否 | `'active'` | 本表生命周期状态，合法值由 CHECK 列出 | `CHECK(status IN ('active','archived','unreadable','revoked'))` |
| `created_at` | INTEGER | 否 | 无 | 创建时间，Unix 毫秒 | — |
| `updated_at` | INTEGER | 否 | 无 | 最后更新时间，Unix 毫秒 | — |

表级约束与索引：

```sql
UNIQUE(provider_id,logical_id,fingerprint)
UNIQUE(provider_id,id)
CHECK(is_current=1 OR (is_active=0 AND secret_ciphertext=''))
CREATE UNIQUE INDEX IF NOT EXISTS service_keys_current ON service_provider_keys(provider_id,logical_id) WHERE is_current=1;
CREATE UNIQUE INDEX IF NOT EXISTS service_keys_active ON service_provider_keys(provider_id) WHERE is_active=1;
```

### 10.3 service_provider_accounts

用途：独立的 Codex 官方账号凭据。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `provider_id` | TEXT | 否 | 无 | 所属服务商主配置 ID | `UNIQUE REFERENCES service_provider_configs(id) ON DELETE RESTRICT` |
| `external_account_id` | TEXT | 否 | 无 | 上游官方账号标识，独立于本地服务商 ID | — |
| `email` | TEXT | 否 | `''` | 账号展示邮箱 | — |
| `access_ciphertext` | TEXT | 否 | `''` | 加密 access token | — |
| `refresh_ciphertext` | TEXT | 否 | `''` | 加密 refresh token | — |
| `id_ciphertext` | TEXT | 否 | `''` | 加密 id token | — |
| `expires_at` | INTEGER | 是 | 无 | 到期时间，Unix 毫秒；未知或不设有效期时为 NULL | — |
| `token_generation` | INTEGER | 否 | `0` | 官方凭据代际；每次替换凭据递增，冻结到 attempt 归属 | — |
| `refresh_status` | TEXT | 否 | `''` | 账号刷新状态预留字段；当前成功保存清空，未实现完整失败状态机 | — |
| `quota_json` | TEXT | 否 | `'{}'` | 最近官方额度响应 JSON，与本地 Token 消费统计分开 | `CHECK(json_valid(quota_json))` |
| `revision` | INTEGER | 否 | `1` | 乐观并发修订号，更新递增 | — |
| `created_at` | INTEGER | 否 | 无 | 创建时间，Unix 毫秒 | — |
| `updated_at` | INTEGER | 否 | 无 | 最后更新时间，Unix 毫秒 | — |

### 10.4 service_provider_models

用途：服务商模型目录与价格。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `provider_id` | TEXT | 否 | 无 | 所属服务商主配置 ID | `REFERENCES service_provider_configs(id) ON DELETE RESTRICT` |
| `upstream_model_id` | TEXT | 否 | 无 | 真实上游模型名，服务商内唯一 | `CHECK(length(trim(upstream_model_id))>0)` |
| `display_name` | TEXT | 否 | `''` | 模型展示名称 | — |
| `context_window` | INTEGER | 是 | 无 | 上下文窗口 Token 数；未知为 NULL | `CHECK(context_window>0)` |
| `catalog_json` | TEXT | 否 | `'{}'` | 保留上游模型目录扩展信息的 JSON 对象 | `CHECK(json_valid(catalog_json) AND json_type(catalog_json)='object')` |
| `catalog_schema_version` | INTEGER | 否 | `1` | 目录包装版本，当前写入默认 1 | — |
| `enabled` | INTEGER | 否 | `1` | 启用状态；还需通过服务商、Agent、实例和凭据的其他校验 | `CHECK(enabled IN (0,1))` |
| `input_price_micros` | INTEGER | 是 | 无 | 非缓存输入每百万 Token 的货币微单位单价；未知为 NULL | `CHECK(input_price_micros>=0)` |
| `cached_input_price_micros` | INTEGER | 是 | 无 | 缓存输入每百万 Token 的货币微单位单价；未知为 NULL | `CHECK(cached_input_price_micros>=0)` |
| `output_price_micros` | INTEGER | 是 | 无 | 输出每百万 Token 的货币微单位单价；未知为 NULL | `CHECK(output_price_micros>=0)` |
| `currency` | TEXT | 否 | `'USD'` | 单价和费用币种；默认 USD，不做隐式跨币种相加 | — |
| `created_at` | INTEGER | 否 | 无 | 创建时间，Unix 毫秒 | — |
| `updated_at` | INTEGER | 否 | 无 | 最后更新时间，Unix 毫秒 | — |

表级约束与索引：

```sql
UNIQUE(provider_id,upstream_model_id)
UNIQUE(provider_id,id)
```

### 10.5 service_provider_agent_bindings

用途：服务商对各 Agent 的授权和默认配置。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | `(lower(hex(randomblob(16))))` | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY  DEFAULT (lower(hex(randomblob(16))))` |
| `provider_id` | TEXT | 否 | 无 | 所属服务商主配置 ID | `REFERENCES service_provider_configs(id) ON DELETE RESTRICT` |
| `agent_type` | TEXT | 否 | 无 | Agent 类型，API 当前接受 codex/claude/claude-desktop/gateway | `CHECK(length(agent_type)>0)` |
| `enabled` | INTEGER | 否 | `0` | 启用状态；还需通过服务商、Agent、实例和凭据的其他校验 | `CHECK(enabled IN (0,1))` |
| `default_model_id` | TEXT | 是 | 无 | 该服务商对当前 Agent 的默认模型 ID；可空 | — |
| `options_json` | TEXT | 否 | `'{}'` | Agent 专有选项对象；当前仅 Codex 支持已声明参数 | `CHECK(json_valid(options_json) AND json_type(options_json)='object')` |
| `revision` | INTEGER | 否 | `1` | 乐观并发修订号，更新递增 | — |
| `updated_at` | INTEGER | 否 | 无 | 最后更新时间，Unix 毫秒 | — |

表级约束与索引：

```sql
UNIQUE(provider_id,agent_type)
FOREIGN KEY(provider_id,default_model_id) REFERENCES service_provider_models(provider_id,id) ON DELETE RESTRICT
```

### 10.6 service_agent_model_routes

用途：各 Agent 的客户端模型别名。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `provider_id` | TEXT | 否 | 无 | 所属服务商主配置 ID | — |
| `agent_type` | TEXT | 否 | 无 | Agent 类型，API 当前接受 codex/claude/claude-desktop/gateway | — |
| `client_model_name` | TEXT | 否 | 无 | 客户端请求的模型别名 | `CHECK(length(client_model_name)>0)` |
| `model_id` | TEXT | 否 | 无 | 所属服务商下的模型 ID；使用复合外键防止跨服务商引用 | — |
| `enabled` | INTEGER | 否 | `1` | 启用状态；还需通过服务商、Agent、实例和凭据的其他校验 | `CHECK(enabled IN (0,1))` |

表级约束与索引：

```sql
FOREIGN KEY(provider_id,agent_type) REFERENCES service_provider_agent_bindings(provider_id,agent_type) ON DELETE RESTRICT
FOREIGN KEY(provider_id,model_id) REFERENCES service_provider_models(provider_id,id) ON DELETE RESTRICT
UNIQUE(provider_id,agent_type,client_model_name)
```

### 10.7 service_compatibility_tests

用途：修订版本对应的兼容性测试证据。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `provider_id` | TEXT | 否 | 无 | 所属服务商主配置 ID | — |
| `agent_type` | TEXT | 否 | 无 | Agent 类型，API 当前接受 codex/claude/claude-desktop/gateway | — |
| `model_id` | TEXT | 否 | 无 | 所属服务商下的模型 ID；使用复合外键防止跨服务商引用 | — |
| `provider_revision` | INTEGER | 否 | 无 | 测试执行时记录的服务商修订 | — |
| `agent_version` | TEXT | 否 | 无 | 客户端版本字符串；未检测或未提供时为空 | — |
| `adapter_version` | INTEGER | 否 | `1` | 协议适配器版本，当前默认 1 | — |
| `client_protocol` | TEXT | 否 | 无 | 实例或测试的入站协议；all 仅用于多协议工具实例 | — |
| `upstream_protocol` | TEXT | 否 | 无 | 上游协议；普通 API 可为 responses/chat，官方账号固定 responses | — |
| `capability` | TEXT | 否 | 无 | 被测试的能力名，当前工具往返及流式文本为 tool_roundtrip_stream | — |
| `status` | TEXT | 否 | 无 | 本表生命周期状态，合法值由 CHECK 列出 | `CHECK(status IN ('passed','failed','unsupported'))` |
| `evidence_json` | TEXT | 否 | 无 | 测试结果与实例 ID 等证据 JSON；不存完整对话正文 | `CHECK(json_valid(evidence_json))` |
| `tested_at` | INTEGER | 否 | 无 | 测试时间，Unix 毫秒 | — |

表级约束与索引：

```sql
FOREIGN KEY(provider_id,agent_type) REFERENCES service_provider_agent_bindings(provider_id,agent_type) ON DELETE RESTRICT
FOREIGN KEY(provider_id,model_id) REFERENCES service_provider_models(provider_id,id) ON DELETE RESTRICT
```

### 10.8 service_agent_instances

用途：客户端目录或内部/外部调用实例。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `agent_type` | TEXT | 否 | 无 | Agent 类型，API 当前接受 codex/claude/claude-desktop/gateway | `CHECK(length(agent_type)>0)` |
| `name` | TEXT | 否 | 无 | 界面显示名称 | — |
| `config_path` | TEXT | 否 | 无 | 规范化配置目录；内部/测试/外部实例使用 internal:/test:/external: 前缀标识 | — |
| `agent_version` | TEXT | 否 | `''` | 客户端版本字符串；未检测或未提供时为空 | — |
| `client_protocol` | TEXT | 否 | 无 | 实例或测试的入站协议；all 仅用于多协议工具实例 | `CHECK(client_protocol IN ('responses','messages','chat','all'))` |
| `provider_id` | TEXT | 是 | 无 | 所属服务商主配置 ID | — |
| `model_id` | TEXT | 是 | 无 | 所属服务商下的模型 ID；使用复合外键防止跨服务商引用 | — |
| `status` | TEXT | 否 | `'detached'` | 本表生命周期状态，合法值由 CHECK 列出 | `CHECK(status IN ('detached','active','error'))` |
| `sync_status` | TEXT | 否 | `'pending'` | 实例同步状态：pending/synced/conflict/restored | `CHECK(sync_status IN ('pending','synced','conflict','restored'))` |
| `applied_revision` | INTEGER | 是 | 无 | 接管成功时的服务商修订；尚未应用为 NULL | — |
| `applied_hash` | TEXT | 否 | `''` | 接管文件路径与 after 哈希组合后得到的摘要 | — |
| `last_synced_at` | INTEGER | 是 | 无 | 最后成功同步时间，Unix 毫秒 | — |
| `last_error` | TEXT | 否 | `''` | 实例最近一次错误说明 | — |
| `revision` | INTEGER | 否 | `1` | 乐观并发修订号，更新递增 | — |
| `created_at` | INTEGER | 否 | 无 | 创建时间，Unix 毫秒 | — |
| `updated_at` | INTEGER | 否 | 无 | 最后更新时间，Unix 毫秒 | — |

表级约束与索引：

```sql
FOREIGN KEY(provider_id,model_id) REFERENCES service_provider_models(provider_id,id) ON DELETE RESTRICT
FOREIGN KEY(provider_id,agent_type) REFERENCES service_provider_agent_bindings(provider_id,agent_type) ON DELETE RESTRICT
CHECK(status<>'active' OR (provider_id IS NOT NULL AND model_id IS NOT NULL))
CHECK((provider_id IS NULL)=(model_id IS NULL))
UNIQUE(agent_type,config_path)
```

### 10.9 service_gateway_tokens

用途：绑定实例的本地调用凭据哈希。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `token_hash` | TEXT | 否 | 无 | 本地调用 Token 的 SHA-256，不保存 Token 明文 | `UNIQUE` |
| `token_prefix` | TEXT | 否 | 无 | 仅用于展示识别的 Token 前缀 | — |
| `instance_id` | TEXT | 否 | 无 | 对应 Agent 实例；请求账本允许匿名拒绝时为空 | `REFERENCES service_agent_instances(id) ON DELETE RESTRICT` |
| `status` | TEXT | 否 | `'active'` | 本表生命周期状态，合法值由 CHECK 列出 | `CHECK(status IN ('active','revoked','expired'))` |
| `expires_at` | INTEGER | 是 | 无 | 到期时间，Unix 毫秒；未知或不设有效期时为 NULL | — |
| `last_used_at` | INTEGER | 是 | 无 | 最近一次实际路由发送上游时的使用时间 | — |
| `created_at` | INTEGER | 否 | 无 | 创建时间，Unix 毫秒 | — |
| `revoked_at` | INTEGER | 是 | 无 | 撤销时间；尚未撤销为 NULL | — |

表级约束与索引：

```sql
CREATE UNIQUE INDEX IF NOT EXISTS service_token_current ON service_gateway_tokens(instance_id) WHERE status='active';
```

### 10.10 service_switch_operations

用途：新旧后端切换审计。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `from_backend` | TEXT | 否 | 无 | 切换前模式：legacy/service | `CHECK(from_backend IN ('legacy','service'))` |
| `to_backend` | TEXT | 否 | 无 | 目标模式：legacy/service | `CHECK(to_backend IN ('legacy','service'))` |
| `generation` | INTEGER | 否 | 无 | 运行模式代际，防止旧请求跨切换继续派发 | — |
| `stage` | TEXT | 否 | 无 | 切换阶段：preparing/completed/failed/interrupted；由应用维护 | — |
| `error_message` | TEXT | 否 | `''` | 业务错误说明；不保存认证值或请求正文 | — |
| `created_at` | INTEGER | 否 | 无 | 创建时间，Unix 毫秒 | — |
| `completed_at` | INTEGER | 是 | 无 | 迁移或切换完成时间；尚未结束为 NULL | — |

### 10.11 service_managed_files

用途：接管文件的加密快照和恢复状态。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `operation_id` | TEXT | 否 | 无 | 同一批客户端文件接管操作 ID；非模式切换表外键 | — |
| `instance_id` | TEXT | 否 | 无 | 对应 Agent 实例；请求账本允许匿名拒绝时为空 | `REFERENCES service_agent_instances(id) ON DELETE RESTRICT` |
| `file_path` | TEXT | 否 | 无 | 规范化绝对文件路径 | — |
| `before_exists` | INTEGER | 否 | 无 | 接管前文件是否存在；不存在时恢复应删除本次生成文件 | `CHECK(before_exists IN (0,1))` |
| `before_sha256` | TEXT | 否 | 无 | 接管前内容 SHA-256；原文件不存在时为规范化空内容摘要 | — |
| `before_ciphertext` | TEXT | 否 | 无 | 接管前完整文件内容的加密值 | — |
| `after_sha256` | TEXT | 否 | 无 | 本程序生成的完整内容 SHA-256，用于检测外部修改 | — |
| `after_ciphertext` | TEXT | 否 | 无 | 本程序生成的完整文件内容的加密值 | — |
| `managed_fields_json` | TEXT | 否 | 无 | 接管范围描述 JSON；当前采用整文件快照与哈希冲突检查 | `CHECK(json_valid(managed_fields_json))` |
| `write_status` | TEXT | 否 | 无 | 写入阶段：prepared/written/failed | `CHECK(write_status IN ('prepared','written','failed'))` |
| `restore_status` | TEXT | 否 | `'pending'` | 恢复阶段：pending/restored/conflict | `CHECK(restore_status IN ('pending','restored','conflict'))` |
| `error_message` | TEXT | 否 | `''` | 业务错误说明；不保存认证值或请求正文 | — |
| `created_at` | INTEGER | 否 | 无 | 创建时间，Unix 毫秒 | — |
| `updated_at` | INTEGER | 否 | 无 | 最后更新时间，Unix 毫秒 | — |

表级约束与索引：

```sql
CREATE UNIQUE INDEX IF NOT EXISTS service_managed_file_owner ON service_managed_files(file_path) WHERE restore_status<>'restored';
```

### 10.12 service_gateway_requests

用途：入站业务请求账本。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `token_id` | TEXT | 是 | 无 | 已认证的本地 Token ID 快照；拒绝认证时可为 NULL | — |
| `instance_id` | TEXT | 是 | 无 | 对应 Agent 实例；请求账本允许匿名拒绝时为空 | — |
| `agent_type` | TEXT | 否 | `''` | Agent 类型，API 当前接受 codex/claude/claude-desktop/gateway | — |
| `generation` | INTEGER | 否 | 无 | 运行模式代际，防止旧请求跨切换继续派发 | — |
| `source` | TEXT | 否 | `'service-gateway'` | 请求来源，当前为 service-gateway | — |
| `client_request_id` | TEXT | 否 | `''` | 客户端 x-request-id；缺省为空 | — |
| `requested_model` | TEXT | 否 | `''` | 客户端原始请求模型名 | — |
| `protocol_in` | TEXT | 否 | 无 | 识别的入站协议；未知路径可标 unsupported | — |
| `request_path` | TEXT | 否 | 无 | 入站路径，不保存请求正文 | — |
| `state` | TEXT | 否 | 无 | 请求或上游尝试的状态，分别由本表 CHECK 限定 | `CHECK(state IN ('received','rejected','routed','completed','failed','cancelled','unknown'))` |
| `status_code` | INTEGER | 是 | 无 | 已收到的 HTTP 状态码；未确认响应为 NULL | — |
| `error_message` | TEXT | 否 | `''` | 业务错误说明；不保存认证值或请求正文 | — |
| `created_at` | INTEGER | 否 | 无 | 创建时间，Unix 毫秒 | — |
| `finished_at` | INTEGER | 是 | 无 | 请求或尝试结束时间；在途为 NULL | — |

表级约束与索引：

```sql
CREATE INDEX IF NOT EXISTS service_requests_created ON service_gateway_requests(created_at);
```

### 10.13 service_gateway_attempts

用途：实际上游尝试与已观测用量。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `request_id` | TEXT | 否 | 无 | 对应入站请求 ID | `REFERENCES service_gateway_requests(id) ON DELETE RESTRICT` |
| `attempt_no` | INTEGER | 否 | 无 | 同一请求内的尝试序号；当前单次发送为 1 | `CHECK(attempt_no>0)` |
| `provider_id` | TEXT | 否 | 无 | 所属服务商主配置 ID | `REFERENCES service_provider_configs(id) ON DELETE RESTRICT` |
| `provider_name` | TEXT | 否 | 无 | 派发时服务商名称快照 | — |
| `credential_id` | TEXT | 否 | 无 | 派发时 API Key 版本 ID，或官方账号记录ID:token_generation | — |
| `credential_name` | TEXT | 否 | `''` | 派发时 Key 名称或官方账号邮箱快照 | — |
| `credential_fingerprint` | TEXT | 否 | `''` | 派发时 API Key 指纹，官方账号不存 access token 指纹 | — |
| `model_id` | TEXT | 否 | 无 | 所属服务商下的模型 ID；使用复合外键防止跨服务商引用 | — |
| `upstream_model` | TEXT | 否 | 无 | 派发的真实物理模型名快照 | — |
| `protocol_out` | TEXT | 否 | 无 | 实际选定的上游协议 | — |
| `state` | TEXT | 否 | 无 | 请求或上游尝试的状态，分别由本表 CHECK 限定 | `CHECK(state IN ('created','started','streaming','succeeded','failed','cancelled','unknown'))` |
| `status_code` | INTEGER | 是 | 无 | 已收到的 HTTP 状态码；未确认响应为 NULL | — |
| `upstream_request_id` | TEXT | 否 | `''` | 上游响应头 request ID，缺省为空 | — |
| `upstream_response_id` | TEXT | 否 | `''` | 上游协议返回的 response ID，缺省为空 | — |
| `actual_model` | TEXT | 否 | `''` | 上游响应声明的实际模型；未知为空 | — |
| `service_tier` | TEXT | 否 | `''` | 上游响应声明的服务等级；未知为空 | — |
| `usage_source` | TEXT | 否 | `'unknown'` | unknown 或 upstream；部分字段已收到也可为 upstream，完整性还需检查具体列 | `CHECK(usage_source IN ('unknown','upstream'))` |
| `input_tokens` | INTEGER | 是 | 无 | 上游报告的输入 Token，包含缓存输入；未报告为 NULL | `CHECK(input_tokens>=0)` |
| `output_tokens` | INTEGER | 是 | 无 | 上游报告的输出 Token；未报告为 NULL | `CHECK(output_tokens>=0)` |
| `cached_input_tokens` | INTEGER | 是 | 无 | 输入 Token 中的缓存部分；未报告为 NULL | `CHECK(cached_input_tokens>=0)` |
| `reasoning_tokens` | INTEGER | 是 | 无 | 输出中的推理 Token 细分；不重复加入总量，未报告为 NULL | `CHECK(reasoning_tokens>=0)` |
| `price_snapshot_json` | TEXT | 否 | `'{}'` | 派发时模型价格快照 JSON，用于历史估算 | `CHECK(json_valid(price_snapshot_json))` |
| `estimated_cost_micros` | INTEGER | 是 | 无 | 本次尝试估算费用的货币微单位；字段或价格不足时 NULL | `CHECK(estimated_cost_micros>=0)` |
| `currency` | TEXT | 否 | `'USD'` | 单价和费用币种；默认 USD，不做隐式跨币种相加 | — |
| `error_message` | TEXT | 否 | `''` | 业务错误说明；不保存认证值或请求正文 | — |
| `started_at` | INTEGER | 否 | 无 | 尝试派发时间，Unix 毫秒 | — |
| `finished_at` | INTEGER | 是 | 无 | 请求或尝试结束时间；在途为 NULL | — |

表级约束与索引：

```sql
UNIQUE(request_id,attempt_no)
FOREIGN KEY(provider_id,model_id) REFERENCES service_provider_models(provider_id,id) ON DELETE RESTRICT
```

### 10.14 service_provider_migrations

用途：每次旧配置复制的运行记录。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `source_schema` | TEXT | 否 | 无 | 迁移识别的旧 schema 类型，当前为 providers-v1 | — |
| `status` | TEXT | 否 | 无 | 本表生命周期状态，合法值由 CHECK 列出 | `CHECK(status IN ('running','completed','failed'))` |
| `created_at` | INTEGER | 否 | 无 | 创建时间，Unix 毫秒 | — |
| `completed_at` | INTEGER | 是 | 无 | 迁移或切换完成时间；尚未结束为 NULL | — |
| `error_message` | TEXT | 否 | `''` | 业务错误说明；不保存认证值或请求正文 | — |

### 10.15 service_provider_migration_items

用途：各旧来源的迁移结果与幂等标记。

| 字段 | 类型 | 可空 | 默认值 | 含义 | SQL 字段约束 |
| --- | --- | --- | --- | --- | --- |
| `id` | TEXT | 否 | 无 | 本表稳定主键；新建由应用生成，迁移服务商保留旧存储 ID | `PRIMARY KEY` |
| `migration_id` | TEXT | 否 | 无 | 本次迁移运行 ID | `REFERENCES service_provider_migrations(id) ON DELETE RESTRICT` |
| `source_table` | TEXT | 否 | 无 | 旧来源表名，当前为 providers | — |
| `source_id` | TEXT | 否 | 无 | 旧来源存储主键 | — |
| `source_fingerprint` | TEXT | 否 | 无 | 旧来源行 JSON 的 SHA-256，用于审计来源，不用于覆盖新配置 | — |
| `target_id` | TEXT | 是 | 无 | 新表服务商 ID；skipped 时为 NULL | — |
| `status` | TEXT | 否 | 无 | 本表生命周期状态，合法值由 CHECK 列出 | `CHECK(status IN ('copied','skipped','unreadable','already_copied'))` |
| `reason` | TEXT | 否 | `''` | 逐项迁移结果的中文说明 | — |

表级约束与索引：

```sql
UNIQUE(migration_id,source_table,source_id)
CREATE UNIQUE INDEX IF NOT EXISTS service_migration_source ON service_provider_migration_items(source_table,source_id) WHERE status IN ('copied','unreadable');
```
