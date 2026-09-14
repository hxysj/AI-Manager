import { request, subscribe } from '../request'

export const translationApi = {
  translateText: payload => request('translation:translate', payload),
  // 历史与代理用量通过同一个分页接口查询，完成后通知设置页刷新。
  list: payload => request('translation:list', payload),
  onChanged: callback => subscribe('translation:changed', callback)
}
