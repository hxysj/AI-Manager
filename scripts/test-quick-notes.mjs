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
const artifact = resolve(
  process.env.NOTES_TEST_ARTIFACT_DIR || join(root, "results")
)
const styles = []
const mock = `
import {ref} from 'vue'
export const systemApi={openExternal:async()=>{}}
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
 if(action==='detail')return {...structuredClone(state.notes.find(n=>n.id===p.id)),reminders:structuredClone(state.reminders.filter(r=>r.noteId===p.id)),history:[{id:'rev',operation:'create',actor:'user',createdAt:time,after:{content:'原始内容'}}],captures:[]}
 if(action==='save'){const old=state.notes.find(n=>n.id===p.id);const note={...old,...p,id:old?.id||crypto.randomUUID(),revision:(old?.revision||0)+1,createdAt:old?.createdAt||Date.now(),lifecycle:p.lifecycle||'active'};if(old)Object.assign(old,note);else state.notes.push(note);if(Array.isArray(p.reminders))state.reminders=[...state.reminders.filter(r=>r.noteId!==note.id),...p.reminders.map(r=>({...r,id:r.id||crypto.randomUUID(),noteId:note.id}))];changed();return structuredClone(note)}
 if(action==='task-update'){const n=state.notes.find(n=>n.id===p.id);n.tasks.find(t=>t.id===p.taskId).status=p.status;n.revision++;changed();return structuredClone(n)}
 if(action==='archive'||action==='trash'||action==='restore'){const n=state.notes.find(n=>n.id===p.id);n.lifecycle=action==='archive'?'archived':action==='trash'?'trash':'active';n.revision++;changed();return structuredClone(n)}
 if(action==='capture'){globalThis.lastCapture=p;return {id:'capture',requestId:p.requestId,status:'running'}}
 if(action==='capture-save'){const n={...p.note,id:crypto.randomUUID(),revision:1,createdAt:Date.now(),updatedAt:Date.now(),groupId:p.note.groupId||'inbox',categoryId:p.note.categoryId||'uncategorized',lifecycle:'active'};state.notes.push(n);changed();return structuredClone(n)}
 if(action==='capture-state'){if(globalThis.deferCaptureState)await new Promise(resolve=>{globalThis.releaseCaptureState=resolve});return {id:'capture',requestId:p.requestId,status:'preview',result:{title:'修复分值同步问题',content:'修复登录页与预约页分值不同步的问题，补充回归验证。',type:'todo',groupId:'project',categoryId:'debugging',tags:['分值同步','回归测试'],tasks:[{id:'ai-task',title:'补充回归测试',status:'pending',dueDate:null}],worklog:{},reminders:[]}}}
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
import {calls,state} from 'notes-mock'
const assert=(value,message)=>{if(!value)throw Error(message)}
const pause=()=>new Promise(resolve=>setTimeout(resolve,40))
const settle=async()=>{await nextTick();await pause();await nextTick()}
const button=(label,scope=document)=>[...scope.querySelectorAll('button')].find(n=>n.getClientRects().length&&n.textContent.trim()===label)
const set=(element,value)=>{assert(element,'找不到输入控件');element.value=value;element.dispatchEvent(new Event(element.tagName==='SELECT'?'change':'input',{bubbles:true}))}
const local=value=>new Date(value-new Date(value).getTimezoneOffset()*60000).toISOString().slice(0,16)
const day=date=>document.querySelector('.calendar-day[data-date="'+date+'"]')
const period=(year,month)=>{set(document.querySelector('[aria-label=日历年份]'),String(year));set(document.querySelector('[aria-label=日历月份]'),String(month-1))}
const workspace=createApp(Notes,{providers:[],codexAccounts:[],runtimeModels:[]}).mount('#app')
async function test(){
 for(let i=0;i<30&&!document.querySelector('.calendar-day');i++)await pause()
 assert(document.querySelectorAll('.calendar-day').length===42,'默认月视图没有显示六周')
 assert(!document.querySelector('.notes-header')&&!document.querySelector('.notes-filters'),'日历不应有列表头部')
 assert(document.querySelectorAll('.notes-nav-section').length===3,'侧栏没有按工作区、提醒、归档分组')
 const calendar=document.querySelector('.note-calendar').getBoundingClientRect()
 assert(Math.abs(calendar.bottom-document.querySelector('.quick-notes').getBoundingClientRect().bottom)<20,'日历未填满可用高度')
 button('周视图').click();await settle();assert(document.querySelectorAll('.calendar-day').length===7,'周视图没有显示七天')
 button('列表').click();await settle();assert(document.querySelector('.calendar-list'),'没有切换到日程列表')
 button('月视图').click();await settle();period(2026,11);await settle();day('2026-11-18').click();await settle()
 assert(day('2026-11-18').querySelector('.calendar-lunar').textContent==='初十','农历日期错误')
 day('2026-11-19').click();await settle();assert(document.querySelector('.calendar-agenda').textContent.includes('验证重启后的提醒恢复'),'截止日没有关联待办')
 document.querySelector('[aria-label=在这天新建便签]').click();await settle()
 let form=document.querySelector('.note-form')
 assert(!document.querySelector('.note-editor-inline')&&form,'新建没有打开右侧抽屉')
 assert(form.querySelector('input[type=date]').value==='2026-11-19','新建没有带入日历日期')
 set(form.querySelector('.editor-title-field input'),'界面回归记录')
 set(form.querySelector('.body-input'),'记录新的想法')
 const body=form.querySelector('.body-input');body.focus();body.setSelectionRange(0,2);form.querySelector('[aria-label=加粗]').click();await settle()
 assert(body.value.startsWith('**记录**'),'正文工具栏没有操作选区')
 button('添加待办',form).click();await settle();set(form.querySelector('.note-task input'),'检查新建抽屉')
 button('添加提醒',form).click();await settle()
 const time=form.querySelector('.note-reminder input[type=datetime-local]')
 assert(time&&new Date(time.value).getTime()>Date.now(),'没有预填未来提醒时间')
 const before=calls.filter(c=>c.action==='save').length
 set(time,local(Date.now()-600000));form.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await settle()
 assert(calls.filter(c=>c.action==='save').length===before&&document.querySelector('.note-error').textContent.includes('必须晚于当前时间'),'过去提醒没有被拒绝')
 set(time,local(Date.now()+3600000));form.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await settle()
 assert(!document.querySelector('.note-form'),'抽屉保存后没有关闭')
 const saved=calls.find(c=>c.action==='save'&&c.p.title==='界面回归记录');assert(saved?.p.occurredOn==='2026-11-19'&&saved.p.tasks[0].title==='检查新建抽屉','保存缺少日期或待办')
 button('全部记录').click();await settle();assert(document.querySelectorAll('.quick-note-card').length===3,'全部记录缺少卡片')
 assert(document.querySelector('.notes-header')&&document.querySelector('.notes-filters'),'列表视图缺少标题筛选')
 document.querySelector('.quick-note-card .note-card-open').click();await settle()
 assert(document.querySelector('.note-editor-inline')&&document.querySelector('.note-editor-aside'),'详情未显示双栏页面')
 assert(document.querySelector('.editor-history').textContent.includes('创建了便签'),'详情没有读取操作记录')
 // AI 建议应用到编辑草稿，保留已有任务与目录，不提前保存。
 const detailTasks=document.querySelectorAll('.note-editor-inline .note-task').length
 const detailSaveCount=calls.filter(c=>c.action==='save').length
 button('分析这条便签').click();await settle()
 const analysisModal=[...document.querySelectorAll('.smart-record-modal')].find(n=>n.getClientRects().length)
 assert(analysisModal.querySelector('.base-modal__panel').getBoundingClientRect().width>innerWidth*0.7,'详情中的智能分析被抽屉样式影响')
 button('分析并预览',analysisModal).click();await settle();await settle();button('应用到便签',analysisModal).click();await settle()
 assert(document.querySelectorAll('.note-editor-inline .note-task').length===detailTasks+1,'应用 AI 建议覆盖了已有待办')
 assert(calls.filter(c=>c.action==='save').length===detailSaveCount,'应用草稿时提前保存了修改')
 set(document.querySelector('.editor-title-field input'),'更新后的标题');button('保存修改').click();await settle()
 assert(calls.some(c=>c.action==='save'&&c.p.title==='更新后的标题'),'详情编辑没有保存')
 document.querySelector('.quick-note-card [aria-label=管理提醒]')?.click();await settle()
 if(document.querySelector('.note-form')){button('返回列表').click();await settle()}
 // 分析先生成可编辑预览，确认前不能写入正式便签。
 button('智能记录').click();await settle()
 const modal=[...document.querySelectorAll('.smart-record-modal')].find(n=>n.getClientRects().length)
 let source=modal.querySelector('.smart-input');set(source,'修复登录页与预约页分值不同步的问题，补充回归测试。');await settle()
 const count=state.notes.length
 button('分析并预览',modal).click();await settle();await settle()
 assert(globalThis.lastCapture.preview===true,'智能记录没有请求预览')
 assert(state.notes.length===count&&!calls.some(c=>c.action==='capture-save'),'预览前提前保存了便签')
 assert(modal.querySelector('.smart-preview-title').textContent==='修复分值同步问题','没有渲染模型实际返回结果')
 const suggestions=modal.querySelectorAll('.smart-task').length
 button('添加',modal).click();await settle();assert(modal.querySelectorAll('.smart-task').length===suggestions+1,'预览没有新增待办')
 const newSuggestion=[...modal.querySelectorAll('.smart-task')].at(-1);set(newSuggestion.querySelector('input'),'补充检查');newSuggestion.querySelector('[aria-label=移除建议待办]').click();await settle()
 assert(modal.querySelectorAll('.smart-task').length===suggestions,'预览没有移除待办')
 set(modal.querySelector('.smart-field input'),'确认后的 AI 标题');button('确认并保存',modal).click();await settle()
 assert(calls.some(c=>c.action==='capture-save'&&c.p.note.title==='确认后的 AI 标题'),'预览修改没有提交')
 assert(state.notes.length===count+1,'确认后没有生成便签')
 button('智能记录').click();await settle();source=modal.querySelector('.smart-input');set(source,'关闭后保留的草稿');await settle();modal.querySelector('[aria-label=关闭智能记录]').click();await settle()
 button('智能记录').click();await settle();assert(source.value==='关闭后保留的草稿','关闭后丢失原文草稿')
 button('仅保存原文',modal).click();await settle();assert(calls.some(c=>c.action==='save'&&c.p.content==='关闭后保留的草稿'),'仅保存原文没有调用保存')
 // 新建草稿可以保存到本机并恢复，不会作为正式便签提交。
 button('新建便签').click();await settle();set(document.querySelector('.body-input'),'本机草稿内容');button('存为草稿').click();await settle()
 assert(JSON.parse(localStorage.getItem('quick-notes:new-draft')).content==='本机草稿内容','草稿没有存入本机')
 document.querySelector('[aria-label=关闭便签编辑器]').click();await settle();button('新建便签').click();await settle()
 assert(document.querySelector('.body-input').value==='本机草稿内容','未恢复本机草稿');document.querySelector('[aria-label=关闭便签编辑器]').click();localStorage.removeItem('quick-notes:new-draft');await settle()
 button('日历日程').click();await settle();period(2028,1);await settle();day('2028-01-31').click();document.querySelector('[aria-label=下个月]').click();await settle();assert(day('2028-02-29').getAttribute('aria-pressed')==='true','闰年月末切换错误')
 period(2026,12);await settle();day('2026-12-31').click();document.querySelector('[aria-label=下个月]').click();await settle();assert(day('2027-01-31').getAttribute('aria-pressed')==='true','跨年切换错误')
 period(2026,11);await settle();day('2026-11-18').click();await settle()
 await workspace.focusSearch();await settle()
 assert(document.activeElement===document.querySelector('.notes-search input'),'顶部搜索入口没有聚焦列表搜索')
 workspace.toggleFilters();await settle();assert(!document.querySelector('.notes-filters'),'顶部筛选入口没有收起筛选')
 workspace.toggleFilters();await settle();assert(document.querySelector('.notes-filters'),'顶部筛选入口没有恢复筛选')
 workspace.openOrganizer();await settle();assert(document.querySelector('.notes-organizer'),'顶部目录入口没有打开目录管理')
 document.querySelector('.notes-organizer').closest('.base-modal').querySelector('.base-modal__close').click();await settle()
 await workspace.refresh();await settle();button('日历日程').click();await settle()
 const scene=__NOTES_SCENE__
 if(scene==='list'){button('全部记录').click();await settle()}
 if(scene==='detail'){document.querySelector('.calendar-entry').click();await settle()}
 if(scene==='drawer'){document.querySelector('[aria-label=在这天新建便签]').click();await settle()}
 if(scene==='smart'){button('智能记录').click();await settle();set(modal.querySelector('.smart-input'),'修复登录页与预约页分值不同步的问题，补充回归验证。');await settle();button('分析并预览',modal).click();await settle();await settle()}
 assert(document.querySelector('.quick-notes').scrollWidth<=document.querySelector('.quick-notes').clientWidth,'便签工作区横向溢出')
 document.getElementById('result').textContent='PASS: 便签布局重构，侧栏分组、全高日历、月周列表、农历、抽屉新建、正文工具栏、草稿恢复、双栏详情、提醒校验、AI 预览确认与原文保存'
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
      __NOTES_SCENE__: JSON.stringify(
        process.env.NOTES_TEST_SCENE || "calendar"
      ),
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
  await writeFile(
    join(root, "fixture.js"),
    result.outputFiles.find((file) => file.path.endsWith(".js")).text
  )
  styles.push(
    ...result.outputFiles
      .filter((file) => file.path.endsWith(".css"))
      .map((file) => file.text)
  )
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
      `--window-size=${process.env.NOTES_TEST_WINDOW || "1440,1050"}`,
      "--virtual-time-budget=12000",
      `--screenshot=${join(artifact, "workspace.png")}`,
      "--dump-dom",
      new URL(`file:///${join(root, "index.html").replaceAll("\\", "/")}`).href
    ],
    { windowsHide: true, maxBuffer: 8 * 1024 * 1024 }
  )
  await writeFile(join(artifact, "result.html"), stdout)
  assert.ok(
    stdout.includes("PASS: 便签布局重构"),
    stdout.match(/FAIL:[\s\S]*?<\/pre>/)?.[0] || "浏览器未完成速记回归"
  )
  console.log(
    `速记便签与日历真实界面交互通过，截图：${join(artifact, "workspace.png")}`
  )
} finally {
  assert.equal(dirname(root), tmpdir())
  await rm(root, { recursive: true, force: true })
}
