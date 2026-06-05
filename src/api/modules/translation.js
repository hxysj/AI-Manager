import { request, subscribe } from '../request'

export const translationApi = {
  translateText: payload => request('translation:translate', payload),
  onTranslateSelection: callback =>
    subscribe('translation:selection-requested', callback)
}
