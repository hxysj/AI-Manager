import { request } from '../request'

export const sessionApi = {
  searchSessions: payload => request('session:search', payload),
  loadSessionMessages: payload => request('session:messages', payload),
  deleteSession: payload => request('session:delete', payload),
  listRecycledSessions: () => request('session:recycle-list'),
  restoreSession: payload => request('session:restore', payload),
  purgeSession: payload => request('session:purge', payload)
}
