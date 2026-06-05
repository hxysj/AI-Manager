import { request } from '../request'

export const repoApi = {
  addRepo: payload => request('repo:add', payload),
  syncRepo: payload => request('repo:sync', payload),
  syncAllRepos: () => request('repo:sync-all'),
  removeRepo: payload => request('repo:remove', payload)
}
