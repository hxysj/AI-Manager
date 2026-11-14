import { invoke, isTauri } from '@tauri-apps/api/core'

const rendererSessionId = crypto.randomUUID()
let runtimeInstalled = false

// 前后端共用 traceId，页面和窗口信息用于区分主面板、工具窗、截图及翻译窗。
export function createLogContext() {
  const params = new URLSearchParams(location.search)
  return {
    traceId: crypto.randomUUID(),
    rendererSessionId,
    panel: params.get('panel') || 'main',
    view: document.querySelector('[data-active-view]')?.dataset.activeView || params.get('tool') || '',
    callSite: new Error().stack || ''
  }
}

export function reportAppError(kind, error, details = {}) {
  if (!isTauri()) return
  const message = error?.message || String(error ?? '未知错误')
  const causes = []
  let cause = error?.cause
  while (cause && causes.length < 5) {
    causes.push(cause.message || String(cause))
    cause = cause.cause
  }
  // 日志写入失败不能再递归记录，也不能覆盖原业务错误。
  try {
    void invoke('dispatch_api', {
      channel: 'app-log:record',
      payload: {
        ...details,
        kind,
        message,
        error: {
          name: error?.name || 'Error',
          message,
          stack: error?.stack || '',
          causes
        },
        context: details.context || createLogContext()
      }
    }).catch(() => {})
  } catch {
    // IPC 尚未可用时保留原有错误处理流程。
  }
}

export function installRuntimeLogging() {
  if (runtimeInstalled) return
  runtimeInstalled = true
  window.addEventListener(
    'error',
    (event) => {
      const target = event.target
      const resourceTag = target?.tagName?.toLowerCase() || ''
      const resourceSource =
        target?.getAttribute?.('src') ?? target?.getAttribute?.('href') ?? ''
      const resourceSourceSet = target?.getAttribute?.('srcset') || ''
      // 空 src 的解析地址可能指向当前页面，不能当成实际失败资源。
      const resourceUrl =
        resourceSource.trim() || resourceSourceSet.trim()
          ? target.currentSrc || target.src || target.href || resourceSource
          : ''
      const resourceMessage = resourceTag
        ? `页面资源加载失败（${resourceTag}${resourceUrl ? '' : '，地址为空'}）`
        : '页面资源加载失败'
      reportAppError(
        'window-error',
        event.error || event.message || resourceMessage,
        {
          context: {
            ...createLogContext(),
            filename: event.filename || resourceUrl,
            ...(resourceTag
              ? { resourceTag, resourceSource, resourceSourceSet, resourceUrl }
              : {}),
            line: event.lineno,
            column: event.colno
          }
        }
      )
    },
    true
  )
  window.addEventListener('unhandledrejection', event => {
    reportAppError('unhandled-rejection', event.reason)
  })
}

export function installAppLogging(app) {
  const previous = app.config.errorHandler
  app.config.errorHandler = (error, instance, info) => {
    reportAppError('vue-error', error, {
      context: {
        ...createLogContext(),
        component: instance?.$options?.name || instance?.$options?.__name || '',
        lifecycle: info
      }
    })
    previous?.(error, instance, info)
  }
  return app
}
