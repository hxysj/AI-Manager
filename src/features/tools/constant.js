import {
  Activity,
  Braces,
  FileDiff,
  GitBranchIcon,
  Image as ImageIcon,
  Images,
  Network,
  PawPrint,
  ScanLine,
  Share2
} from 'lucide-vue-next'

// 工具中心、悬浮窗和设置共用入口，保持名称、图标及可用条件一致。
export function getToolItems({ cliTargets = [], repos = [] } = {}) {
  const items = [
    {
      id: 'image-workbench',
      label: '图片工作台',
      summary: '使用官方账号通过 Web 或 Codex 生成、编辑图片，管理与导出任务。',
      meta: '文生图 / 图片编辑',
      icon: Images,
      theme: 'sky'
    },
    {
      id: 'json-agent',
      label: 'JSON 智能解析',
      summary: '格式化 JSON，并通过 Codex Agent 按指令修复异常内容。',
      meta: 'JSON / Agent',
      icon: Braces,
      theme: 'purple'
    },
    {
      id: 'string-diff',
      label: '差异对比',
      summary: '对比两个字符串或 JSON 内容，定位路径、行和字符差异。',
      meta: '文本 / JSON',
      icon: FileDiff,
      theme: 'green'
    },
    {
      id: 'image-link-extractor',
      label: '图片链接提取',
      summary: '从文本、Markdown、HTML 或 JSON 中提取图片并批量导出。',
      meta: '图片 / 导出',
      icon: ImageIcon,
      theme: 'orange'
    },
    {
      id: 'coordinate-overlay',
      label: '坐标框预览',
      summary: '载入题目接口 JSON，在原图上查看题框与作答框并控制单框显隐。',
      meta: '题框 / 作答框',
      icon: ScanLine,
      theme: 'blue'
    },
    {
      // 端口监测直接使用桌面端系统权限，不依赖浏览器工具服务。
      id: 'port-monitor',
      label: '端口监测',
      summary: '查看本机监听端口、所属程序和系统服务，并按需关闭进程。',
      meta: '本机进程',
      icon: Network,
      theme: 'blue'
    },
    {
      id: 'network-quality',
      label: '网络质量监测',
      summary: '检测公网出口、地理位置、系统连通性、服务可用性与目标连接质量。',
      meta: '延迟 / 丢包 / 路径',
      icon: Activity,
      theme: 'green'
    },
    {
      id: 'git',
      label: 'Git 管理',
      summary: '管理项目的本地分支归档、提交检查和 stash 归档。',
      meta: `${repos.length} 个项目`,
      icon: GitBranchIcon,
      theme: 'rose'
    },
    {
      id: 'lan-share',
      label: '设备快传',
      summary: '客户端直接连接，聊天中发送文件与图片；网页仅作为访客入口。',
      meta: '自动发现',
      icon: Share2,
      theme: 'violet'
    }
  ]

  // 仅在应用状态确认本机已安装 Codex 后提供宠物管理入口。
  if (cliTargets.some(target => target.id === 'codex' && target.installed === true)) {
    items.push({
      id: 'codex-pets',
      label: 'Codex 宠物',
      summary: '管理 Codex 宠物的名称、启用状态和本地文件。',
      meta: '本机 Codex',
      icon: PawPrint,
      theme: 'sky'
    })
  }

  return items
}
