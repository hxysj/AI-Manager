## 变更说明

- 增强 Prompt 规则启用能力。
  Rule 模块新增通用 Prompt tab，可把通用 Prompt 直接挂载到 Claude 或 Codex；挂载通用 Prompt 时会取消对应 CLI tab 下已启用的 Prompt，并在启动和刷新时检查全局 Prompt 与当前启用配置是否存在差异。
- 优化 Prompt 规则页面交互。
  通用 Prompt 列表和抽屉增加 Claude、Codex 目标按钮，支持直接启用、取消启用和查看差异状态，不再依赖下拉框切换目标 CLI。
- 修复坚果云备份路径问题。
  WebDAV 文件地址组装会移除目录末尾空路径段，避免推送或下载时生成 `//ai-manager.aimbackup` 这类非法路径；失败时会显示坚果云返回的具体错误内容，便于定位配置问题。
- 优化 Windows 发布构建。
  Release workflow 增加 Cargo 缓存、Rust release target 缓存和 Tauri 本地工具缓存，减少重复下载 NSIS 与重复编译耗时。
- 更新自动更新签名配置。
  重新配置 Tauri updater 公钥，并启用本地工具目录缓存，保持安装包更新产物可签名发布。
