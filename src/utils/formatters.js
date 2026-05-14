const statusLabels = {
  installed: '已安装',
  'not-installed': '未安装',
  'broken-link': '链接损坏',
  disabled: '不可用'
}

export function formatDateTime(value) {
  if (!value) {
    return '未记录'
  }

  return new Intl.DateTimeFormat('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit'
  }).format(new Date(value))
}

export function formatStatusLabel(value) {
  return statusLabels[value] || value || '未知'
}

export function formatCount(value, singularLabel, pluralLabel = singularLabel) {
  const count = Number(value || 0)
  const label = count === 1 ? singularLabel : pluralLabel
  return `${count} ${label}`
}

export function iconLetters(value) {
  return String(value || '')
    .replace(/[^a-zA-Z0-9]/g, '')
    .slice(0, 2)
    .toUpperCase() || 'AI'
}

export function hashColor(value) {
  const source = String(value || 'ai-manager')
  let seed = 0

  for (let index = 0; index < source.length; index += 1) {
    seed = source.charCodeAt(index) + ((seed << 5) - seed)
  }

  const hue = Math.abs(seed % 360)
  const secondHue = (hue + 42) % 360

  return `linear-gradient(135deg, hsl(${hue} 78% 56%), hsl(${secondHue} 82% 46%))`
}
