## 变更说明

- 完成 Tauri 2 桌面端发布配置。
  支持通过 `v*` tag 触发 GitHub Actions，自动打包 Windows NSIS setup 安装程序并上传到 GitHub Release。
- 同步应用版本号为 `2.0.0`。
  统一更新 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 和 `src-tauri/tauri.conf.json`。
- 调整默认启动窗口尺寸。
  缩小应用初始化窗口，保留现有最小窗口尺寸限制。
- 兼容旧版 Electron 客户端更新检查。
  发布 Windows 安装包时同步生成 `latest.yml`，旧客户端可继续从 GitHub Release 读取 2.0.0 更新元数据。
- 完善 Windows 安装包升级检测。
  Tauri NSIS 安装程序会在安装前检测旧版 Electron 安装记录，手动安装时确认后先卸载旧版本并继续安装，旧客户端自动更新时静默迁移，用户数据默认保留。
