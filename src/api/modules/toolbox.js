import { request } from '../request'

export const toolboxApi = {
  openToolbox: () => request('tools:open-toolbox')
}
