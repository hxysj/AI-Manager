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
  process.env.NOTES_TEST_ARTIFACT_DIR || "artifacts/quick-notes"
)
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
 if(action==='save'){const old=state.notes.find(n=>n.id===p.id);const note={...old,...p,id:old?.id||crypto.randomUUID(),revision:(old?.revision||0)+1,createdAt:old?.createdAt||Date.now(),lifecycle:p.lifecycle||'active'};if(old)Object.assign(old,note);else state.notes.push(note);if(Array.isArray(p.reminders))state.reminders=[...state.reminders.filter(r=>r.noteId!==note.id),...p.reminders.map(r=>({...r,id:r.id||crypto.randomUUID(),noteId:note.id}))];changed();return structuredClone(note)}
 if(action==='task-update'){const n=state.notes.find(n=>n.id===p.id);n.tasks.find(t=>t.id===p.taskId).status=p.status;n.revision++;changed();return structuredClone(n)}
 if(action==='archive'||action==='trash'||action==='restore'){const n=state.notes.find(n=>n.id===p.id);n.lifecycle=action==='archive'?'archived':action==='trash'?'trash':'active';n.revision++;changed();return structuredClone(n)}
 if(action==='capture'){globalThis.lastCapture=p;return {id:'capture',requestId:p.requestId,status:'running'}}
 if(action==='capture-state'){if(globalThis.deferCaptureState)await new Promise(resolve=>{globalThis.releaseCaptureState=resolve});return {id:'capture',requestId:p.requestId,status:'completed',result:structuredClone(state.notes[0])}}
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
const button=(label,scope=document)=>[...scope.querySelectorAll('button')].find(n=>n.textContent.trim()===label)
const set=(element,value)=>{element.value=value;element.dispatchEvent(new Event(element.tagName==='SELECT'?'change':'input',{bubbles:true}))}
createApp(Notes,{providers:[],codexAccounts:[],runtimeModels:[]}).mount('#app')
async function test(){
 for(let i=0;i<30&&!document.querySelector('.calendar-day');i++)await pause()
 assert(document.querySelector('.notes-nav-active').textContent.includes('日历日程')&&document.querySelectorAll('.calendar-day').length===42,'默认没有进入日历日程')
 assert(!document.querySelector('.notes-header')&&!document.querySelector('.notes-filters')&&!document.querySelector('.notes-filter-dates'),'日历页不应显示内部头部和筛选栏')
 assert(!document.querySelector('.notes-main .notes-composer'),'记录输入区不应常驻日历页面')
 const headerButtons=[...document.querySelectorAll('.calendar-agenda-actions button')]
 assert(headerButtons.findIndex(n=>n.textContent.includes('智能记录'))+1===headerButtons.findIndex(n=>n.textContent.includes('新建便签')),'智能记录没有放在新建便签旁边')
 assert(getComputedStyle(document.querySelector('.notes-capture-modal')).display==='none','智能记录弹框不应默认打开')
 button('智能记录').click();await settle()
 assert(getComputedStyle(document.querySelector('.notes-capture-modal')).display!=='none','日历中的智能记录入口不可用')
 document.querySelector('.notes-capture-modal .base-modal__close').click();await settle()
 button('全部记录').click();await settle()
 assert(document.querySelector('.notes-header')&&document.querySelector('.notes-filters')&&document.querySelector('.notes-filter-dates'),'其他记录视图应保留头部和筛选栏')
 assert(document.querySelectorAll('.quick-note-card').length===2,'首屏没有显示两条记录')
 // 已有便签的入口直接新增提醒，提交前拒绝过去、当前分钟和过期的提前提醒。
 document.querySelector('.quick-note-card [aria-label=添加提醒]').click();await settle()
 let reminderForm=document.querySelector('.note-form')
 let reminderTime=reminderForm.querySelector('.note-reminder input[type=datetime-local]')
 assert(reminderTime&&new Date(reminderTime.value).getTime()>Date.now(),'卡片提醒入口没有预填未来时间')
 assert(document.activeElement===reminderTime,'提醒入口没有定位到时间输入')
 assert(reminderTime.min&&new Date(reminderTime.min).getTime()>Date.now(),'时间选择器没有设置未来时间下限')
 const localTime=value=>new Date(value-new Date(value).getTimezoneOffset()*60000).toISOString().slice(0,16)
 const saveCount=()=>calls.filter(c=>c.action==='save').length
 const beforeInvalid=saveCount()
 for(const at of [Date.now()-600000,Date.now()]){
  set(reminderTime,localTime(at));reminderForm.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await settle()
  assert(document.querySelector('.note-error')?.textContent.includes('必须晚于当前时间'),'过去或当前时间没有提示')
  assert(saveCount()===beforeInvalid,'无效提醒不应调用保存接口')
 }
 set(reminderTime,localTime(Date.now()+1800000));reminderForm.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await settle()
 assert(saveCount()===beforeInvalid+1&&!document.querySelector('.note-form'),'已有便签没有保存未来提醒')
 document.querySelector('.quick-note-card [aria-label=管理提醒]').click();await settle()
 reminderForm=document.querySelector('.note-form');reminderTime=reminderForm.querySelector('.note-reminder input[type=datetime-local]')
 assert(reminderForm.querySelectorAll('.note-reminder').length===1,'管理已有提醒不应重复添加')
 set(reminderForm.querySelector('.note-task input[type=datetime-local]'),localTime(Date.now()+120000));await settle()
 const taskLink=[...reminderForm.querySelectorAll('.note-reminder select')].find(n=>[...n.options].some(o=>o.value==='t'));set(taskLink,'t')
 const offsetSelect=[...reminderForm.querySelectorAll('.note-reminder select')].find(n=>[...n.options].some(o=>o.textContent.includes('独立指定时间')));await settle();set(offsetSelect,'5');await settle()
 const beforeOffset=saveCount();reminderForm.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await settle()
 assert(saveCount()===beforeOffset&&document.querySelector('.note-error')?.textContent.includes('必须晚于当前时间'),'提前提醒落在过去没有阻止保存')
 document.querySelector('.quick-note-editor .base-modal__close').click();await settle()
 button('待办事项').click();await settle();assert(document.querySelectorAll('.quick-note-card').length===2,'待办筛选失败')
 const checkbox=document.querySelector('.note-card-task input');checkbox.click();await settle()
 assert(calls.some(c=>c.action==='task-update'&&c.p.status==='completed'),'勾选待办没有保存')
 button('全部记录').click();await settle();button('新建便签').click();await settle()
 const form=document.querySelector('.note-form');assert(form,'未打开便签编辑器')
 const title=[...form.querySelectorAll('input')].find(n=>n.placeholder==='留空时使用正文首行');set(title,'浏览器回归记录')
 set(form.querySelector('textarea'),'手动记录的原始内容')
 button('添加待办').click();await settle();set(form.querySelector('.note-task input'),'检查速记功能')
 button('添加提醒',form).click();await settle()
 assert(form.querySelector('input[type=datetime-local]'),'没有具体提醒时间输入')
 form.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await settle()
 const saved=calls.find(c=>c.action==='save'&&c.p.title==='浏览器回归记录')
 assert(saved?.p.tasks[0].title==='检查速记功能','待办内容没有保存')
 assert(saved.p.reminders[0].nextFireAt>Date.now(),'提醒时间没有转换成未来时间点')
 button('智能记录').click();await settle()
 const captureModal=document.querySelector('.notes-capture-modal')
 const quick=captureModal.querySelector('.notes-quick-input')
 assert(getComputedStyle(captureModal).display!=='none'&&document.activeElement===quick,'智能记录弹框没有打开并聚焦输入')
 set(quick,'关闭后保留的草稿');await settle();captureModal.querySelector('.base-modal__close').click();await settle()
 assert(getComputedStyle(captureModal).display==='none','关闭按钮没有收起智能记录')
 button('智能记录').click();await settle();assert(quick.value==='关闭后保留的草稿','重开弹框丢失了草稿')
 quick.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));await settle()
 assert(getComputedStyle(captureModal).display==='none','Esc 没有关闭弹框')
 button('智能记录').click();await settle();set(quick,'明天下午三点提醒我回归测试');await settle()
 button('速记').click();await settle();await settle()
 assert(globalThis.lastCapture.text==='明天下午三点提醒我回归测试','速记没有捕获当前输入')
 assert(!globalThis.lastCapture.model,'跟随助手时应由后台读取当前模型，不能提交过时选择')
 assert(!quick.value,'速记保存成功后没有清理相同草稿')
 set(document.querySelector('[aria-label=速记模型模式]'),'true');await settle()
 set(quick,'使用本地模型整理');await settle();button('速记').click();set(quick,'保存过程中继续输入');await settle();await settle()
 assert(globalThis.lastCapture.model.targetId==='local','独立速记模型没有使用本地来源')
 assert(quick.value==='保存过程中继续输入','速记不应清除用户随后输入的内容')
 // 弹框隐藏时保留整理组件，让后台回包仍能清理对应草稿并显示结果。
 globalThis.deferCaptureState=true;set(quick,'关闭弹框后继续整理');await settle();button('速记').click();await settle()
 assert(typeof globalThis.releaseCaptureState==='function','没有进入后台整理')
 captureModal.querySelector('.base-modal__close').click();await settle()
 globalThis.deferCaptureState=false;globalThis.releaseCaptureState();await settle();await settle()
 assert(quick.value===''&&!calls.some(c=>c.action==='capture-cancel'),'关闭弹框取消了任务或丢失整理回包')
 button('智能记录').click();await settle();set(quick,'直接记录弹框中的原始内容');await settle()
 button('直接记录',captureModal).click();await settle()
 assert(getComputedStyle(captureModal).display==='none'&&document.querySelector('.note-form textarea').value==='直接记录弹框中的原始内容','直接记录没有带入草稿并进入编辑器')
 document.querySelector('.quick-note-editor .base-modal__close').click();await settle()
 button('智能记录').click();await settle();assert(quick.value==='直接记录弹框中的原始内容','取消手动编辑后草稿丢失')
 set(quick,'');captureModal.querySelector('.base-modal__close').click();await settle()
 const grouping=document.querySelector('[aria-label=分组显示]');set(grouping,'groupId');await settle()
 assert(document.querySelector('.notes-group-title')?.textContent.includes('AI-Manager'),'项目分组显示失败')
 button('工作留痕').click();await settle();assert(document.querySelectorAll('.quick-note-card').length===1,'工作留痕筛选失败')
 button('全部记录').click();await settle()
 document.querySelector('.capture-feedback [aria-label=收起速记提示]')?.click();await settle()
 // 日历沿用已有记录与编辑器，分别验证事项日期、任务截止日和实际提醒时间。
 state.reminders.push({id:'calendar-reminder',noteId:'work',enabled:1,nextFireAt:new Date(2026,10,20,15).getTime(),schedule:{repeat:'once'}},{id:'disabled-reminder',noteId:'work',enabled:0,nextFireAt:new Date(2026,10,21,15).getTime(),schedule:{repeat:'once'}})
 state.notes.find(n=>n.id==='work').tasks.push({id:'midnight',title:'午夜交付检查',status:'doing',dueAt:new Date(2026,10,22,0,10).getTime()},{id:'cancelled',title:'已取消事项',status:'cancelled',dueDate:'2026-11-21'})
 button('日历日程').click();await settle()
 assert(document.querySelectorAll('.calendar-day').length===42,'月历应按周一开头显示六周')
 const period=(year,month)=>{set(document.querySelector('[aria-label=日历年份]'),String(year));set(document.querySelector('[aria-label=日历月份]'),String(month-1))}
 const day=date=>document.querySelector('.calendar-day[data-date="'+date+'"]')
 period(2026,11);await settle();day('2026-11-18').click();await settle()
 assert(day('2026-11-18').querySelector('.calendar-lunar').textContent==='初十','农历日期错误')
 assert(day('2026-11-18').getAttribute('aria-pressed')==='true','没有选中日期')
 assert(document.querySelector('.calendar-agenda').textContent.includes('完成速记模块的存储设计'),'事项日期没有关联便签')
 const entry=[...document.querySelectorAll('.calendar-entry')].find(n=>n.textContent.includes('完成速记模块的存储设计'))
 entry.click();await settle();assert(document.querySelector('.note-form'),'日历事项没有打开现有编辑器')
 document.querySelector('.quick-note-editor .base-modal__close').click();await settle()
 day('2026-11-19').click();await settle()
 assert(document.querySelector('.calendar-agenda').textContent.includes('验证重启后的提醒恢复'),'截止日没有显示待办')
 document.querySelector('[aria-label=在这天新建便签]').click();await settle()
 assert(document.querySelector('.note-form input[type=date]').value==='2026-11-19','日历新建没有带入所选日期')
 set(document.querySelector('.note-form textarea'),'在日历中安排的事情')
 document.querySelector('.note-form').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await settle()
 assert(calls.some(c=>c.action==='save'&&c.p.occurredOn==='2026-11-19'&&c.p.content==='在日历中安排的事情'),'日历新建日期没有保存')
 assert(document.querySelector('.calendar-agenda').textContent.includes('在日历中安排的事情'),'新建后没有刷新当天日程')
 day('2026-11-20').click();await settle()
 assert(document.querySelector('.calendar-agenda').textContent.includes('提醒')&&document.querySelector('.calendar-entry-time').textContent==='15:00','提醒时间没有按本地时间显示')
 day('2026-11-21').click();await settle()
 assert(!document.querySelector('.calendar-entry'),'停用提醒不应进入日历')
 assert(document.querySelector('.calendar-agenda-empty'),'无事项日期没有显示空状态')
 day('2026-11-22').click();await settle()
 assert(document.querySelector('.calendar-entry-time').textContent==='00:10'&&document.querySelector('.calendar-entry').textContent.includes('午夜交付检查'),'跨午夜的截止时间没有按本地日期归属')
 period(2026,1);await settle();day('2026-01-31').click();document.querySelector('[aria-label=下个月]').click();await settle()
 assert(day('2026-02-28').getAttribute('aria-pressed')==='true','非闰年月末切换日期错误')
 period(2028,1);await settle();day('2028-01-31').click();document.querySelector('[aria-label=下个月]').click();await settle()
 assert(day('2028-02-29').getAttribute('aria-pressed')==='true','闰年月末切换日期错误')
 period(2026,12);await settle();day('2026-12-31').click();document.querySelector('[aria-label=下个月]').click();await settle()
 assert(document.querySelector('[aria-label=日历年份]').value==='2027'&&day('2027-01-31').getAttribute('aria-pressed')==='true','跨年切换错误')
 document.querySelector('[aria-label=上个月]').click();await settle();assert(day('2026-12-31').getAttribute('aria-pressed')==='true','上个月切换错误')
 period(2026,11);await settle();day('2026-10-26').click();await settle()
 assert(document.querySelector('[aria-label=日历月份]').value==='9','相邻月份日期点击后没有切换')
 button('今天').click();await settle()
 assert(day(new Date().toLocaleDateString('sv-SE')).getAttribute('aria-current')==='date','回到今天失败')
 button('全部记录').click();await settle()
 assert(document.querySelectorAll('.quick-note-card').length===4,'退出日历后便签视图异常')
 // 列表筛选继续生效，切回日历后不应被不可见的筛选条件漏掉事项。
 const range=document.querySelectorAll('.notes-filter-dates input[type=date]')
 set(document.querySelector('.notes-search input'),'不匹配的列表筛选')
 set(document.querySelector('[aria-label=分类筛选]'),'debugging')
 set(range[0],'2099-01-01');set(range[1],'2099-12-31')
 set(document.querySelector('[aria-label=记录类型]'),'worklog');await settle()
 assert(!document.querySelector('.quick-note-card'),'列表中的筛选没有生效')
 button('日历日程').click();await settle();period(2026,11);await settle();day('2026-11-18').click();await settle()
 const calendarQuery=calls.findLast(c=>c.action==='list').p
 assert(calendarQuery.search===''&&calendarQuery.categoryId==='','日历请求不应沿用隐藏的搜索和分类')
 assert(document.querySelector('.calendar-agenda').textContent.includes('完成速记模块的存储设计'),'列表筛选影响了日历事项')
 assert(!document.querySelector('.notes-header')&&!document.querySelector('.notes-filters')&&!document.querySelector('.notes-filter-dates'),'切换后日历重新出现了头部')
 button('全部记录').click();await settle()
 assert(!document.querySelector('.quick-note-card'),'返回列表后丢失了列表筛选')
 set(document.querySelector('.notes-search input'),'');set(document.querySelector('[aria-label=分类筛选]'),'')
 const clearRange=document.querySelectorAll('.notes-filter-dates input[type=date]')
 set(clearRange[0],'');set(clearRange[1],'');set(document.querySelector('[aria-label=记录类型]'),'');await settle()
 assert(document.querySelectorAll('.quick-note-card').length===4,'清除列表筛选后记录没有恢复')
 button('日历日程').click();await settle();period(2026,11);await settle();day('2026-11-18').click();await settle()
 assert(![...document.querySelectorAll('.calendar-day')].some(n=>n.scrollWidth>n.clientWidth),'日期格出现横向溢出')
 document.getElementById('result').textContent='PASS: 手动记录、待办、提醒编辑、本地模型速记、分类分组与工作留痕筛选；日历隐藏头部与筛选、保留记录入口、隔离列表筛选；默认日历、智能记录弹框、草稿保留、后台整理；日历月份切换、农历、日期归属、编辑、新建、闰年与跨年'
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
    stdout.includes("PASS: 手动记录"),
    stdout.match(/FAIL:[\s\S]*?<\/pre>/)?.[0] || "浏览器未完成速记回归"
  )
  console.log(
    `速记便签与日历真实界面交互通过，截图：${join(artifact, "workspace.png")}`
  )
} finally {
  assert.equal(dirname(root), tmpdir())
  await rm(root, { recursive: true, force: true })
}
