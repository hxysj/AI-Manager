import assert from "node:assert/strict"
import { execFile } from "node:child_process"
import { existsSync, readFileSync } from "node:fs"
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { promisify } from "node:util"
import { createHash } from "node:crypto"
import { build } from "esbuild"
import { compileScript, compileStyleAsync, parse } from "@vue/compiler-sfc"
import less from "less"

// 实际挂载便签工作区与助手入口，使用隔离的本地接口和无头浏览器验证操作，不触碰用户数据。
const browser = [
  process.env.NOTES_TEST_BROWSER,
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Google/Chrome/Application/chrome.exe"
].find((path) => path && existsSync(path))
assert.ok(browser, "请设置 NOTES_TEST_BROWSER 指向 Chromium 浏览器")
const root = await mkdtemp(join(tmpdir(), "quick-notes-ui-"))
const artifact = resolve("artifacts/quick-notes")
const styles = []
const mock = `
import {ref} from 'vue'
export const calls=[]
export const handlers=new Set()
const time=Date.now()
export const state={notes:[
 {id:'work',title:'完成速记模块的存储设计',content:'整理了便签、待办和提醒的数据关系，下一步验证重启恢复。',type:'worklog',groupId:'project',categoryId:'development',tags:['AI-Manager','本地存储'],color:'mint',pinned:true,occurredOn:'2026-11-18',createdAt:time,updatedAt:time,revision:1,lifecycle:'active',tasks:[{id:'t',title:'验证重启后的提醒恢复',status:'pending',dueDate:'2026-11-19'}],worklog:{result:'完成存储设计'}},
 {id:'idea',title:'周五的发布检查',content:'检查 Claude 和 Codex 的切换，再整理这周的工作记录。',type:'todo',groupId:'inbox',categoryId:'debugging',tags:['发布'],color:'sand',createdAt:time,updatedAt:time,revision:1,lifecycle:'active',tasks:[{id:'t2',title:'检查发布包',status:'pending'}],worklog:{}}
],folders:[{id:'inbox',kind:'group',name:'收件箱',aliases:[]},{id:'project',kind:'group',name:'AI-Manager',aliases:['桌宠']},{id:'uncategorized',kind:'category',name:'未分类',aliases:[]},{id:'development',kind:'category',name:'开发实现',aliases:[]},{id:'debugging',kind:'category',name:'问题排查',aliases:[]}],events:[],reminders:[],captures:[],hasMore:false}
const changed=()=>{for(const handle of handlers)handle({})}
export const quickNotesApi={onChanged:async fn=>{handlers.add(fn);return()=>handlers.delete(fn)},onOpen:async()=>()=>{},invoke:async(action,p={})=>{
 p=JSON.parse(JSON.stringify(p))
 calls.push({action,p:JSON.parse(JSON.stringify(p))})
 if(action==='options')return {model:{targetId:'local',model:'qwen3.5-0.8b',protocol:'chat'}}
 if(action==='list')return {...structuredClone(state),notes:structuredClone(state.notes.filter(n=>n.lifecycle===(p.lifecycle||'active')&&(!p.groupId||n.groupId===p.groupId)&&(!p.categoryId||n.categoryId===p.categoryId)&&(!p.search||JSON.stringify(n).includes(p.search))))}
 if(action==='detail')return {...structuredClone(state.notes.find(n=>n.id===p.id)),reminders:structuredClone(state.reminders.filter(r=>r.noteId===p.id)),history:[],captures:[]}
 if(action==='save'){const old=state.notes.find(n=>n.id===p.id);const note={...old,...p,id:old?.id||crypto.randomUUID(),revision:(old?.revision||0)+1,createdAt:old?.createdAt||Date.now(),lifecycle:p.lifecycle||'active'};if(old)Object.assign(old,note);else state.notes.push(note);changed();return structuredClone(note)}
 if(action==='task-update'){const n=state.notes.find(n=>n.id===p.id);n.tasks.find(t=>t.id===p.taskId).status=p.status;n.revision++;changed();return structuredClone(n)}
 if(action==='archive'||action==='trash'||action==='restore'){const n=state.notes.find(n=>n.id===p.id);n.lifecycle=action==='archive'?'archived':action==='trash'?'trash':'active';n.revision++;changed();return structuredClone(n)}
 if(action==='capture'){globalThis.lastCapture=p;return {id:'capture',requestId:p.requestId,status:'running'}}
 if(action==='capture-state'){return {id:'capture',requestId:p.requestId,status:'completed',result:structuredClone(state.notes[0])}}
 if(action==='organize'){state.folders.push({...p,id:crypto.randomUUID()});changed();return {id:'folder'}}
 return {ok:true}
}}
export const settingsApi={saveSettings:async()=>{throw Error('速记模型选择不应改写助手配置')}}
export const request=async(channel,p)=>channel==='local-models:installed'?{models:[{id:'qwen3.5-0.8b'}]}:{}
export const subscribeReady=async()=>()=>{}
`
const fixture = `
import {createApp,nextTick} from 'vue'
import Notes from './src/features/quick-notes/index.vue'
import {quickNotesApi,calls,state} from 'notes-mock'
const assert=(value,message)=>{if(!value)throw Error(message)}
const pause=()=>new Promise(resolve=>setTimeout(resolve,30))
const settle=async()=>{await nextTick();await pause();await nextTick()}
const button=label=>[...document.querySelectorAll('button')].find(n=>n.textContent.trim()===label)
const set=(element,value)=>{element.value=value;element.dispatchEvent(new Event(element.tagName==='SELECT'?'change':'input',{bubbles:true}))}
createApp(Notes,{providers:[],codexAccounts:[],runtimeModels:[]}).mount('#app')
async function test(){
 for(let i=0;i<30&&!document.querySelector('.quick-note-card');i++)await pause()
 assert(document.querySelectorAll('.quick-note-card').length===2,'首屏没有显示两条记录')
 button('待办事项').click();await settle();assert(document.querySelectorAll('.quick-note-card').length===2,'待办筛选失败')
 const checkbox=document.querySelector('.note-card-task input');checkbox.click();await settle()
 assert(calls.some(c=>c.action==='task-update'&&c.p.status==='completed'),'勾选待办没有保存')
 button('全部记录').click();await settle();button('新建便签').click();await settle()
 const form=document.querySelector('.note-form');assert(form,'未打开便签编辑器')
 const title=[...form.querySelectorAll('input')].find(n=>n.placeholder==='留空时使用正文首行');set(title,'浏览器回归记录')
 set(form.querySelector('textarea'),'手动记录的原始内容')
 button('添加待办').click();await settle();set(form.querySelector('.note-task input'),'检查速记功能')
 button('添加提醒').click();await settle()
 assert(form.querySelector('input[type=datetime-local]'),'没有具体提醒时间输入')
 form.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await settle()
 const saved=calls.find(c=>c.action==='save'&&c.p.title==='浏览器回归记录')
 assert(saved?.p.tasks[0].title==='检查速记功能','待办内容没有保存')
 assert(saved.p.reminders[0].nextFireAt>Date.now(),'提醒时间没有转换成未来时间点')
 const quick=document.querySelector('.notes-quick-input');set(quick,'明天下午三点提醒我回归测试');await settle()
 button('速记').click();await settle();await settle()
 assert(globalThis.lastCapture.text==='明天下午三点提醒我回归测试','速记没有捕获当前输入')
 assert(!globalThis.lastCapture.model,'跟随助手时应由后台读取当前模型，不能提交过时选择')
 assert(!quick.value,'速记保存成功后没有清理相同草稿')
 set(document.querySelector('[aria-label=速记模型模式]'),'true');await settle()
 set(quick,'使用本地模型整理');await settle();button('速记').click();set(quick,'保存过程中继续输入');await settle();await settle()
 assert(globalThis.lastCapture.model.targetId==='local','独立速记模型没有使用本地来源')
 assert(quick.value==='保存过程中继续输入','速记不应清除用户随后输入的内容')
 set(quick,'');await settle()
 const grouping=document.querySelector('[aria-label=分组显示]');set(grouping,'groupId');await settle()
 assert(document.querySelector('.notes-group-title')?.textContent.includes('AI-Manager'),'项目分组显示失败')
 button('工作留痕').click();await settle();assert(document.querySelectorAll('.quick-note-card').length===1,'工作留痕筛选失败')
 button('全部记录').click();await settle()
 document.querySelector('.capture-feedback [aria-label=收起速记提示]')?.click();await settle()
 document.getElementById('result').textContent='PASS: 手动记录、待办、提醒编辑、本地模型速记、分类分组与工作留痕筛选'
}
test().catch(error=>{document.getElementById('result').textContent='FAIL: '+error.stack})
`
try {
  const result = await build({
    stdin: {
      contents: fixture,
      resolveDir: process.cwd(),
      sourcefile: "notes-fixture.js"
    },
    bundle: true,
    format: "iife",
    write: false,
    outdir: root,
    define: {
      __VUE_OPTIONS_API__: "true",
      __VUE_PROD_DEVTOOLS__: "false",
      __VUE_PROD_HYDRATION_MISMATCH_DETAILS__: "false"
    },
    plugins: [
      {
        name: "notes-test",
        setup(builder) {
          builder.onResolve(
            {
              filter:
                /^notes-mock$|^@\/api(?:\/request|\/modules\/quick-notes)?$/
            },
            () => ({ path: "mock", namespace: "notes-mock" })
          )
          builder.onLoad({ filter: /.*/, namespace: "notes-mock" }, () => ({
            contents: mock,
            loader: "js",
            resolveDir: process.cwd()
          }))
          builder.onResolve({ filter: /^@\// }, (args) => ({
            path:
              resolve("src", args.path.slice(2)) +
              (args.path.endsWith(".vue") || args.path.endsWith(".js")
                ? ""
                : ".js")
          }))
          builder.onLoad({ filter: /\.vue$/ }, async (args) => {
            const { descriptor, errors } = parse(
              readFileSync(args.path, "utf8"),
              { filename: args.path }
            )
            assert.deepEqual(errors, [])
            const id = `data-v-${createHash("sha1").update(args.path).digest("hex").slice(0, 8)}`
            const script = compileScript(descriptor, {
              id,
              inlineTemplate: true,
              genDefaultAs: "component"
            })
            for (const style of descriptor.styles) {
              const compiled = await compileStyleAsync({
                source: style.content,
                filename: args.path,
                id,
                scoped: style.scoped,
                preprocessLang: style.lang
              })
              assert.deepEqual(compiled.errors, [])
              styles.push(compiled.code)
            }
            return {
              contents: `${script.content}\ncomponent.__scopeId='${id}';export default component`,
              loader: "js",
              resolveDir: dirname(args.path)
            }
          })
        }
      }
    ]
  })
  const theme = await less.render(readFileSync("src/styles/app.less", "utf8"), {
    filename: resolve("src/styles/app.less")
  })
  await writeFile(join(root, "fixture.js"), result.outputFiles.find(file => file.path.endsWith('.js')).text)
  styles.push(...result.outputFiles.filter(file => file.path.endsWith('.css')).map(file => file.text))
  await writeFile(
    join(root, "index.html"),
    `<!doctype html><html lang="zh-CN"><meta charset="UTF-8"><style>${theme.css}\n${styles.join("\n")} html,body,#app{height:100%;margin:0} #result{position:fixed;left:5px;bottom:0;color:#17803d;font-size:10px;z-index:1000;background:white}</style><div id="app"></div><pre id="result">RUNNING</pre><script src="fixture.js"></script></html>`
  )
  await mkdir(artifact, { recursive: true })
  const { stdout } = await promisify(execFile)(
    browser,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-first-run",
      "--allow-file-access-from-files",
      `--user-data-dir=${join(root, "profile")}`,
      "--window-size=1440,1050",
      "--virtual-time-budget=12000",
      `--screenshot=${join(artifact, "workspace.png")}`,
      "--dump-dom",
      new URL(`file:///${join(root, "index.html").replaceAll("\\", "/")}`).href
    ],
    { windowsHide: true, maxBuffer: 8 * 1024 * 1024 }
  )
  await writeFile(join(artifact, "result.html"), stdout)
  assert.ok(
    stdout.includes('PASS: 手动记录'),
    stdout.match(/FAIL:[\s\S]*?<\/pre>/)?.[0] || "浏览器未完成速记回归"
  )
  console.log(
    "速记便签真实界面交互通过，截图：artifacts/quick-notes/workspace.png"
  )
} finally {
  assert.equal(dirname(root), tmpdir())
  await rm(root, { recursive: true, force: true })
}
