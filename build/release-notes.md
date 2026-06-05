## 变更说明

- Codex 独立实例用量区分。
  新增 Codex Provider 独立实例启动和实例目录记录，启动后可使用独立 `CODEX_HOME`；代理请求日志会记录请求来源和实例 Provider 信息，Usage 统计、筛选、表格、CSV 和长图导出也能按请求来源区分用量。
- 工作区数据目录分层。
  将用量日志、用量请求记录、应用调用日志、会话索引缓存和 Skill 仓库扫描缓存从 `storage` 分离到 `logs` 或 `temp`，避免运行时数据和可重新生成缓存进入云备份。
- 旧版本数据兼容迁移。
  启动和备份恢复后会把旧 `storage` 中的日志、会话缓存和 Skill 仓库扫描结果迁移到新目录，并保留 Skill 仓库地址等真正需要云备份的配置数据。
- 项目协作规则补充。
  在 `AGENTS.md` 中明确 `storage`、`logs`、`temp`、`skills`、`prompts`、`profiles`、`repos`、`sessions` 和 `git-tool` 的目录职责，后续新增数据文件需要先按数据性质放入正确目录。
