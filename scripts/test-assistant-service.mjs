import assert from "node:assert/strict"
import { spawn } from "node:child_process"
import { once } from "node:events"
import { cp, mkdtemp, rm } from "node:fs/promises"
import { createServer } from "node:http"
import { tmpdir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { createInterface } from "node:readline"
import { randomUUID } from "node:crypto"

// 使用真实 Mastra、libSQL 和安装目录副本，仅把模型及原生桌面动作替换成本地夹具。
const root = await mkdtemp(join(tmpdir(), "application-assistant-test-"))
const events = []
const tools = []
const requests = []
const plans = []
const clients = new Set()
let child
let rpc
let stopChild
let toolError = ""
let waitForRequest
let failureMode = false
let repeatedFailure
const server = createServer(async (request, response) => {
  clients.add(response)
  response.on("close", () => clients.delete(response))
  let body = ""
  for await (const chunk of request) body += chunk
  const payload = JSON.parse(body)
  requests.push({
    path: request.url,
    token: request.headers.authorization,
    payload
  })
  const plan =
    plans.shift() ||
    repeatedFailure ||
    (failureMode ? { type: "http-error" } : undefined)
  waitForRequest?.()
  waitForRequest = null
  if (plan?.type === "hang") return
  if (plan?.type === "http-error") {
    if (plan.repeat) repeatedFailure = plan
    response.writeHead(plan.status || 429, { "content-type": "application/json" })
    response.end(
      JSON.stringify({
        error: {
          message: plan.message || "测试模型限流",
          type: plan.errorType || "rate_limit_error",
          code: plan.code || "rate_limit_exceeded",
          ...(plan.detail ? { detail: plan.detail } : {})
        }
      })
    )
    return
  }
  response.writeHead(200, { "content-type": "text/event-stream" })
  let sequence = 0
  const emit = (type, value) =>
    response.write(
      `${request.url === "/v1/messages" ? `event: ${type}\n` : ""}data: ${JSON.stringify({ type, sequence_number: sequence++, ...value })}\n\n`
    )
  const id = `resp_${randomUUID()}`
  if (request.url === "/v1/chat/completions") {
    const chunk = (delta, finish_reason = null) =>
      response.write(
        `data: ${JSON.stringify({
          id,
          object: "chat.completion.chunk",
          created: 1700000000,
          model: payload.model,
          choices: [{ index: 0, delta, finish_reason }]
        })}\n\n`
      )
    if (plan?.type === "tool") {
      chunk({
        role: "assistant",
        tool_calls: [
          {
            index: 0,
            id: "call_chat",
            type: "function",
            function: { name: plan.name, arguments: JSON.stringify(plan.args) }
          }
        ]
      })
      chunk({}, "tool_calls")
    } else {
      chunk({ role: "assistant", content: plan?.text || "Chat 回复" })
      chunk({}, "stop")
    }
    response.end("data: [DONE]\n\n")
    return
  }
  if (request.url === "/v1/messages") {
    emit("message_start", {
      message: {
        id,
        type: "message",
        role: "assistant",
        model: payload.model,
        content: [],
        stop_reason: null,
        stop_sequence: null,
        usage: { input_tokens: 12, output_tokens: 0 }
      }
    })
    if (plan?.type === "tool") {
      emit("content_block_start", {
        index: 0,
        content_block: {
          type: "tool_use",
          id: "call_anthropic",
          name: plan.name,
          input: {}
        }
      })
      emit("content_block_delta", {
        index: 0,
        delta: {
          type: "input_json_delta",
          partial_json: JSON.stringify(plan.args)
        }
      })
    } else {
      emit("content_block_start", {
        index: 0,
        content_block: { type: "text", text: "" }
      })
      emit("content_block_delta", {
        index: 0,
        delta: { type: "text_delta", text: plan?.text || "Claude 回复" }
      })
    }
    emit("content_block_stop", { index: 0 })
    emit("message_delta", {
      delta: {
        stop_reason: plan?.type === "tool" ? "tool_use" : "end_turn",
        stop_sequence: null
      },
      usage: { output_tokens: 8 }
    })
    emit("message_stop", {})
    response.end()
    return
  }
  emit("response.created", {
    response: {
      id,
      created_at: 1700000000,
      model: payload.model,
      status: "in_progress"
    }
  })
  const item =
    plan?.type === "tool"
      ? {
          id: "fc_1",
          type: "function_call",
          call_id: `call_${randomUUID()}`,
          name: plan.name,
          arguments: JSON.stringify(plan.args),
          status: "completed"
        }
      : {
          id: "msg_1",
          type: "message",
          role: "assistant",
          status: "completed",
          content: [
            {
              type: "output_text",
              text: plan?.text || "模型回复",
              annotations: []
            }
          ]
        }
  emit("response.output_item.added", {
    output_index: 0,
    item: {
      ...item,
      ...(plan?.type === "tool" ? { arguments: "" } : { content: [] })
    }
  })
  if (plan?.type === "tool") {
    emit("response.function_call_arguments.delta", {
      output_index: 0,
      item_id: item.id,
      delta: item.arguments
    })
    emit("response.function_call_arguments.done", {
      output_index: 0,
      item_id: item.id,
      arguments: item.arguments
    })
  } else {
    emit("response.content_part.added", {
      output_index: 0,
      item_id: item.id,
      content_index: 0,
      part: { type: "output_text", text: "", annotations: [] }
    })
    emit("response.output_text.delta", {
      output_index: 0,
      item_id: item.id,
      content_index: 0,
      delta: item.content[0].text
    })
  }
  emit("response.output_item.done", { output_index: 0, item })
  emit("response.completed", {
    response: {
      id,
      created_at: 1700000000,
      model: payload.model,
      status: "completed",
      output: [item],
      usage: { input_tokens: 12, output_tokens: 8, total_tokens: 20 }
    }
  })
  response.end()
})
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve))
const baseURL = `http://127.0.0.1:${server.address().port}/v1`
const runtimePath = join(root, "installed", "assistant")
const storageDir = join(root, "storage")

async function startChild() {
  const pending = new Map()
  let stderr = ""
  child = spawn(process.execPath, [join(runtimePath, "agent.mjs")], {
    cwd: root,
    env: { ...process.env, NODE_PATH: "" },
    stdio: ["pipe", "pipe", "pipe"]
  })
  child.stderr.on("data", (bytes) => {
    stderr = (stderr + bytes.toString()).slice(-12000)
  })
  createInterface({ input: child.stdout }).on("line", (line) => {
    const message = JSON.parse(line)
    if (message.type === "event") events.push(message.event)
    if (message.type === "tool") {
      tools.push(message)
      child.stdin.write(
        JSON.stringify({
          type: "tool-result",
          id: message.id,
          ...(toolError
            ? { error: toolError }
            : {
                result: {
                  action: message.args.action,
                  message: "原生动作已执行"
                }
              })
        }) + "\n"
      )
    } else if (message.id) {
      const waiter = pending.get(message.id)
      pending.delete(message.id)
      if (message.error) waiter?.reject(new Error(message.error))
      else waiter?.resolve(message.result)
    }
  })
  child.on("exit", (code) => {
    for (const waiter of pending.values())
      waiter.reject(new Error(`助手进程退出 ${code}: ${stderr}`))
    pending.clear()
  })
  rpc = (method, params = {}) => {
    const id = randomUUID()
    const result = new Promise((resolve, reject) =>
      pending.set(id, { resolve, reject })
    )
    child.stdin.write(JSON.stringify({ id, method, params }) + "\n")
    return result
  }
  stopChild = async () => {
    const exited = once(child, "exit")
    child.stdin.end()
    await exited
  }
  const initialized = await rpc("initialize", { storageDir })
  assert.equal(initialized.ready, true)
}
function turn(threadId, message) {
  return {
    runId: randomUUID(),
    threadId,
    message,
    targetId: "test-provider",
    model: "agent-test",
    token: "loopback-only",
    baseURL
  }
}
const watchdog = setTimeout(() => {
  console.error("助手测试超过 60 秒")
  child?.kill()
  for (const client of clients) client.destroy()
  server.closeAllConnections()
}, 60000)
try {
  await cp(resolve("src-tauri/node/assistant"), runtimePath, {
    recursive: true
  })
  await startChild()
  const thread = await rpc("create-thread")
  assert.ok(thread.id)
  const params = turn(thread.id, "让桌宠读书")
  plans.push(
    {
      type: "tool",
      name: "set_pet_action",
      args: { action: "read_book", message: "正在读书" }
    },
    { text: "已让桌宠开始读书。" }
  )
  const result = await rpc("run", params)
  assert.equal(result.text, "已让桌宠开始读书。")
  assert.equal(tools.length, 1)
  assert.equal(tools[0].args.action, "read_book")
  assert.ok(
    events.some(
      (event) => event.type === "text-delta" && event.text.includes("读书")
    )
  )
  assert.ok(
    events.some(
      (event) => event.type === "tool-start" && event.name === "set_pet_action"
    )
  )
  assert.ok(
    events.some(
      (event) =>
        event.type === "tool-end" && event.output.message === "原生动作已执行"
    )
  )
  assert.equal(requests[0].path, "/v1/responses")
  assert.equal(requests[0].token, "Bearer loopback-only")
  assert.equal(requests[0].payload.model, "agent-test")
  assert.equal(requests[0].payload.tools.length, 10)
  assert.ok(
    JSON.stringify(requests[1].payload.input).includes("原生动作已执行"),
    "原生结果回到下一轮模型上下文"
  )
  let history = await rpc("messages", { threadId: thread.id })
  assert.ok(JSON.stringify(history.messages).includes("已让桌宠开始读书。"))
  assert.equal(history.messages[0].role, "user", "每页按对话顺序返回")
  assert.equal(
    history.messages[0].id,
    params.runId,
    "消息 ID 用于恢复窗口时识别当前轮次"
  )
  console.log(
    "真实历史消息结构：",
    JSON.stringify(
      history.messages.map((message) => ({
        role: message.role,
        partTypes: message.content.parts?.map((part) => part.type)
      }))
    )
  )
  plans.push({ text: "记得，你让桌宠读书。" })
  await rpc("run", turn(thread.id, "我刚才让你做了什么？"))
  assert.ok(
    JSON.stringify(requests.at(-1).payload.input).includes("让桌宠读书"),
    "多轮会话自动加载记忆"
  )

  const other = await rpc("create-thread")
  plans.push({ text: "这是新会话。" })
  await rpc("run", turn(other.id, "新的话题"))
  assert.ok(
    !JSON.stringify(requests.at(-1).payload.input).includes("让桌宠读书"),
    "会话之间隔离"
  )
  await stopChild()
  await startChild()
  history = await rpc("messages", { threadId: thread.id })
  assert.ok(
    JSON.stringify(history.messages).includes("记得，你让桌宠读书。"),
    "后台重启后本地记忆仍在"
  )
  assert.equal((await rpc("threads")).total, 2)

  const cancelParams = turn(thread.id, "等待取消")
  const requested = new Promise((resolve) => {
    waitForRequest = resolve
  })
  plans.push({ type: "hang" })
  const running = rpc("run", cancelParams)
  await requested
  await assert.rejects(rpc("run", turn(other.id, "不应并发")), /已有任务/)
  await rpc("cancel", { runId: cancelParams.runId })
  assert.equal((await running).cancelled, true)
  const earlyCancel = turn(other.id, "启动前取消")
  await rpc("cancel", { runId: earlyCancel.runId })
  assert.equal((await rpc("run", earlyCancel)).cancelled, true)

  failureMode = true
  await assert.rejects(rpc("run", turn(thread.id, "测试限流")), /限流|429/)
  failureMode = false
  toolError = "桌宠尚未启用"
  plans.push(
    { type: "tool", name: "set_pet_action", args: { action: "sleep" } },
    { text: "桌宠尚未启用，动作未执行。" }
  )
  const failedTool = await rpc("run", turn(other.id, "让桌宠睡觉"))
  assert.match(failedTool.text, /未执行/)
  assert.ok(
    events.some(
      (event) =>
        event.type === "tool-error" && String(event.error).includes("未启用")
    ),
    "原生工具错误要报告失败"
  )
  toolError = ""
  plans.splice(0)
  const upstreamFailureThread = await rpc("create-thread")
  plans.push(
    {
      type: "tool",
      name: "set_pet_action",
      args: { action: "jump" }
    },
    {
      type: "http-error",
      status: 502,
      message: "Upstream request failed（HTTP 502）：工具结果无法继续处理",
      errorType: "upstream_error",
      code: "upstream_unavailable",
      repeat: true
    }
  )
  await assert.rejects(
    rpc("run", turn(upstreamFailureThread.id, "让桌宠跳跃后继续回答")),
    /工具结果无法继续处理|502/,
    "工具循环后的模型错误要保留上游原因和状态码"
  )
  assert.equal(
    requests.some((request) =>
      JSON.stringify(request.payload).includes("HTTP 502）（HTTP 502")
    ),
    false,
    "错误消息不能重复追加 HTTP 状态码"
  )
  failureMode = false
  repeatedFailure = undefined
  await rpc("delete-thread", { threadId: upstreamFailureThread.id })
  for (const api of ["chat", "messages"]) {
    const thread = await rpc("create-thread")
    plans.push(
      { type: "tool", name: "set_pet_action", args: { action: "jump" } },
      { text: `${api} 工具调用成功` }
    )
    const result = await rpc("run", { ...turn(thread.id, "让桌宠跳跃"), api })
    assert.equal(result.text, `${api} 工具调用成功`)
    assert.equal(
      requests.at(-1).path,
      api === "chat" ? "/v1/chat/completions" : "/v1/messages"
    )
    assert.ok(
      JSON.stringify(requests.at(-1).payload.messages).includes(
        "原生动作已执行"
      )
    )
    await rpc("delete-thread", { threadId: thread.id })
  }
  // 公共记忆独立于会话，跨窗口编辑冲突、停用注入和重启恢复都走真实 libSQL。
  await rpc("save-public-memory", {
    text: "用户希望使用简体中文，项目名称是小猴助手。",
    previous: ""
  })
  await assert.rejects(
    rpc("save-public-memory", { text: "过期编辑", previous: "" }),
    /其他窗口更新/
  )
  plans.push({ text: "我记得项目名。" })
  await rpc("run", turn(other.id, "我有什么偏好？"))
  assert.match(JSON.stringify(requests.at(-1).payload), /小猴助手/)
  plans.push({ text: "公共记忆已关闭。" })
  await rpc("run", {
    ...turn(other.id, "新消息"),
    settings: { sharedMemoryEnabled: false }
  })
  assert.ok(
    !JSON.stringify(requests.at(-1).payload).includes("项目名称是小猴助手"),
    "关闭后不注入公共记忆"
  )
  plans.push(
    {
      type: "tool",
      name: "update_public_memory",
      args: { content: "用户喜欢简洁回答，项目名称是小猴助手。" }
    },
    { text: "已记住。" }
  )
  await rpc("run", turn(thread.id, "请记住我喜欢简洁回答"))
  assert.match((await rpc("public-memory")).text, /简洁回答/)
  assert.ok(
    !tools.some((tool) => tool.name === "update_public_memory"),
    "记忆工具在 Mastra 内执行，不经过原生动作桥"
  )
  const originalHistory = await rpc("messages", { threadId: thread.id })
  const observation =
    "<observations>\n- 用户让桌宠读书，随后要求简洁回答；项目名称是小猴助手。保留执行成功记录。\n</observations>\n<current-task>\n等待下一条用户消息。\n</current-task>"
  plans.push({ text: observation })
  const compressed = await rpc("compress", {
    ...turn(thread.id, ""),
    settings: { contextWindow: 32000 }
  })
  assert.equal(compressed.compressed, true)
  let context = await rpc("context", { threadId: thread.id })
  assert.match(context.summary, /桌宠读书/)
  assert.ok(context.lastObservedAt)
  const compressedHistory = await rpc("messages", { threadId: thread.id })
  assert.deepEqual(
    compressedHistory.messages
      .filter((message) => message.role === "user")
      .map((message) => message.id),
    originalHistory.messages
      .filter((message) => message.role === "user")
      .map((message) => message.id),
    "压缩保留原始消息"
  )
  plans.push({ text: "摘要承接了之前的要求。" })
  await rpc("run", turn(thread.id, "继续"))
  assert.match(JSON.stringify(requests.at(-1).payload), /桌宠读书/)
  assert.ok(
    !JSON.stringify(requests.at(-1).payload).includes("测试限流"),
    "已压缩的原始消息不再反复发送"
  )
  const savedSummary = (await rpc("context", { threadId: thread.id })).summary
  failureMode = true
  await assert.rejects(rpc("compress", turn(thread.id, "")), /限流|429/)
  failureMode = false
  assert.equal(
    (await rpc("context", { threadId: thread.id })).summary,
    savedSummary,
    "摘要生成失败不覆盖成功检查点"
  )
  await stopChild()
  await startChild()
  context = await rpc("context", { threadId: thread.id })
  assert.equal(context.summary, savedSummary, "重启恢复摘要和游标")
  assert.match((await rpc("public-memory")).text, /简洁回答/)
  plans.push({ text: "重启后继续原会话。" })
  await rpc("run", turn(thread.id, "重启后继续"))
  assert.match(JSON.stringify(requests.at(-1).payload), /桌宠读书/)
  const autoThread = await rpc("create-thread")
  plans.push({ text: "保留长消息。" })
  await rpc("run", {
    ...turn(autoThread.id, "原始长消息：" + "配置项目约束".repeat(1700)),
    settings: { autoCompress: false }
  })
  const beforeAuto = requests.length
  plans.push(
    {
      text: "<observations>\n- 用户提供了长配置，必须保留项目约束。\n</observations>"
    },
    { text: "已按配置继续。" }
  )
  await rpc("run", {
    ...turn(autoThread.id, "继续长任务"),
    settings: { contextWindow: 8192, compressionThreshold: 30 }
  })
  assert.ok(
    requests.length >= beforeAuto + 2,
    "达到阈值后自动调用摘要模型，再继续回答"
  )
  assert.match(
    (await rpc("context", { threadId: autoThread.id })).summary,
    /项目约束/
  )
  // 图片以三种协议的原生多模态结构发送，并由历史恢复，不退化为文字占位。
  const visionThread = await rpc("create-thread")
  const png =
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9ZlS4AAAAASUVORK5CYII="
  for (const api of ["responses", "chat", "messages"]) {
    plans.push({ text: "已查看图片" })
    await rpc("run", {
      ...turn(visionThread.id, "查看图片"),
      api,
      images: [{ dataUrl: png, mimeType: "image/png" }],
      settings: {
        autoCompress: false,
        disabledTools: ["set_pet_action", "update_public_memory"],
        skills: [
          {
            id: "own",
            name: "助手专属技能",
            description: "测试目录",
            instructions: "不应自动发送整份技能正文",
            enabled: true
          },
          { id: "off", name: "已停用技能", enabled: false }
        ]
      }
    })
    const body = JSON.stringify(requests.at(-1).payload)
    assert.ok(body.includes("iVBORw0KGgo"), api + " 必须发送图片数据")
    assert.ok(body.includes("助手专属技能"))
    assert.ok(
      !body.includes("不应自动发送整份技能正文"),
      "技能正文应按需通过工具读取"
    )
    const registered = requests
      .at(-1)
      .payload.tools.map((tool) => tool.name || tool.function?.name)
    assert.ok(
      !registered.includes("set_pet_action"),
      "停用工具不进入模型工具清单"
    )
    assert.ok(!registered.includes("update_public_memory"))
    assert.ok(registered.includes("read_software_skills"))
    assert.ok(registered.includes("read_assistant_skills"))
  }
  const imageHistory = await rpc("messages", { threadId: visionThread.id })
  assert.equal(
    imageHistory.messages.filter(
      (message) =>
        message.role === "user" &&
        message.content.parts.some(
          (part) =>
            part.type === "file" &&
            part.mimeType === "image/png" &&
            part.data === png
        )
    ).length,
    3,
    "三种协议均以 Mastra 的 file part 恢复图片历史"
  )
  await rpc("delete-thread", { threadId: visionThread.id })

  await rpc("delete-thread", { threadId: autoThread.id })
  await rpc("delete-thread", { threadId: other.id })
  assert.equal((await rpc("threads")).total, 1)
  console.log(
    "Mastra 后台验证通过：三协议工具循环、安装目录隔离、流式回复、自动/手动压缩、摘要失败保留、重启恢复、跨会话公共记忆、停用注入和编辑冲突、取消与错误"
  )
} finally {
  clearTimeout(watchdog)
  if (child?.exitCode === null) await stopChild()
  for (const client of clients) client.destroy()
  server.closeAllConnections()
  await new Promise((resolve) => server.close(resolve))
  assert.equal(dirname(root), tmpdir())
  await rm(root, { recursive: true, force: true })
}
