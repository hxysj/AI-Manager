export function getFontSize(size = 'base') {
  return Number.parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue(
      `--font-size-${size}`
    )
  )
}
