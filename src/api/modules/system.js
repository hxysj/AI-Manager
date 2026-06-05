import { request } from '../request'

export const systemApi = {
  selectDirectory: payload => request('system:select-directory', payload),
  selectFile: payload => request('system:select-file', payload),
  openPath: payload => request('system:open-path', payload),
  openExternal: payload => request('system:open-external', payload)
}
