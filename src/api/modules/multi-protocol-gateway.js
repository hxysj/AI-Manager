import { request } from '../request'

// 网关作为独立工具读写状态，API Key 仅在复制或预览时按需读取。
export const multiProtocolGatewayApi = {
  getState: () => request('gateway:state'),
  getUsage: () => request('gateway:usage'),
  getLogDetail: payload => request('gateway:log-detail', payload),
  saveConfig: payload => request('gateway:save-config', payload),
  enable: payload => request('gateway:enable', payload),
  disable: () => request('gateway:disable'),
  createToken: payload => request('gateway:create-token', payload),
  getTokenValue: payload => request('gateway:token-value', payload),
  deleteToken: payload => request('gateway:delete-token', payload),
  setTokenEnabled: payload => request('gateway:set-token-enabled', payload)
}
