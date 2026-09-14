import { build } from 'esbuild'

// 打包 Node 端依赖，安装版运行时无需从项目 node_modules 查找 LangChain。
await build({
  entryPoints: ['src-tauri/node/translation-service.mjs'],
  outfile: 'src-tauri/node/translation-agent.cjs',
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node20',
  logLevel: 'warning'
})
