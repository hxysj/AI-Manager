## 变更说明

- 修复 Codex 浏览器登录后的面板状态。
  登录接口现在返回刷新后的完整状态，点击打开浏览器登录后，前端面板可以立即进入对应的登录中状态。
- 修复用量页面图表加载展示。
  Token 趋势图和服务商占比图统一在图表区域内显示加载状态，加载内容保持上下居中，避免刷新时只出现单个不居中的 loading。
- 统一用户数据和运行数据目录。
  本地备份、应用调用日志、模型缓存和 Skill zip 导入临时目录统一写入配置数据目录，默认位于 `D:\ai-manager-data`，并在启动时迁移旧 Roaming 目录中的历史数据。
- 同步应用版本号为 `2.0.1`。
  统一更新 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 和 `src-tauri/tauri.conf.json`。
