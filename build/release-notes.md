## 变更说明

- 增强用量数据同步。
  用量页进入后会先触发同步，并在已有会话基础上补充扫描 Skill 会话文件，让新增的可用记录能纳入统计。
- 优化用量页加载状态。
  用量页刷新期间锁定滚动，避免加载遮罩展示时页面内容继续滚动造成视觉干扰。
- 增强 Git 工具归档恢复。
  归档抽屉支持勾选、全选和恢复选中归档，批量恢复后会同步清理已选状态。
- 补齐应用更新配置。
  安装包会生成 Tauri 更新元数据和签名更新包，并通过 GitHub Release 的 `latest.json` 检查、下载和安装新版本。
- 同步应用版本号为 `2.0.2`。
  统一更新 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 和 `src-tauri/tauri.conf.json`。
