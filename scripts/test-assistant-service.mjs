import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import { cp, mkdtemp, rm } from 'node:fs/promises'
import { createServer } from 'node:http'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { createInterface } from 'node:readline'
import { randomUUID } from 'node:crypto'

// 使用真实 Mastra、libSQL 和安装目录副本，仅把模型及原生桌面动作替换成本地夹具。
const root = await mkdtemp(join(tmpdir(), 'application-assistant-test-'))
const events = []
const tools = []
const requests = []
const plans = []
const clients = new Set()
let child
let rpc
let stopChild
let toolError = ''
let waitForRequest
let failureMode = false
const server = createServer(async (request, response) => {
  clients.add(response)
  response.on('close', () => clients.delete(response))
  let body = ''
  for await (const chunk of request) body += chunk
  const payload = JSON.parse(body)
  requests.push({ path: request.url, token: request.headers.authorization, payload })
  const plan = failureMode ? { type: 'http-error' } : plans.shift()
  waitForRequest?.()
  waitForRequest = null
  if (plan?.type === 'hang') return
  if (plan?.type === 'http-error') {
    response.writeHead(429, { 'content-type': 'application/json' })
    response.end(JSON.stringify({ error: { message: '测试模型限流', type: 'rate_limit_error', code: 'rate_limit_exceeded' } }))
    return
  }
  response.writeHead(200, { 'content-type': 'text/event-stream' })
  let sequence = 0
  const emit = (type, value) => response.write(`${request.url === '/v1/messages' ? `event: ${type}\n` : ''}data: ${JSON.stringify({ type, sequence_number: sequence++, ...value })}\n\n`)
  const id = `resp_${randomUUID()}`
  if (request.url === '/v1/chat/completions') {
    const chunk = (delta, finish_reason = null) => response.write(`data: ${JSON.stringify({
      id, object: 'chat.completion.chunk', created: 1700000000, model: payload.model,
      choices: [{ index: 0, delta, finish_reason }]
    })}\n\n`)
    if (plan?.type === 'tool') {
      chunk({ role: 'assistant', tool_calls: [{ index: 0, id: 'call_chat', type: 'function', function: { name: plan.name, arguments: JSON.stringify(plan.args) } }] })
      chunk({}, 'tool_calls')
    } else {
      chunk({ role: 'assistant', content: plan?.text || 'Chat 回复' })
      chunk({}, 'stop')
    }
    response.end('data: [DONE]\n\n')
    return
  }
  if (request.url === '/v1/messages') {
    emit('message_start', { message: { id, type: 'message', role: 'assistant', model: payload.model, content: [], stop_reason: null, stop_sequence: null, usage: { input_tokens: 12, output_tokens: 0 } } })
    if (plan?.type === 'tool') {
      emit('content_block_start', { index: 0, content_block: { type: 'tool_use', id: 'call_anthropic', name: plan.name, input: {} } })
      emit('content_block_delta', { index: 0, delta: { type: 'input_json_delta', partial_json: JSON.stringify(plan.args) } })
    } else {
      emit('content_block_start', { index: 0, content_block: { type: 'text', text: '' } })
      emit('content_block_delta', { index: 0, delta: { type: 'text_delta', text: plan?.text || 'Claude 回复' } })
    }
    emit('content_block_stop', { index: 0 })
    emit('message_delta', { delta: { stop_reason: plan?.type === 'tool' ? 'tool_use' : 'end_turn', stop_sequence: null }, usage: { output_tokens: 8 } })
    emit('message_stop', {})
    response.end()
    return
  }
  emit('response.created', { response: { id, created_at: 1700000000, model: payload.model, status: 'in_progress' } })
  const item = plan?.type === 'tool'
    ? { id: 'fc_1', type: 'function_call', call_id: `call_${randomUUID()}`, name: plan.name, arguments: JSON.stringify(plan.args), status: 'completed' }
    : { id: 'msg_1', type: 'message', role: 'assistant', status: 'completed', content: [{ type: 'output_text', text: plan?.text || '模型回复', annotations: [] }] }
  emit('response.output_item.added', { output_index: 0, item: { ...item, ...(plan?.type === 'tool' ? { arguments: '' } : { content: [] }) } })
  if (plan?.type === 'tool') {
    emit('response.function_call_arguments.delta', { output_index: 0, item_id: item.id, delta: item.arguments })
    emit('response.function_call_arguments.done', { output_index: 0, item_id: item.id, arguments: item.arguments })
  } else {
    emit('response.content_part.added', { output_index: 0, item_id: item.id, content_index: 0, part: { type: 'output_text', text: '', annotations: [] } })
    emit('response.output_text.delta', { output_index: 0, item_id: item.id, content_index: 0, delta: item.content[0].text })
  }
  emit('response.output_item.done', { output_index: 0, item })
  emit('response.completed', { response: { id, created_at: 1700000000, model: payload.model, status: 'completed', output: [item], usage: { input_tokens: 12, output_tokens: 8, total_tokens: 20 } } })
  response.end()
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const baseURL = `http://127.0.0.1:${server.address().port}/v1`
const runtimePath = join(root, 'installed', 'assistant')
const storageDir = join(root, 'storage')

async function startChild() {
  const pending = new Map()
  let stderr = ''
  child = spawn(process.execPath, [join(runtimePath, 'agent.mjs')], {
    cwd: root, env: { ...process.env, NODE_PATH: '' }, stdio: ['pipe', 'pipe', 'pipe']
  })
  child.stderr.on('data', bytes => { stderr = (stderr + bytes.toString()).slice(-12000) })
  createInterface({ input: child.stdout }).on('line', line => {
    const message = JSON.parse(line)
    if (message.type === 'event') events.push(message.event)
    if (message.type === 'tool') {
      tools.push(message)
      child.stdin.write(JSON.stringify({
        type: 'tool-result', id: message.id,
        ...(toolError ? { error: toolError } : { result: { action: message.args.action, message: '原生动作已执行' } })
      }) + '\n')
    } else if (message.id) {
      const waiter = pending.get(message.id)
      pending.delete(message.id)
      if (message.error) waiter?.reject(new Error(message.error))
      else waiter?.resolve(message.result)
    }
  })
  child.on('exit', code => {
    for (const waiter of pending.values()) waiter.reject(new Error(`助手进程退出 ${code}: ${stderr}`))
    pending.clear()
  })
  rpc = (method, params = {}) => {
    const id = randomUUID()
    const result = new Promise((resolve, reject) => pending.set(id, { resolve, reject }))
    child.stdin.write(JSON.stringify({ id, method, params }) + '\n')
    return result
  }
  stopChild = async () => {
    const exited = once(child, 'exit')
    child.stdin.end()
    await exited
  }
  const initialized = await rpc('initialize', { storageDir })
  assert.equal(initialized.ready, true)
}
function turn(threadId, message) {
  return { runId: randomUUID(), threadId, message, targetId: 'test-provider', model: 'agent-test', token: 'loopback-only', baseURL }
}
const watchdog = setTimeout(() => {
  console.error('助手测试超过 60 秒')
  child?.kill()
  for (const client of clients) client.destroy()
  server.closeAllConnections()
}, 60000)
try {
  await cp(resolve('src-tauri/node/assistant'), runtimePath, { recursive: true })
  await startChild()
  const thread = await rpc('create-thread')
  assert.ok(thread.id)
  const params = turn(thread.id, '让桌宠读书')
  plans.push({ type: 'tool', name: 'set_pet_action', args: { action: 'read_book', message: '正在读书' } }, { text: '已让桌宠开始读书。' })
  const result = await rpc('run', params)
  assert.equal(result.text, '已让桌宠开始读书。')
  assert.equal(tools.length, 1)
  assert.equal(tools[0].args.action, 'read_book')
  assert.ok(events.some(event => event.type === 'text-delta' && event.text.includes('读书')))
  assert.ok(events.some(event => event.type === 'tool-start' && event.name === 'set_pet_action'))
  assert.ok(events.some(event => event.type === 'tool-end' && event.output.message === '原生动作已执行'))
  assert.equal(requests[0].path, '/v1/responses')
  assert.equal(requests[0].token, 'Bearer loopback-only')
  assert.equal(requests[0].payload.model, 'agent-test')
  assert.equal(requests[0].payload.tools.length, 6)
  assert.ok(JSON.stringify(requests[1].payload.input).includes('原生动作已执行'), '原生结果回到下一轮模型上下文')
  let history = await rpc('messages', { threadId: thread.id })
  assert.ok(JSON.stringify(history.messages).includes('已让桌宠开始读书。'))
  assert.equal(history.messages[0].role, 'user', '每页按对话顺序返回')
  assert.equal(history.messages[0].id, params.runId, '消息 ID 用于恢复窗口时识别当前轮次')
  console.log('真实历史消息结构：', JSON.stringify(history.messages.map(message => ({
    role: message.role, partTypes: message.content.parts?.map(part => part.type)
  }))))
  plans.push({ text: '记得，你让桌宠读书。' })
  await rpc('run', turn(thread.id, '我刚才让你做了什么？'))
  assert.ok(JSON.stringify(requests.at(-1).payload.input).includes('让桌宠读书'), '多轮会话自动加载记忆')

  const other = await rpc('create-thread')
  plans.push({ text: '这是新会话。' })
  await rpc('run', turn(other.id, '新的话题'))
  assert.ok(!JSON.stringify(requests.at(-1).payload.input).includes('让桌宠读书'), '会话之间隔离')
  await stopChild()
  await startChild()
  history = await rpc('messages', { threadId: thread.id })
  assert.ok(JSON.stringify(history.messages).includes('记得，你让桌宠读书。'), '后台重启后本地记忆仍在')
  assert.equal((await rpc('threads')).total, 2)

  const cancelParams = turn(thread.id, '等待取消')
  const requested = new Promise(resolve => { waitForRequest = resolve })
  plans.push({ type: 'hang' })
  const running = rpc('run', cancelParams)
  await requested
  await assert.rejects(rpc('run', turn(other.id, '不应并发')), /已有任务/)
  await rpc('cancel', { runId: cancelParams.runId })
  assert.equal((await running).cancelled, true)
  const earlyCancel = turn(other.id, '启动前取消')
  await rpc('cancel', { runId: earlyCancel.runId })
  assert.equal((await rpc('run', earlyCancel)).cancelled, true)

  failureMode = true
  await assert.rejects(rpc('run', turn(thread.id, '测试限流')), /限流|429/)
  failureMode = false
  toolError = '桌宠尚未启用'
  plans.push({ type: 'tool', name: 'set_pet_action', args: { action: 'sleep' } }, { text: '桌宠尚未启用，动作未执行。' })
  const failedTool = await rpc('run', turn(other.id, '让桌宠睡觉'))
  assert.match(failedTool.text, /未执行/)
  assert.ok(events.some(event => event.type === 'tool-error' && String(event.error).includes('未启用')), '原生工具错误要报告失败')
  toolError = ''
  for (const api of ['chat', 'messages']) {
    const thread = await rpc('create-thread')
    plans.push({ type: 'tool', name: 'set_pet_action', args: { action: 'jump' } }, { text: `${api} 工具调用成功` })
    const result = await rpc('run', { ...turn(thread.id, '让桌宠跳跃'), api })
    assert.equal(result.text, `${api} 工具调用成功`)
    assert.equal(requests.at(-1).path, api === 'chat' ? '/v1/chat/completions' : '/v1/messages')
    assert.ok(JSON.stringify(requests.at(-1).payload.messages).includes('原生动作已执行'))
    await rpc('delete-thread', { threadId: thread.id })
  }
  await rpc('delete-thread', { threadId: other.id })
  assert.equal((await rpc('threads')).total, 1)
  console.log('Mastra 后台验证通过：Responses / Chat / Messages 工具循环、安装目录隔离、流式回复、记忆持久化、会话隔离、并发保护、取消、模型错误和工具失败')
} finally {
  clearTimeout(watchdog)
  if (child?.exitCode === null) await stopChild()
  for (const client of clients) client.destroy()
  server.closeAllConnections()
  await new Promise(resolve => server.close(resolve))
  assert.equal(dirname(root), tmpdir())
  await rm(root, { recursive: true, force: true })
}
