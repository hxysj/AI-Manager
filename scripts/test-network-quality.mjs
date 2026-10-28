import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import {
  parse,
  compileScript,
  compileTemplate,
  compileStyleAsync
} from "@vue/compiler-sfc"
import { renderToString } from "@vue/server-renderer"
import { transformSync } from "esbuild"
import * as vue from "vue"

const filename = "src/features/networkQuality/index.vue"
const { descriptor, errors } = parse(readFileSync(filename, "utf8"), {
  filename
})
assert.deepEqual(errors, [])
const script = compileScript(descriptor, {
  id: "network-quality-test",
  genDefaultAs: "__component"
})
const template = compileTemplate({
  source: descriptor.template.content,
  filename,
  id: "network-quality-test",
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])
for (const style of descriptor.styles) {
  const result = await compileStyleAsync({
    source: style.content,
    filename,
    id: "network-quality-test",
    scoped: style.scoped,
    preprocessLang: style.lang
  })
  assert.deepEqual(result.errors, [])
}

const copied = []
const notices = []
const exports = []
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: { clipboard: { writeText: async (address) => copied.push(address) } }
})
const module = { exports: {} }
const icon = { render: () => vue.h("span") }
new Function(
  "require",
  "module",
  "exports",
  transformSync(
    `${script.content}\n${template.code}\n__component.render = render\nexport default __component`,
    { format: "cjs" }
  ).code
)(
  (name) => {
    if (name === "vue") return vue
    if (name === "lucide-vue-next") return new Proxy({}, { get: () => icon })
    if (name === "@/utils/message") {
      return {
        createMessage: {
          success: (text) => notices.push(text),
          error: (text) => {
            throw new Error(text)
          }
        }
      }
    }
    if (name === "@/api") {
      return {
        networkQualityApi: {
          exportReport: async (payload) => exports.push(payload)
        },
        systemApi: { saveFile: async () => "network-quality.json" }
      }
    }
    throw new Error(`未处理的测试依赖：${name}`)
  },
  module,
  module.exports
)

const source = (module, id, name, url) => ({ module, id, name, url })
const fixture = {
  schema: 1,
  status: "success",
  timestamp: "2026-10-28T00:00:00Z",
  transport: {
    family: "auto",
    proxy: "system:http://127.0.0.1:7890",
    maskIp: false
  },
  identity: { ipv4: { address: "1.1.1.1", sourceVotes: 2 } },
  ipIntelligence: {},
  egressIps: [
    {
      id: "ipv4-0",
      family: "ipv4",
      address: "1.1.1.1",
      sources: [
        source(
          "identity",
          "cloudflare",
          "Cloudflare",
          "https://www.cloudflare.com/cdn-cgi/trace"
        ),
        source("identity", "ipify", "ipify", "https://api64.ipify.org/"),
        source(
          "geo",
          "cloudflare",
          "Cloudflare",
          "https://www.cloudflare.com/cdn-cgi/trace"
        )
      ]
    },
    {
      id: "ipv4-1",
      family: "ipv4",
      address: "8.8.8.8",
      sources: [source("identity", "ident_me", "ident.me", "https://ident.me/")]
    },
    {
      id: "ipv6-0",
      family: "ipv6",
      address: "2606:4700:4700::1111",
      sources: [
        source(
          "geo",
          "cloudflare",
          "Cloudflare",
          "https://www.cloudflare.com/cdn-cgi/trace"
        )
      ]
    }
  ],
  geo: [],
  connectivityChecks: { results: [] },
  serviceAccess: [{ id: "openai", name: "OpenAI", state: "reachable" }],
  aiEndpoints: [],
  connectivity: { targets: [{ resolvedIp: "9.9.9.9", verdict: {} }] }
}
const component = module.exports.default
const setup = component.setup
let state
let currentReport = fixture
const mounted = {
  ...component,
  setup(props, context) {
    const bindings = setup(props, context)
    state = vue.proxyRefs(bindings)
    state.report = currentReport
    state.status = "success"
    return bindings
  }
}
const render = () => renderToString(vue.createSSRApp(mounted))

// 使用真实模板检查顶部汇总、来源标签和复制、导出处理，目标服务器地址不能混入。
let html = await render()
const summary = html.slice(
  html.indexOf("network-quality-egress-panel"),
  html.indexOf("network-quality-dashboard-grid")
)
assert.match(summary, /检测到的出口 IP/)
assert.match(summary, /3 个地址/)
assert.equal((summary.match(/class="egress-ip-item"/g) || []).length, 3)
for (const address of ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"]) {
  assert.ok(summary.includes(address))
}
assert.match(summary, /公网身份\s+· Cloudflare/)
assert.match(summary, /公网身份\s+· ipify/)
assert.match(summary, /地理位置\s+· Cloudflare/)
assert.match(summary, /公网身份\s+· ident.me/)
assert.ok(!summary.includes("9.9.9.9"))
assert.ok(
  !summary.includes("OpenAI"),
  "未返回出口地址的检测不能被推测归到某个 IP"
)
assert.deepEqual(state.egressIps, fixture.egressIps)
await state.copyAddress("8.8.8.8")
assert.deepEqual(copied, ["8.8.8.8"])
assert.equal(state.copiedAddress, "8.8.8.8")
state.request.maskIp = true
await state.exportJson()
assert.equal(exports[0].maskIp, true)
assert.deepEqual(exports[0].report.egressIps, fixture.egressIps)
assert.ok(notices.includes("网络质量报告已导出。"))

currentReport = {
  ...fixture,
  egressIps: [
    { ...fixture.egressIps[0], address: "8.8.*.*" },
    { ...fixture.egressIps[1], address: "8.8.*.*" }
  ]
}
html = await render()
const maskedSummary = html.slice(
  html.indexOf("network-quality-egress-panel"),
  html.indexOf("network-quality-dashboard-grid")
)
assert.equal((maskedSummary.match(/class="egress-ip-item"/g) || []).length, 2)
assert.match(maskedSummary, /2 个地址/)
assert.equal(state.egressIps.length, 2, "两个不同出口不能因掩码相同而合并")

currentReport = { ...fixture, identity: {}, egressIps: [] }
html = await render()
assert.match(html, /本次检测未返回可确认的公网出口 IP/)
assert.equal(state.egressIps.length, 0)
currentReport = null
html = await render()
assert.ok(!html.includes('class="egress-ip-item"'))
console.log("网络质量出口 IP 汇总、检测来源、复制、导出和掩码展示验证通过")
