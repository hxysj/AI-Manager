import { request } from '../request'

export const settingsApi = {
  saveSettings: payload => request('settings:save', payload)
}
