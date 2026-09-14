import { ChatOpenAI } from '@langchain/openai'
import { createAgent } from 'langchain'

// 从标准输入读取原文和本次回环代理凭据，避免文本进入命令行参数。
async function main() {
  let input = ''
  for await (const chunk of process.stdin) input += chunk
  const payload = JSON.parse(input)
  const model = new ChatOpenAI({
    apiKey: payload.token,
    model: payload.model,
    useResponsesApi: true,
    maxRetries: 0,
    configuration: { baseURL: payload.baseURL }
  })
  const agent = createAgent({ model, tools: [], systemPrompt: payload.systemPrompt })
  const result = await agent.invoke({ messages: [{ role: 'user', content: payload.text }] })
  const content = result.messages.at(-1)?.content
  const translatedText = typeof content === 'string'
    ? content
    : (content || []).filter(item => item.type === 'text').map(item => item.text).join('')
  process.stdout.write(JSON.stringify({ translatedText }))
}

main().catch(error => {
  process.stderr.write(error.message || String(error))
  process.exitCode = 1
})
