## 变更说明

- 完成 Tauri 2 桌面端发布配置。
  支持通过 `v*` tag 触发 GitHub Actions，自动打包 Windows NSIS setup 安装程序并上传到 GitHub Release。
- 同步应用版本号为 `2.0.0`。
  统一更新 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 和 `src-tauri/tauri.conf.json`。
- 调整默认启动窗口尺寸。
  缩小应用初始化窗口，保留现有最小窗口尺寸限制。
