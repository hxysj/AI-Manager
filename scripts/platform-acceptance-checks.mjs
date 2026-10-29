export const manualChecks = [
  ["installation", "安装、首启、系统权限、托盘、自启动、更新、取消授权及卸载"],
  ["providers-runtime", "账号、Provider、CLI 发现与终端启动、Claude Desktop"],
  ["skills-rules", "Skills、仓库、规则与 Prompt 的导入、启停和写入目标应用"],
  ["sessions-usage", "会话读取、回收恢复、用量统计和定价"],
  ["git-tools", "Git 状态、差异、提交与冲突操作，图片工作台和工具导出"],
  ["data-settings", "数据根目录、备份恢复、云同步和日志"],
  ["network", "系统代理、网络检测、端口发现与关闭自建测试进程"],
  ["lan-share", "两台设备配对，文字、图片、文件、群聊和断点续传"],
  ["screenshot", "截图、混合 DPI 多屏、编辑、OCR、贴图分组、拖动和剪贴板"],
  ["long-screenshot", "手动与自动滚动、共享授权次数、完成和取消、撤回共享后重试"],
  ["translation", "跨应用取词、剪贴板翻译、复制恢复、弹窗与快捷键变更"],
  ["floating-window", "桌宠、悬浮工具拖动、收起展开、窗口焦点和多屏切换"]
].map(([id, title]) => ({ id, title }))
