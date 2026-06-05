import { request } from '../request'

export const providerApi = {
  saveProvider: payload => request('provider:save', payload),
  deleteProvider: payload => request('provider:delete', payload)
}
