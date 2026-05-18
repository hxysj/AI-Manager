# v1.0.1

## 变更说明

- 新增坚果云 WebDAV 云同步配置，并将配置保存到本地设置文件。
- 设置页数据管理新增“保存配置”按钮，方便单独保存云同步信息。
- 修复 Windows 打包时 electron-builder 因隐式发布导致的 `GH_TOKEN` 缺失问题。
- 调整 Windows 打包脚本为显式不发布，交由 GitHub Actions 统一上传安装包。
