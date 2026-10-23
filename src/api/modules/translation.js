import { request, subscribe, subscribeReady } from '../request'

export const translationApi = {
  translateText: payload => request('translation:translate', payload),
  // 桌面窗口按需启动，监听建立后再取选区，避免首条消息丢失。
  desktopReady: () => request('translation:desktop-ready'),
  desktopClose: () => request('translation:desktop-close'),
  desktopDrag: () => request('translation:desktop-drag'),
  desktopStatus: () => request('translation:desktop-status'),
  onDesktopSelection: callback => subscribeReady('translation:desktop-selection', callback),
  onDesktopStatus: callback => subscribe('translation:desktop-status', callback),
  // 历史与代理用量通过同一个分页接口查询，完成后通知设置页刷新。
  list: payload => request('translation:list', payload),
  onChanged: callback => subscribe('translation:changed', callback)
}
