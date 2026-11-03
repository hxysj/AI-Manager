import { request } from '../request'

// 网关作为独立工具读写状态，创建时返回的明文 Token 不进入应用公共状态。
export const multiProtocolGatewayApi = {
  getState: () => request('gateway:state'),
  getUsage: () => request('gateway:usage'),
  saveConfig: payload => request('gateway:save-config', payload),
  enable: payload => request('gateway:enable', payload),
  disable: () => request('gateway:disable'),
  createToken: payload => request('gateway:create-token', payload),
  deleteToken: payload => request('gateway:delete-token', payload),
  setTokenEnabled: payload => request('gateway:set-token-enabled', payload)
}
