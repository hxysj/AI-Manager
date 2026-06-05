import { request } from '../request'

export const runtimeApi = {
  launchCodexProviderInstance: payload =>
    request('codex:launch-provider-instance', payload),
  saveRuntimeModel: payload => request('runtime-model:save', payload),
  switchRuntime: payload => request('runtime:switch', payload),
  clearRuntime: payload => request('runtime:clear', payload),
  compareRuntime: payload => request('runtime:compare', payload),
  getRuntimeConfig: payload => request('runtime:config', payload),
  resolveRuntimeDrift: payload => request('runtime:resolve-drift', payload),
  getRuntimeEnv: payload => request('runtime:env', payload)
}
