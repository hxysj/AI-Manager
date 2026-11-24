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
export const openHandlers=new Set()
const time=Date.now()
const historyByNote=new Map()
export const state={notes:[
 {id:'work',title:'完成速记模块的存储设计',content:'整理了便签、待办和提醒的数据关系，下一步验证重启恢复。',type:'worklog',groupId:'project',categoryId:'development',tags:['AI-Manager','本地存储'],color:'mint',pinned:true,occurredOn:'2026-11-18',createdAt:time,updatedAt:time,revision:1,lifecycle:'active',tasks:[{id:'t',title:'验证重启后的提醒恢复',status:'pending',dueDate:'2026-11-19'}],worklog:{result:'完成存储设计'}},
 {id:'idea',title:'周五的发布检查',content:'检查 Claude 和 Codex 的切换，再整理这周的工作记录。',type:'todo',groupId:'inbox',categoryId:'debugging',tags:['发布'],color:'sand',createdAt:time,updatedAt:time,revision:1,lifecycle:'active',tasks:[{id:'t2',title:'检查发布包',status:'pending'}],worklog:{}}
],folders:[{id:'inbox',kind:'group',name:'收件箱',aliases:[]},{id:'project',kind:'group',name:'AI-Manager',aliases:['桌宠']},{id:'uncategorized',kind:'category',name:'未分类',aliases:[]},{id:'development',kind:'category',name:'开发实现',aliases:[]},{id:'debugging',kind:'category',name:'问题排查',aliases:[]}],events:[],reminders:[],captures:[],hasMore:false,
calendarRules:[{id:'weekend',revision:1,title:'周末双休',kind:'rest',calendar:'solar',repeat:'weekly',weekdays:[5,6],monthDays:[],workStatus:'rest',color:'#579b80',dateParity:'all',enabled:true,priority:0}]}
export const changed=()=>{for(const handle of handlers)handle({})}
export const quickNotesApi={onChanged:async fn=>{handlers.add(fn);return()=>handlers.delete(fn)},onOpen:async fn=>{openHandlers.add(fn);return()=>openHandlers.delete(fn)},invoke:async(action,p={})=>{
 p=JSON.parse(JSON.stringify(p))
 calls.push({action,p:JSON.parse(JSON.stringify(p))})
 if(action==='calendar-entries')return structuredClone({
  notes:state.notes.filter(note=>note.lifecycle==='active'),
  reminders:state.reminders,
  calendarRules:state.calendarRules
 })
 if(action==='calendar-rule-save'){
  const old=state.calendarRules.find(rule=>rule.id===p.id)
  if(old&&p.revision!==old.revision)throw Error('测试日历修订号不匹配')
  const saved={calendar:'solar',repeat:'once',enabled:true,priority:0,...old,...p,id:old?.id||crypto.randomUUID(),revision:(old?.revision||0)+1,updatedAt:Date.now()}
  if(old)Object.assign(old,saved);else state.calendarRules.push(saved)
  changed()
  return structuredClone(saved)
 }
 if(action==='calendar-rule-delete'){
  state.calendarRules=state.calendarRules.filter(rule=>rule.id!==p.id||rule.revision!==p.revision)
  changed()
  return {id:p.id,deleted:true}
 }
 if(action==='options')return {model:{targetId:'local',model:'qwen3.5-0.8b',protocol:'chat'}}
 if(action==='list'){
  const offset=p.offset||0
  const limit=Math.min(p.limit||300,globalThis.notesPageSize||1000)
  const notes=state.notes.filter(n=>
   n.lifecycle===(p.lifecycle||'active')&&(!p.groupId||n.groupId===p.groupId)&&
   (!p.categoryId||n.categoryId===p.categoryId)&&(!p.search||JSON.stringify(n).includes(p.search))&&
   (!p.type||n.type===p.type)&&(!p.workStatus||(n.type==='worklog'&&(n.workTracking?.status||'untracked')===p.workStatus))
  ).slice(offset,offset+limit)
  return structuredClone({
   ...state,notes,hasMore:notes.length===limit,
   captures:state.captures.filter(c=>c.status!=='completed')
  })
 }
 if(action==='detail')return {...structuredClone(state.notes.find(n=>n.id===p.id)),reminders:structuredClone(state.reminders.filter(r=>r.noteId===p.id)),history:[...(historyByNote.get(p.id)||[]),{id:'rev',operation:'create',actor:'user',createdAt:time,after:{content:'原始内容'}}],captures:[]}
 if(action==='save'){
  const old=state.notes.find(n=>n.id===p.id)
  const {progressNote,appendReminder,...fields}=p
  const note={...old,...fields,id:old?.id||crypto.randomUUID(),revision:(old?.revision||0)+1,createdAt:old?.createdAt||Date.now(),lifecycle:p.lifecycle||'active'}
  if(old&&p.revision!==old.revision)throw Error('测试便签修订号不匹配')
  if(old&&p.workTracking&&(progressNote||JSON.stringify(old.workTracking)!==JSON.stringify(p.workTracking))){
   const entry={id:crypto.randomUUID(),operation:'progress',actor:'user',createdAt:Date.now(),before:structuredClone(old),after:{...note,progressNote}}
   historyByNote.set(note.id,[entry,...(historyByNote.get(note.id)||[])])
  }
  if(old)Object.assign(old,note);else state.notes.push(note)
  if(appendReminder)state.reminders.push({id:crypto.randomUUID(),noteId:note.id,enabled:true,nextFireAt:appendReminder.at,schedule:{...appendReminder,repeat:'once'}})
  if(Array.isArray(p.reminders))state.reminders=[...state.reminders.filter(r=>r.noteId!==note.id),...p.reminders.map(r=>({...r,id:r.id||crypto.randomUUID(),noteId:note.id}))]
  changed()
  return structuredClone(note)
 }
 if(action==='task-update'){const n=state.notes.find(n=>n.id===p.id);n.tasks.find(t=>t.id===p.taskId).status=p.status;n.revision++;changed();return structuredClone(n)}
 if(action==='archive'||action==='trash'||action==='restore'){const n=state.notes.find(n=>n.id===p.id);n.lifecycle=action==='archive'?'archived':action==='trash'?'trash':'active';n.revision++;changed();return structuredClone(n)}
 // 小结任务先落盘，再通过变更事件通知已打开的便签视图。
 if(action==='summary'){
  if(globalThis.deferSummary)await new Promise(resolve=>{globalThis.releaseSummary=resolve})
  const capture={id:crypto.randomUUID(),status:'running',source:{type:'summary',noteIds:p.ids},sourceText:'内部小结输入'}
  state.captures.unshift(capture)
  changed()
  return structuredClone(capture)
 }
 if(action==='capture-cancel'){
  const capture=state.captures.find(c=>c.id===p.requestId)
  Object.assign(capture,{status:'cancelled',error:'已取消整理，原文已保留'})
  changed()
  return structuredClone(capture)
 }
 if(action==='capture-discard'){
  const capture=state.captures.find(c=>c.id===p.id)
  if(capture&&capture.status!=='running')capture.status='dismissed'
  changed()
  return {ok:true}
 }
 if(action==='capture'){globalThis.lastCapture=p;return {id:'capture',requestId:p.requestId,status:'running'}}
 if(action==='capture-save'){const n={...p.note,id:crypto.randomUUID(),revision:1,createdAt:Date.now(),updatedAt:Date.now(),groupId:p.note.groupId||'inbox',categoryId:p.note.categoryId||'uncategorized',lifecycle:'active'};state.notes.push(n);state.reminders.push(...(p.note.reminders||[]).map(r=>({...r,id:r.id||crypto.randomUUID(),noteId:n.id})));changed();return structuredClone(n)}
 if(action==='capture-state'){if(globalThis.deferCaptureState)await new Promise(resolve=>{globalThis.releaseCaptureState=resolve});return {id:'capture',requestId:p.requestId,status:'preview',result:{title:'修复分值同步问题',content:'修复登录页与预约页分值不同步的问题，补充回归验证。',type:'worklog',groupId:'project',categoryId:'debugging',tags:['分值同步','回归测试'],tasks:[{id:'ai-task',title:'补充回归测试',status:'pending',dueDate:null}],worklog:{},workTracking:{status:'planned',progress:null},reminders:globalThis.aiReminderAt?[{taskId:'ai-task',enabled:true,nextFireAt:globalThis.aiReminderAt,schedule:{at:globalThis.aiReminderAt,repeat:'once'}}]:[]}}}
 if(action==='organize'){state.folders.push({...p,id:crypto.randomUUID()});changed();return {id:'folder'}}
 return {ok:true}
}}
export const settingsApi={saveSettings:async()=>{throw Error('速记模型选择不应改写助手配置')}}
export const request=async(channel,p)=>{
 if(channel==='usage:stats'){
  calls.push({action:channel,p})
  const at=new Date(2026,10,19,12).getTime()
  return {status:'ok',data:{trends:p.startAt<=at&&p.endAt>=at?[{date:'2026/11/19',requestCount:7,actualTokens:1200,outputTokens:350}]:[]}}
 }
 return channel==='local-models:installed'?{models:[{id:'qwen3.5-0.8b'}]}:{}
}
export const usageApi={getUsageStats:payload=>request('usage:stats',payload)}
export const subscribeReady=async()=>()=>{}
`
const fixture = `
import {createApp,h,nextTick,ref} from 'vue'
import Notes from './src/features/quick-notes/index.vue'
import Calendar from './src/features/calendar/index.vue'
import {calls,state,openHandlers,changed} from 'notes-mock'
const assert=(value,message)=>{if(!value)throw Error(message)}
const pause=()=>new Promise(resolve=>setTimeout(resolve,40))
const settle=async()=>{await nextTick();await pause();await nextTick()}
const button=(label,scope=document)=>[...scope.querySelectorAll('button')].find(n=>n.getClientRects().length&&n.textContent.trim()===label)
const set=(element,value)=>{assert(element,'找不到输入控件');element.value=value;element.dispatchEvent(new Event(element.tagName==='SELECT'?'change':'input',{bubbles:true}))}
const local=value=>new Date(value-new Date(value).getTimezoneOffset()*60000).toISOString().slice(0,16)
const day=date=>document.querySelector('.calendar-day[data-date="'+date+'"]')
const period=(year,month)=>{set(document.querySelector('[aria-label=日历年份]'),String(year));set(document.querySelector('[aria-label=日历月份]'),String(month-1))}
const route=ref('calendar')
const request=ref(null)
const notes=ref(null)
const workspace={
 refresh:()=>notes.value.refresh(),
 focusSearch:()=>notes.value.focusSearch(),
 toggleFilters:()=>notes.value.toggleFilters(),
 openOrganizer:()=>notes.value.openOrganizer()
}
const toCalendar=()=>{route.value='calendar'}
const toNotes=(payload=null)=>{request.value=payload;route.value='notes'}
createApp({
 setup:()=>()=>route.value==='calendar'
  ?h(Calendar,{onOpenNotes:toNotes})
  :h(Notes,{ref:notes,noteRequest:request.value,providers:[],codexAccounts:[],runtimeModels:[]})
}).mount('#app')
async function test(){
 for(let i=0;i<30&&!document.querySelector('.calendar-day');i++)await pause()
 assert(document.querySelectorAll('.calendar-day').length===42,'默认月视图没有显示六周')
 assert(!document.querySelector('.notes-header')&&!document.querySelector('.notes-filters'),'日历不应有列表头部')
 const calendar=document.querySelector('.note-calendar').getBoundingClientRect()
 assert(Math.abs(calendar.bottom-document.querySelector('#app').getBoundingClientRect().bottom)<20,'日历未填满可用高度')
 button('周视图').click();await settle();assert(document.querySelectorAll('.calendar-day').length===7,'周视图没有显示七天')
 button('列表').click();await settle();assert(document.querySelector('.calendar-list'),'没有切换到日程列表')
 button('月视图').click();await settle();period(2026,11);await settle();day('2026-11-18').click();await settle()
 assert(day('2026-11-18').querySelector('.calendar-lunar').textContent==='初十','农历日期错误')
 const rightClick=async date=>{
  const cell=day(date),rect=cell.getBoundingClientRect()
  cell.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,button:2,clientX:rect.left+8,clientY:rect.top+8}))
  await settle()
 }
 const contextItem=label=>[...document.querySelectorAll('[role=menuitem]')].find(node=>node.getClientRects().length&&node.textContent.includes(label))
 const leaveDialog=()=>document.querySelector('.calendar-leave-modal')
 await rightClick('2026-11-20')
 assert(contextItem('设置请假'),'右键日期没有显示请假菜单')
 contextItem('设置请假').click();await settle()
 let leave=leaveDialog()
 const drawer=leave.querySelector('.base-modal__panel').getBoundingClientRect()
 const footer=leave.querySelector('.leave-actions').getBoundingClientRect()
 assert(Math.abs(drawer.right-innerWidth)<2&&drawer.top===0&&drawer.left>0,'请假编辑器没有沿窗口右侧贴边')
 assert(Math.abs(footer.bottom-drawer.bottom)<2,'请假操作栏没有固定在抽屉底部')
 assert(document.activeElement.id==='calendar-leave-title','打开请假后焦点没有进入抽屉标题栏')
 const dates=()=>leave.querySelectorAll('input[type=date]')
 assert([...dates()].every(input=>input.value==='2026-11-20'),'请假未带入右键日期')
 set(dates()[1],'2026-11-22')
 button('时间范围',leave).click();await settle()
 const [from,to]=leave.querySelectorAll('input[type=time]')
 assert(dates()[0].closest('.leave-row').contains(from)&&dates()[1].closest('.leave-row').contains(to),'跨天时段没有与各自的日期配对')
 set(from,'14:00');set(to,'10:00');button('保存请假',leave).click();await settle()
 assert(!leaveDialog()&&day('2026-11-20').getAttribute('aria-label').includes('14:00 起'),'首日请假没更新')
 assert(day('2026-11-21').getAttribute('aria-label').includes('休息，请假 · 全天'),'中间日请假覆盖了原排班')
 assert(day('2026-11-22').getAttribute('aria-label').includes('至 10:00'),'末日请假没更新')
 const leaveCell=day('2026-11-21')
 const leaveMark=leaveCell.querySelector('.calendar-leave-mark')
 const cellRect=leaveCell.getBoundingClientRect(), markRect=leaveMark.getBoundingClientRect()
 assert(markRect.top-cellRect.top<8&&cellRect.right-markRect.right<8,'请假标签没有紧贴右上角')
 assert(getComputedStyle(leaveCell).backgroundColor!==getComputedStyle(day('2026-11-28')).backgroundColor,'请假日期没有独立底色')
 assert(leaveCell.querySelector('.calendar-schedule-label').getBoundingClientRect().top>=markRect.bottom,'原排班标签覆盖了请假')
 assert(!day('2026-11-23').querySelector('.calendar-leave-mark'),'日期范围多标了一天')
 await rightClick('2026-11-21')
 contextItem('编辑请假').click();await settle()
 leave=leaveDialog()
 assert(dates()[0].value==='2026-11-20'&&dates()[1].value==='2026-11-22','多天请假未打开同一记录')
 button('全天',leave).click();await settle();button('保存请假',leave).click();await settle()
 const update=calls.findLast(call=>call.action==='calendar-rule-save').p
 assert(update.id&&update.revision===1&&update.startTime===''&&update.endTime==='','改为全天未清空时间或未用当前修订号')
 button('列表').click();await settle()
 assert(document.querySelectorAll('.calendar-leave-entry').length===3,'列表没有显示只含请假的日期')
 document.querySelector('.calendar-leave-entry').click();await settle()
 button('删除请假',leaveDialog()).click();await settle()
 assert(!document.querySelector('.calendar-leave-entry'),'删除后请假残留')
 button('月视图').click();await settle()
 await quickNotesMockAi()
 button('休假规则').click();await settle()
 const rules=document.querySelector('.calendar-rules-modal')
 assert(rules.querySelectorAll('.rules-item').length===1,'排班设置未保留原规则')
 button('AI 设置',rules).click();await settle()
 assert(rules.querySelector('.rules-description').getClientRects().length,'AI 设置未显示输入区')
 rules.querySelector('.base-modal__close').click();await settle()
 day('2026-11-19').click();await settle();assert(document.querySelector('.calendar-agenda').textContent.includes('验证重启后的提醒恢复'),'截止日没有关联待办')
 assert(day('2026-11-19').querySelector('.calendar-dot-usage'),'有模型请求的日期没有用量标记')
 const usage=[...document.querySelectorAll('.calendar-usage-metrics dd')]
 assert(usage[0].textContent==='7'&&usage[1].title===(1200).toLocaleString()&&usage[2].title===(350).toLocaleString(),'当日用量和所选日期不匹配')
 assert(calls.some(call=>call.action==='usage:stats'&&call.p.statsScope==='calendar'),'日历没有请求独立的按日用量')
 document.querySelector('[aria-label=在这天新建便签]').click();await settle()
 let form=document.querySelector('.note-form')
 assert(document.querySelectorAll('.notes-nav-section').length===3&&!document.querySelector('.note-calendar'),'日历新建没有打开独立的便签工具')
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
 // 桌宠查看和通知点击通过同一打开事件定位便签，复用工作区时也应切换到指定详情。
 const openedId=calls.filter(call=>call.action==='detail').at(-1).p.id
 assert(openHandlers.size===1,'便签工作区没有注册打开事件')
 for(const open of openHandlers)open({id:'idea'})
 await settle()
 assert(document.querySelector('.editor-title-field input').value==='周五的发布检查','外部打开事件没有定位到指定便签')
 for(const open of openHandlers)open({id:openedId})
 await settle()
 const tracking=document.querySelector('.work-tracking-fields')
 assert(tracking.querySelector('select').value==='','旧工作记录应保持未跟踪')
 set(document.querySelector('.body-input'),'尚未保存的正文')
 set(tracking.querySelector('select'),'in_progress')
 await nextTick()
 set(tracking.querySelector('input'),'40')
 set(document.querySelector('textarea[maxlength="500"]'),'已修复接口，等待联调')
 button('记录进度').click();await settle()
 const progressSave=calls.findLast(c=>c.action==='save').p
 assert(progressSave.workTracking.progress===40&&!('content' in progressSave)&&!('tasks' in progressSave),'记录进度提交了其他草稿字段')
 assert(document.querySelector('.body-input').value==='尚未保存的正文','记录进度丢失了正文草稿')
 assert(document.querySelector('.editor-history').textContent.includes('进行中 · 40%')&&document.querySelector('.editor-history').textContent.includes('等待联调'),'进度记录未显示在操作记录')
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
 assert(calls.some(c=>c.action==='save'&&c.p.title==='更新后的标题'&&c.p.workTracking.progress===40&&c.p.revision===progressSave.revision+1),'详情编辑没有保留人工进度和新修订号')
 button('工作留痕').click();await settle()
 set(document.querySelector('[aria-label=工作状态筛选]'),'planned');await settle()
 assert(calls.findLast(c=>c.action==='list').p.workStatus==='planned'&&!document.querySelector('.quick-note-card'),'切换状态没有筛选工作留痕')
 set(document.querySelector('[aria-label=工作状态筛选]'),'in_progress');await settle()
 assert(document.querySelector('.note-card-tracking progress')?.value===40,'卡片未显示已记录的进度')
 button('全部记录').click();await settle()
 assert(!calls.findLast(c=>c.action==='list').p.workStatus,'全部记录仍携带工作状态筛选')
 // 验证提交中的临时状态与后台任务接管，其他便签不应被点亮。
 const cardByTitle=title=>[...document.querySelectorAll('.quick-note-card')].find(card=>card.querySelector('.note-card-title').textContent===title)
 globalThis.notesPageSize=2
 await workspace.refresh();await settle()
 assert(!cardByTitle('界面回归记录'),'分页用例缺少下一页便签')
 button('加载更多').click();await settle()
 const workCard=cardByTitle('更新后的标题')
 const loadedCard=cardByTitle('界面回归记录')
 workCard.querySelector('[aria-label=选择便签]').click()
 loadedCard.querySelector('[aria-label=选择便签]').click()
 await nextTick()
 globalThis.deferSummary=true
 const summaryButton=button('生成工作小结')
 summaryButton.click();await settle()
 const pulsing=[...document.querySelectorAll('.note-summarizing')]
 assert(
  pulsing.length===2&&pulsing.includes(workCard)&&pulsing.includes(loadedCard),
  '只有参与小结的卡片应立即显示生成状态'
 )
 assert(
  getComputedStyle(workCard).animationName.includes('note-summary-pulse')&&summaryButton.disabled,
  '生成期间缺少边框动效或可重复提交'
 )
 assert(!button('取消生成',workCard)&&!document.querySelector('.notes-pending'),'提交期间应只在卡片显示生成状态')
 globalThis.deferSummary=false
 globalThis.releaseSummary();await settle()
 const firstSummary=state.captures[0]
 assert(loadedCard.isConnected&&button('取消生成',loadedCard),'启动后台任务后没有保留下一页来源卡片及取消入口')
 assert(!document.querySelector('.notes-pending'),'后台小结重复显示在顶部')
 button('取消生成',loadedCard).click();await settle()
 assert(calls.findLast(c=>c.action==='capture-cancel').p.requestId===firstSummary.id,'卡片取消了其他小结任务')
 assert(!document.querySelector('.note-summarizing'),'取消后卡片仍显示生成状态')
 const pending=document.querySelector('.notes-pending')
 assert(
  pending.textContent.includes('工作小结 · 2 条便签')&&
  !pending.textContent.includes('内部小结输入')&&!button('保存原文',pending),
  '取消后把内部小结输入当成了便签原文'
 )
 button('重试',pending).click();await settle()
 assert(
  state.captures[0].id!==firstSummary.id&&
  JSON.stringify(state.captures[0].source.noteIds)===JSON.stringify(firstSummary.source.noteIds),
  '重试没有按原来的便签生成小结'
 )
 Object.assign(state.captures[0],{status:'failed',error:'模型不可用'})
 changed();await settle()
 const failedSummary=document.querySelector('.notes-pending')
 assert(!document.querySelector('.note-summarizing')&&failedSummary?.textContent.includes('模型不可用'),'失败后应停止动效并显示原因')
 button('忽略',failedSummary).click();await settle()
 globalThis.notesPageSize=undefined
 button('AI-Manager').click();await settle()
 cardByTitle('更新后的标题').querySelector('[aria-label=选择便签]').click();await nextTick()
 button('生成工作小结').click();await settle()
 assert(document.querySelector('.notes-title').textContent==='AI-Manager'&&document.querySelector('.note-summarizing'),'从项目中生成小结离开了当前项目或丢失了生成状态')
 button('全部记录').click();await settle()
 assert(document.querySelector('.note-summarizing')===cardByTitle('更新后的标题'),'切换视图后没有按来源便签恢复生成状态')
 state.captures[0].status='completed';changed();await settle()
 assert(!document.querySelector('.note-summarizing')&&!document.querySelector('.notes-pending'),'后台小结完成后仍显示生成状态')
 // 卡片弹窗在列表中完成新增，取消和无效时间都不提交。
 const ideaCard=[...document.querySelectorAll('.quick-note-card')].find(card=>card.textContent.includes('周五的发布检查'))
 const addReminder=ideaCard.querySelector('[aria-label=添加提醒]')
 const detailRequests=calls.filter(c=>c.action==='detail').length
 const reminderSaves=calls.filter(c=>c.action==='save').length
 addReminder.focus();addReminder.click();await settle()
 let quickReminder=document.querySelector('.notes-quick-reminder')
 const quickTime=quickReminder.querySelector('input')
 assert(document.activeElement===quickTime&&new Date(quickTime.value).getTime()>Date.now(),'弹窗没有聚焦并预填提醒时间')
 assert(ideaCard.getClientRects().length&&!document.querySelector('.note-editor-inline'),'添加提醒离开了便签列表')
 set(quickTime,local(Date.now()-60000))
 button('保存提醒',quickReminder).click();await settle()
 assert(quickReminder.querySelector('[role=alert]').textContent.includes('必须晚于当前时间'),'卡片提醒没有拒绝已过去的时间')
 button('取消',quickReminder).click();await settle()
 assert(!document.querySelector('.notes-quick-reminder')&&document.activeElement===addReminder,'取消提醒未回到卡片按钮')
 assert(calls.filter(c=>c.action==='save').length===reminderSaves,'取消或校验失败时写入了提醒')
 addReminder.click();await settle()
 quickReminder=document.querySelector('.notes-quick-reminder')
 const quickAt=local(Date.now()+120000)
 set(quickReminder.querySelector('input'),quickAt)
 button('保存提醒',quickReminder).click();await settle()
 const appended=calls.findLast(c=>c.action==='save').p
 assert(appended.id==='idea'&&appended.appendReminder.at===new Date(quickAt).getTime(),'卡片提醒未保存所选日期时间')
 assert(!document.querySelector('.notes-quick-reminder')&&ideaCard.querySelector('[aria-label=管理提醒]'),'保存后没有刷新卡片时间')
 assert(calls.filter(c=>c.action==='detail').length===detailRequests,'卡片新增提醒调用了详情入口')
 document.querySelector('.quick-note-card [aria-label=管理提醒]')?.click();await settle()
 if(document.querySelector('.note-form')){button('返回列表').click();await settle()}
 // 分析先生成可编辑预览，确认前不能写入正式便签。
 button('智能记录').click();await settle()
 const modal=[...document.querySelectorAll('.smart-record-modal')].find(n=>n.getClientRects().length)
 let source=modal.querySelector('.smart-input');set(source,'修复登录页与预约页分值不同步的问题，补充回归测试。');await settle()
 // 提醒精度从模型预览到确认、再编辑均须保持原始触发点。
 const preciseTime=new Date(Date.now()+120000)
 preciseTime.setSeconds(34,456)
 globalThis.aiReminderAt=preciseTime.getTime()
 const count=state.notes.length
 button('分析并预览',modal).click();await settle();await settle()
 assert(globalThis.lastCapture.preview===true,'智能记录没有请求预览')
 assert(state.notes.length===count&&!calls.some(c=>c.action==='capture-save'),'预览前提前保存了便签')
 assert(modal.querySelector('.smart-preview-title').textContent==='修复分值同步问题','没有渲染模型实际返回结果')
 const previewTracking=modal.querySelector('.work-tracking-fields')
 assert(previewTracking.querySelector('select').value==='planned'&&previewTracking.querySelector('input').value==='','AI 预览没有保留未知进度')
 set(previewTracking.querySelector('select'),'in_progress')
 await nextTick()
 set(previewTracking.querySelector('input'),'100')
 button('确认并保存',modal).click();await settle()
 assert(state.notes.length===count,'非完成状态接受了 100%')
 set(previewTracking.querySelector('input'),'40')
 const previewTime=modal.querySelector('.smart-field input[type=datetime-local]')
 assert(previewTime?.value.length===19&&previewTime.step==='1','AI 提醒预览没有保留秒数')
 const suggestions=modal.querySelectorAll('.smart-task').length
 button('添加',modal).click();await settle();assert(modal.querySelectorAll('.smart-task').length===suggestions+1,'预览没有新增待办')
 const newSuggestion=[...modal.querySelectorAll('.smart-task')].at(-1);set(newSuggestion.querySelector('input'),'补充检查');newSuggestion.querySelector('[aria-label=移除建议待办]').click();await settle()
 assert(modal.querySelectorAll('.smart-task').length===suggestions,'预览没有移除待办')
 set(modal.querySelector('.smart-field input'),'确认后的 AI 标题');button('确认并保存',modal).click();await settle()
 assert(calls.some(c=>c.action==='capture-save'&&c.p.note.title==='确认后的 AI 标题'),'预览修改没有提交')
 assert(state.notes.length===count+1,'确认后没有生成便签')
 const confirmed=calls.findLast(c=>c.action==='capture-save').p.note
 assert(confirmed.workTracking.status==='in_progress'&&confirmed.workTracking.progress===40,'AI 预览修改的工作进度没有保存')
 assert(confirmed.reminders[0].nextFireAt===globalThis.aiReminderAt&&confirmed.reminders[0].schedule.at===globalThis.aiReminderAt,'确认保存改变了 AI 提醒的触发点')
 const reminderCard=[...document.querySelectorAll('.quick-note-card')].find(n=>n.textContent.includes('确认后的 AI 标题'))
 assert(reminderCard.querySelector('[aria-label=管理提醒]'),'AI 保存的卡片没有启用提醒')
 reminderCard.querySelector('.note-card-open').click();await settle()
 assert(document.querySelector('.note-reminder input[type=datetime-local]').value.length===19,'再次编辑丢失提醒秒数')
 button('保存修改').click();await settle()
 assert(calls.findLast(c=>c.action==='save').p.reminders[0].nextFireAt===globalThis.aiReminderAt,'再次保存改变了已有提醒时间')
 globalThis.aiReminderAt=null
 button('智能记录').click();await settle();source=modal.querySelector('.smart-input');set(source,'关闭后保留的草稿');await settle();modal.querySelector('[aria-label=关闭智能记录]').click();await settle()
 button('智能记录').click();await settle();assert(source.value==='关闭后保留的草稿','关闭后丢失原文草稿')
 button('仅保存原文',modal).click();await settle();assert(calls.some(c=>c.action==='save'&&c.p.content==='关闭后保留的草稿'),'仅保存原文没有调用保存')
 // 新建草稿可以保存到本机并恢复，不会作为正式便签提交。
 button('新建便签').click();await settle();set(document.querySelector('.body-input'),'本机草稿内容');button('存为草稿').click();await settle()
 assert(JSON.parse(localStorage.getItem('quick-notes:new-draft')).content==='本机草稿内容','草稿没有存入本机')
 document.querySelector('[aria-label=关闭便签编辑器]').click();await settle();button('新建便签').click();await settle()
 assert(document.querySelector('.body-input').value==='本机草稿内容','未恢复本机草稿');document.querySelector('[aria-label=关闭便签编辑器]').click();localStorage.removeItem('quick-notes:new-draft');await settle()
 toCalendar();await settle();period(2028,1);await settle();day('2028-01-31').click();document.querySelector('[aria-label=下个月]').click();await settle();assert(day('2028-02-29').getAttribute('aria-pressed')==='true','闰年月末切换错误')
 period(2026,12);await settle();day('2026-12-31').click();document.querySelector('[aria-label=下个月]').click();await settle();assert(day('2027-01-31').getAttribute('aria-pressed')==='true','跨年切换错误')
 period(2026,11);await settle();day('2026-11-18').click();await settle()
 document.querySelector('.calendar-entry').click();await settle()
 assert(document.querySelector('.note-editor-inline'),'日历事项没有打开原便签详情')
 await workspace.focusSearch();await settle()
 assert(document.activeElement===document.querySelector('.notes-search input'),'顶部搜索入口没有聚焦列表搜索')
 workspace.toggleFilters();await settle();assert(!document.querySelector('.notes-filters'),'顶部筛选入口没有收起筛选')
 workspace.toggleFilters();await settle();assert(document.querySelector('.notes-filters'),'顶部筛选入口没有恢复筛选')
 workspace.openOrganizer();await settle();assert(document.querySelector('.notes-organizer'),'顶部目录入口没有打开目录管理')
 document.querySelector('.notes-organizer').closest('.base-modal').querySelector('.base-modal__close').click();await settle()
 await workspace.refresh();await settle();toCalendar();await settle()
 period(2026,11);await settle();day('2026-11-18').click();await settle()
 const scene=__NOTES_SCENE__
 if(scene==='list'){toNotes();await settle()}
 if(scene==='detail'){document.querySelector('.calendar-entry').click();await settle()}
 if(scene==='drawer'){document.querySelector('[aria-label=在这天新建便签]').click();await settle()}
 if(scene==='smart'){button('智能记录').click();await settle();const smart=document.querySelector('.smart-record-modal');set(smart.querySelector('.smart-input'),'修复登录页与预约页分值不同步的问题，补充回归验证。');await settle();button('分析并预览',smart).click();await settle();await settle()}
 if(scene==='rules'){button('休假规则').click();await settle()}
 if(scene==='leave'){day('2026-11-18').click();button('请假').click();await settle();button('时间范围',leaveDialog()).click();await settle()}
 if(scene==='usage'){day('2026-11-19').click();await settle()}
 if(scene==='leave-edit'){period(2026,11);await settle();await rightClick('2026-11-25');contextItem('编辑请假').click();await settle()}
 if(scene==='leave-dates'){period(2026,11);await settle();day('2026-11-26').click();await settle()}
 const visibleWorkspace=document.querySelector('.quick-notes,.calendar-view')
 assert(visibleWorkspace.scrollWidth<=visibleWorkspace.clientWidth,'工作区横向溢出')
 document.getElementById('result').textContent='PASS: 便签布局重构，侧栏分组、全高日历、月周列表、农历、抽屉新建、正文工具栏、草稿恢复、双栏详情、提醒校验、AI 预览确认与原文保存'
}
async function quickNotesMockAi(){
 state.calendarRules.push({id:'assistant-leave',title:'助手请假',kind:'leave',calendar:'solar',repeat:'once',startDate:'2026-11-25',endDate:'2026-11-27',enabled:true,revision:1})
 changed();await settle()
 assert(day('2026-11-27').querySelector('.calendar-leave-mark'),'外部写入没有刷新已打开的日历')
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
