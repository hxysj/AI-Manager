use crate::core::{database, error::ManagerError, paths::AppPaths, service_provider_store::query};
use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Timelike, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};

pub fn fail(message: impl Into<String>) -> ManagerError { ManagerError::System(message.into()) }
pub fn now() -> i64 { Utc::now().timestamp_millis() }
pub fn text<'a>(value: &'a Value, key: &str) -> &'a str { value[key].as_str().unwrap_or("") }
fn id() -> String { uuid::Uuid::new_v4().to_string() }

fn work_status_label(status: &str) -> Option<&'static str> {
    match status {
        "planned" => Some("计划中"),
        "in_progress" => Some("进行中"),
        "blocked" => Some("已阻塞"),
        "paused" => Some("已暂停"),
        "completed" => Some("已完成"),
        _ => None,
    }
}

pub fn open(paths: &AppPaths) -> Result<Connection, ManagerError> {
    let mut c = database::open(paths)?;
    if !c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='quick_notes')", [], |r|r.get::<_,bool>(0))? {
        let tx=c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(include_str!("quick_notes_schema.sql"))?;
        for (kind, key, name) in [("group","inbox","收件箱"),("category","uncategorized","未分类"),
            ("category","planning","需求计划"),("category","development","开发实现"),("category","debugging","问题排查"),
            ("category","meeting","会议沟通"),("category","release","发布部署"),("category","learning","学习资料"),("category","life","生活事务")] {
            tx.execute("INSERT OR IGNORE INTO quick_note_folders(id,kind,name,aliases_json,sort_order,archived) VALUES(?1,?2,?3,'[]',0,0)",params![key,kind,name])?;
        }
        tx.commit()?;
    }
    crate::core::quick_notes_calendar_store::initialize(&mut c)?;
    Ok(c)
}

pub fn initialize(paths: &AppPaths) -> Result<(), ManagerError> {
    let c=open(paths)?;
    c.execute("UPDATE quick_note_captures SET status='interrupted',error='应用退出，原文已保留，可重新整理' WHERE status='running'", [])?;
    c.execute("UPDATE quick_note_reminder_events SET status='pending' WHERE status='delivering'",[])?;
    Ok(())
}

fn read_note(c: &Connection, key: &str) -> Result<Value, ManagerError> {
    let raw: String = c.query_row("SELECT payload_json FROM quick_notes WHERE id=?1", [key], |r|r.get(0))
        .optional()?.ok_or_else(||fail("便签不存在或已彻底删除"))?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn detail(paths: &AppPaths, key: &str) -> Result<Value, ManagerError> {
    let c = open(paths)?;
    let mut note = read_note(&c,key)?;
    note["reminders"] = json!(reminders(&c,key)?);
    note["history"] = json!(query(&c,"SELECT id,operation,actor,created_at AS createdAt,before_json AS beforeJson,after_json AS afterJson FROM quick_note_revisions WHERE note_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 100",&[&key])?);
    note["captures"] = json!(query(&c,"SELECT id,source_json AS sourceJson,source_text AS sourceText,captured_at AS capturedAt,status,error,model_json AS modelJson FROM quick_note_captures WHERE note_id=?1 ORDER BY captured_at DESC",&[&key])?);
    Ok(note)
}

fn reminders(c: &Connection, key: &str) -> Result<Vec<Value>,ManagerError> {
    query(c,"SELECT id,note_id AS noteId,task_id AS taskId,schedule_json AS scheduleJson,next_fire_at AS nextFireAt,enabled,revision FROM quick_note_reminders WHERE note_id=?1 ORDER BY next_fire_at,id",&[&key])
}

pub fn list(paths: &AppPaths, filters: &Value) -> Result<Value,ManagerError> {
    let c = open(paths)?;
    let search = format!("%{}%",text(filters,"search").replace('\\',"\\\\").replace('%',"\\%").replace('_',"\\_"));
    let lifecycle = filters["lifecycle"].as_str().unwrap_or("active");
    let limit = filters["limit"].as_i64().unwrap_or(300).clamp(1,1000);
    let offset = filters["offset"].as_i64().unwrap_or(0).max(0);
    let rows = query(&c,"SELECT payload_json AS payloadJson FROM quick_notes WHERE lifecycle=?1
        AND (?2='' OR group_id=?2) AND (?3='' OR category_id=?3) AND (?4='' OR note_type=?4)
        AND (?5='%%' OR payload_json LIKE ?5 ESCAPE '\\' OR EXISTS(SELECT 1 FROM quick_note_captures cap WHERE cap.note_id=quick_notes.id AND cap.source_text LIKE ?5 ESCAPE '\\'))
        AND (?6='' OR (note_type='worklog' AND COALESCE(json_extract(payload_json,'$.workTracking.status'),'untracked')=?6))
        ORDER BY pinned DESC,updated_at DESC,id LIMIT ?7 OFFSET ?8",
        &[&lifecycle,&text(filters,"groupId"),&text(filters,"categoryId"),&text(filters,"type"),&search,&text(filters,"workStatus"),&limit,&offset])?;
    let notes = rows.into_iter().map(|r|r["payload"].clone()).collect::<Vec<_>>();
    let folders = query(&c,"SELECT id,kind,name,aliases_json AS aliasesJson,sort_order AS sortOrder,archived FROM quick_note_folders ORDER BY sort_order,name",&[])?;
    let plans = query(&c,"SELECT id,note_id AS noteId,task_id AS taskId,next_fire_at AS nextFireAt,enabled,schedule_json AS scheduleJson,revision FROM quick_note_reminders ORDER BY next_fire_at",&[])?;
    let events = query(&c,"SELECT e.id,e.note_id AS noteId,e.reminder_id AS reminderId,e.scheduled_at AS scheduledAt,e.status,e.error,e.read_at AS readAt,n.title,n.lifecycle
        FROM quick_note_reminder_events e JOIN quick_notes n ON n.id=e.note_id WHERE n.lifecycle<>'trash' ORDER BY scheduled_at DESC LIMIT 200",&[])?;
    let captures = query(&c,"SELECT id,note_id AS noteId,source_json AS sourceJson,source_text AS sourceText,captured_at AS capturedAt,status,error FROM quick_note_captures WHERE status<>'completed' ORDER BY captured_at DESC LIMIT 50",&[])?;
    Ok(json!({"notes":notes,"folders":folders,"reminders":plans,"events":events,"captures":captures,"calendarRules":crate::core::quick_notes_calendar_store::list_in(&c)?,"hasMore":notes.len()==limit as usize,"offset":offset,"limit":limit}))
}

fn valid_date(value: &Value,field: &str) -> Result<(),ManagerError> {
    if value.is_null() || value.as_str()==Some("") { return Ok(()); }
    let valid=value.as_str().and_then(|s|NaiveDate::parse_from_str(s,"%Y-%m-%d").ok().filter(|date|date.format("%Y-%m-%d").to_string()==s));
    if valid.is_none(){return Err(fail(format!("{field} 日期必须为 YYYY-MM-DD 或 null，当前值：{value}")));}
    Ok(())
}

// 仅在事务内写入便签、任务、修订和提醒，人工修改通过 revision 防止覆盖。
pub fn save_in(c: &Connection, input: &Value, actor: &str) -> Result<Value,ManagerError> {
    if !input.is_object(){return Err(fail("便签必须是对象"));}
    let key = if text(input,"id").is_empty() { id() } else { text(input,"id").to_owned() };
    let previous = c.query_row("SELECT payload_json FROM quick_notes WHERE id=?1",[&key],|r|r.get::<_,String>(0)).optional()?
        .map(|s|serde_json::from_str::<Value>(&s)).transpose()?;
    if previous.as_ref().is_some_and(|p|p["revision"]!=input["revision"]) { return Err(fail("便签已在其他窗口更新，请重新打开后再保存")); }
    let append_reminder = input.get("appendReminder").filter(|_| actor == "user");
    if append_reminder.is_some() {
        if previous.as_ref().is_none_or(|note| note["lifecycle"] != "active") {
            return Err(fail("只能为有效的便签添加提醒"));
        }
        if !input.as_object().unwrap().keys().all(|key| {
            matches!(key.as_str(), "id" | "revision" | "appendReminder")
        }) {
            return Err(fail("快速添加提醒需要单独保存"));
        }
    }
    let mut note = previous.clone().unwrap_or(json!({"id":key,"type":"note","title":"","content":"","groupId":"inbox","categoryId":"uncategorized","tags":[],"tasks":[],"worklog":{},"color":"sand","pinned":false,"lifecycle":"active","occurredOn":null,"createdAt":now(),"revision":0}));
    if previous.is_none() && actor=="ai" {if let Some(at)=input["capturedAt"].as_i64(){note["createdAt"]=json!(at);}}
    for field in ["type","title","content","groupId","categoryId","tags","tasks","worklog","color","pinned","lifecycle","occurredOn","pendingTime"] {
        if let Some(value)=input.get(field) { note[field]=value.clone(); }
    }
    let content = text(&note,"content").trim().to_string();
    if content.chars().count()>100_000 {return Err(fail("单条便签最多 100000 字"));}
    if content.is_empty() && text(&note,"title").trim().is_empty() {return Err(fail("请输入便签内容或标题"));}
    if !["note","todo","worklog"].contains(&text(&note,"type")) || !["active","archived","trash"].contains(&text(&note,"lifecycle")) {return Err(fail("便签类型或状态无效"));}
    if !["sand","mint","sky","rose","lavender"].contains(&text(&note,"color")) {note["color"]=json!("sand");}
    for (field,kind,default) in [("groupId","group","inbox"),("categoryId","category","uncategorized")] {
        if text(&note,field).is_empty(){note[field]=json!(default);}
        if !c.query_row("SELECT EXISTS(SELECT 1 FROM quick_note_folders WHERE id=?1 AND kind=?2)",params![text(&note,field),kind],|r|r.get::<_,bool>(0))? {return Err(fail("分类或分组不存在"));}
    }
    valid_date(&note["occurredOn"],"occurredOn")?;
    note["content"]=json!(content);
    note["title"]=json!(if text(&note,"title").trim().is_empty() {content.lines().next().unwrap_or("便签").chars().take(60).collect::<String>()} else {text(&note,"title").trim().chars().take(160).collect()});
    if !note["tags"].as_array().is_some_and(|a|a.len()<=30 && a.iter().all(|t|t.as_str().is_some_and(|s|s.chars().count()<=40))) {return Err(fail("标签最多 30 项，每项最多 40 字"));}
    if !note["worklog"].is_object(){return Err(fail("工作留痕格式无效"));}
    // AI 只可给新记录提供初始状态，已有记录的更新说明只写入当次修订。
    let progress_note = if actor == "user" && note["type"] == "worklog" {
        text(input, "progressNote").trim()
    } else {
        ""
    };
    let mut tracking_changed = false;
    if note["type"] == "worklog" && (actor == "user" || previous.is_none()) {
        if progress_note.chars().count() > 500 {
            return Err(fail("进度说明最多 500 字"));
        }
        let tracking = &input["workTracking"];
        if !tracking.is_null() {
            let status = text(tracking, "status");
            if work_status_label(status).is_none() {
                return Err(fail("工作状态无效"));
            }
            let progress = tracking["progress"].as_u64();
            if !tracking["progress"].is_null() && !progress.is_some_and(|value| value <= 100) {
                return Err(fail("工作进度必须留空或填写 0 至 100 的整数"));
            }
            if (progress == Some(100)) != (status == "completed") {
                return Err(fail("已完成对应 100%，其他状态的进度应低于 100% 或留空"));
            }
            let tracking = json!({"status":status,"progress":progress});
            tracking_changed = note["workTracking"] != tracking;
            note["workTracking"] = tracking;
        }
        if !progress_note.is_empty() && note["workTracking"].is_null() {
            return Err(fail("请先选择工作状态"));
        }
    }
    let tasks=note["tasks"].as_array_mut().ok_or_else(||fail("待办必须是数组"))?;
    if tasks.len()>100 {return Err(fail("单条便签最多 100 项待办"));}
    let mut task_ids=std::collections::HashSet::new();
    for task in tasks.iter_mut() {
        if !task.is_object(){return Err(fail("待办条目必须是对象"));}
        if text(task,"title").trim().is_empty() {return Err(fail("待办内容不能为空"));}
        if text(task,"id").is_empty(){task["id"]=json!(id());}
        if !task_ids.insert(text(task,"id").to_owned()){return Err(fail("待办 ID 重复"));}
        if task.get("status").is_none(){task["status"]=json!("pending");}
        if !["pending","doing","completed","cancelled"].contains(&text(task,"status")){return Err(fail("待办状态无效"));}
        valid_date(&task["dueDate"],"tasks.dueDate")?;
        if !task["dueAt"].is_null() && !task["dueAt"].is_i64(){return Err(fail("任务截止时间无效"));}
        let old=previous.as_ref().and_then(|p|p["tasks"].as_array()).and_then(|a|a.iter().find(|t|t["id"]==task["id"]));
        task["completedAt"]=if task["status"]=="completed" {old.filter(|t|t["status"]=="completed").and_then(|t|t.get("completedAt")).cloned().unwrap_or(json!(now()))} else {Value::Null};
    }
    note["updatedAt"]=json!(now());
    note["revision"]=json!(previous.as_ref().and_then(|p|p["revision"].as_i64()).unwrap_or(0)+1);
    c.execute("INSERT INTO quick_notes(id,title,note_type,group_id,category_id,lifecycle,pinned,created_at,updated_at,revision,payload_json)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(id) DO UPDATE SET title=excluded.title,note_type=excluded.note_type,
        group_id=excluded.group_id,category_id=excluded.category_id,lifecycle=excluded.lifecycle,pinned=excluded.pinned,updated_at=excluded.updated_at,revision=excluded.revision,payload_json=excluded.payload_json",
        params![key,text(&note,"title"),text(&note,"type"),text(&note,"groupId"),text(&note,"categoryId"),text(&note,"lifecycle"),note["pinned"].as_bool().unwrap_or(false),note["createdAt"].as_i64(),note["updatedAt"].as_i64(),note["revision"].as_i64(),note.to_string()])?;
    let operation = if previous.is_none() {
        "create"
    } else if tracking_changed || !progress_note.is_empty() {
        "progress"
    } else {
        "update"
    };
    let mut revision_after = note.clone();
    if !progress_note.is_empty() {
        revision_after["progressNote"] = json!(progress_note);
    }
    c.execute("INSERT INTO quick_note_revisions(id,note_id,operation,actor,created_at,before_json,after_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![id(),key,operation,actor,note["updatedAt"].as_i64(),previous.unwrap_or(Value::Null).to_string(),revision_after.to_string()])?;
    // 卡片请求只追加一条提醒，已有计划和通知记录保持原样。
    if let Some(reminder) = append_reminder {
        let at = reminder["at"].as_i64().ok_or_else(|| fail("请选择具体提醒时间"))?;
        save_reminder(c, &note, &json!({
            "nextFireAt": at,
            "schedule": {"at": at, "repeat": "once", "sound": true, "timeZone": reminder["timeZone"]}
        }))?;
        return Ok(note);
    }
    // 单独记录进度时，保留已有提醒和提醒事件。
    if operation == "progress" && input.as_object().is_some_and(|fields| {
        fields.keys().all(|key| {
            matches!(key.as_str(), "id" | "revision" | "workTracking" | "progressNote")
        })
    }) {
        return Ok(note);
    }
    if let Some(plans)=input["reminders"].as_array() {
        let keep=plans.iter().filter_map(|p|p["id"].as_str()).collect::<std::collections::HashSet<_>>();
        for old in reminders(c,&key)? {if !keep.contains(text(&old,"id")){
            c.execute("UPDATE quick_note_reminder_events SET status='cancelled' WHERE reminder_id=?1 AND status IN ('pending','snoozed')",[text(&old,"id")])?;
            c.execute("DELETE FROM quick_note_reminders WHERE id=?1",[text(&old,"id")])?;
        }}
        for plan in plans {save_reminder(c,&note,plan)?;}
    }
    if note["lifecycle"]!="active" {
        c.execute("UPDATE quick_note_reminders SET enabled=0,revision=revision+1 WHERE note_id=?1",[&key])?;
        c.execute("UPDATE quick_note_reminder_events SET status='cancelled' WHERE note_id=?1 AND status IN ('pending','snoozed')",[&key])?;
    }
    for plan in reminders(c,&key)? {
        if input.get("reminders").is_none() && plan["schedule"]["offsetMinutes"].as_i64().is_some_and(|m|m>0) && plan["enabled"]==1 {
            save_reminder(c,&note,&plan)?;
        }
        if !text(&plan,"taskId").is_empty() {
            let task=note["tasks"].as_array().unwrap().iter().find(|t|t["id"]==plan["taskId"]);
            if task.is_none_or(|t|t["status"]=="completed" || t["status"]=="cancelled") {
                let recurring=plan["schedule"]["repeat"].as_str().is_some_and(|s|s!="once");
                let completed=task.is_some_and(|t|t["status"]=="completed");
                if !(recurring && completed && plan["nextFireAt"].as_i64().is_some_and(|t|t>now())) {
                    c.execute("UPDATE quick_note_reminders SET enabled=0 WHERE id=?1",[text(&plan,"id")])?;
                }
                c.execute("UPDATE quick_note_reminder_events SET status='cancelled' WHERE reminder_id=?1 AND status IN ('pending','snoozed')",[text(&plan,"id")])?;
            }
        }
    }
    Ok(note)
}

pub fn save(paths: &AppPaths, input: &Value, actor: &str) -> Result<Value,ManagerError> {
    let mut c=open(paths)?;
    let tx=c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let note=save_in(&tx,input,actor)?;
    tx.commit()?;
    Ok(note)
}

fn save_reminder(c: &Connection,note: &Value,plan: &Value) -> Result<(),ManagerError> {
    let key=if text(plan,"id").is_empty(){id()}else{text(plan,"id").to_owned()};
    let mut schedule=plan["schedule"].clone();
    let task_id=text(plan,"taskId");
    if !task_id.is_empty() && !note["tasks"].as_array().unwrap().iter().any(|t|t["id"]==task_id){return Err(fail("提醒关联的任务不存在"));}
    if !schedule.is_object(){return Err(fail("提醒规则无效"));}
    let repeat=schedule["repeat"].as_str().unwrap_or("once").to_owned();
    if !["once","daily","weekdays","weekly","monthly"].contains(&repeat.as_str()){return Err(fail("重复提醒规则无效"));}
    if schedule["day"].as_u64().is_some_and(|d|!(1..=31).contains(&d)) || schedule["weekdays"].as_array().is_some_and(|days|days.iter().any(|d|d.as_u64().is_none_or(|d|d>6))) {
        return Err(fail("重复提醒的日期或星期无效"));
    }
    if !schedule["maxCount"].is_null() && schedule["maxCount"].as_i64().is_none_or(|n|n<1){return Err(fail("提醒次数必须为正整数"));}
    let mut fire=plan["nextFireAt"].as_i64().or_else(||schedule["at"].as_i64()).ok_or_else(||fail("请选择具体提醒时间"))?;
    if let Some(offset)=schedule["offsetMinutes"].as_i64().filter(|m|*m>0) {
        let due=note["tasks"].as_array().unwrap().iter().find(|t|t["id"]==task_id).and_then(|t|t["dueAt"].as_i64()).ok_or_else(||fail("提前提醒需要关联任务的具体截止时间"))?;
        fire=due.checked_sub(offset.checked_mul(60_000).ok_or_else(||fail("提前提醒时间过大"))?).ok_or_else(||fail("提前提醒时间无效"))?;
        schedule["at"]=json!(fire);
    }
    if Utc.timestamp_millis_opt(fire).single().is_none(){return Err(fail("提醒时间无效"));}
    if schedule["endAt"].as_i64().is_some_and(|end|end<fire){return Err(fail("结束日期不能早于提醒时间"));}
    let old:Option<(String,String)> = c.query_row("SELECT note_id,schedule_json FROM quick_note_reminders WHERE id=?1",[&key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if old.as_ref().is_some_and(|o|o.0!=text(note,"id")){return Err(fail("不能修改其他便签的提醒"));}
    let enabled=plan["enabled"]!=false && plan["enabled"]!=0;
    if enabled && fire<=now() {return Err(fail("提醒时间必须晚于当前时间，请重新选择"));}
    schedule["repeat"]=json!(repeat.to_owned());
    if repeat=="monthly" && schedule["day"].as_u64().is_none() {
        let at=Local.timestamp_millis_opt(fire).single().unwrap();
        schedule["day"]=json!(at.day());
    }
    if !enabled || old.as_ref().is_some_and(|o|o.1!=schedule.to_string()) {
        c.execute("UPDATE quick_note_reminder_events SET status='cancelled' WHERE reminder_id=?1 AND status IN ('pending','snoozed')",[&key])?;
    }
    c.execute("INSERT INTO quick_note_reminders(id,note_id,task_id,schedule_json,next_fire_at,enabled,revision)
        VALUES(?1,?2,?3,?4,?5,?6,1) ON CONFLICT(id) DO UPDATE SET task_id=excluded.task_id,schedule_json=excluded.schedule_json,
        next_fire_at=excluded.next_fire_at,enabled=excluded.enabled,revision=revision+1",
        params![key,text(note,"id"),task_id,schedule.to_string(),fire,enabled])?;
    Ok(())
}

pub fn organize(paths: &AppPaths,input: &Value) -> Result<Value,ManagerError> {
    let mut c=open(paths)?;
    let tx=c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let key=if text(input,"id").is_empty(){id()}else{text(input,"id").to_owned()};
    let kind=text(input,"kind");
    if !["group","category"].contains(&kind) || text(input,"name").trim().is_empty(){return Err(fail("请选择目录类型并填写名称"));}
    if ["inbox","uncategorized"].contains(&key.as_str()){return Err(fail("默认目录不可修改"));}
    if let Some(target)=input["mergeInto"].as_str().filter(|s|!s.is_empty()) {
        if target==key || !tx.query_row("SELECT EXISTS(SELECT 1 FROM quick_note_folders WHERE id=?1 AND kind=?2)",params![target,kind],|r|r.get::<_,bool>(0))? {return Err(fail("合并目标目录无效"));}
        let field=if kind=="group"{"groupId"}else{"categoryId"};
        let column=if kind=="group"{"group_id"}else{"category_id"};
        for row in query(&tx,&format!("SELECT payload_json AS payloadJson FROM quick_notes WHERE {column}=?1"),&[&key])? {
            let mut note=row["payload"].clone(); note[field]=json!(target); save_in(&tx,&note,"user")?;
        }
        tx.execute("DELETE FROM quick_note_folders WHERE id=?1 AND kind=?2",params![key,kind])?;
    } else {
        tx.execute("INSERT INTO quick_note_folders(id,kind,name,aliases_json,sort_order,archived) VALUES(?1,?2,?3,?4,?5,?6)
            ON CONFLICT(id) DO UPDATE SET name=excluded.name,aliases_json=excluded.aliases_json,sort_order=excluded.sort_order,archived=excluded.archived WHERE quick_note_folders.kind=excluded.kind",
            params![key,kind,text(input,"name").trim(),input["aliases"].as_array().map(|a|json!(a)).unwrap_or(json!([])).to_string(),input["sortOrder"].as_i64().unwrap_or(0),input["archived"]==true || input["archived"]==1])?;
    }
    tx.commit()?;
    Ok(json!({"id":key}))
}

pub fn capture(paths: &AppPaths,input: &Value,model: &Value) -> Result<Value,ManagerError> {
    let c=open(paths)?;
    let request=text(input,"requestId");
    if request.is_empty(){return Err(fail("速记缺少请求标识"));}
    let content=text(input,"text").trim();
    if content.is_empty() || content.chars().count()>100_000 {return Err(fail("速记内容不能为空，且最多 100000 字"));}
    let key=id();
    c.execute("INSERT OR IGNORE INTO quick_note_captures(id,request_id,note_id,source_text,source_json,captured_at,model_json,status,error,result_json)
        VALUES(?1,?2,?3,?4,?5,?6,?7,'running','','{}')",params![key,request,input["noteId"].as_str(),content,input["source"].as_object().map(|s|json!(s)).unwrap_or(json!({})).to_string(),now(),model.to_string()])?;
    capture_state(paths,request)
}

pub fn capture_state(paths: &AppPaths,request: &str) -> Result<Value,ManagerError> {
    query(&open(paths)?,"SELECT id,request_id AS requestId,note_id AS noteId,source_text AS sourceText,source_json AS sourceJson,captured_at AS capturedAt,model_json AS modelJson,status,error,result_json AS resultJson FROM quick_note_captures WHERE request_id=?1 OR id=?1",&[&request])?.pop().ok_or_else(||fail("速记任务不存在"))
}

pub fn capture_failure(paths: &AppPaths,key: &str,status: &str,error: &str) -> Result<(),ManagerError> {
    open(paths)?.execute("UPDATE quick_note_captures SET status=?2,error=?3 WHERE id=?1 AND status='running'",params![key,status,error])?;
    Ok(())
}

// AI 新目录与便签使用同一事务，同名或别名目录优先复用，失败时一起回滚。
fn resolve_ai_folders(c: &Connection,item: &mut Value) -> Result<(),ManagerError> {
    for (field,proposal,kind,default) in [("groupId","newGroup","group","inbox"),("categoryId","newCategory","category","uncategorized")] {
        let current=text(item,field);
        let valid=c.query_row("SELECT EXISTS(SELECT 1 FROM quick_note_folders WHERE id=?1 AND kind=?2 AND archived=0)",params![current,kind],|r|r.get::<_,bool>(0))?;
        if valid && current!=default {continue;}
        let proposed=&item[proposal];
        let name=proposed.as_str().unwrap_or_else(||text(proposed,"name")).trim();
        if name.is_empty(){item[field]=json!(default);continue;}
        if name.chars().count()>60 {return Err(fail("AI 创建的分类或分组名称最多 60 字"));}
        let aliases=proposed.get("aliases").cloned().unwrap_or(json!([]));
        if !aliases.as_array().is_some_and(|a|a.len()<=30 && a.iter().all(|s|s.as_str().is_some_and(|s|!s.trim().is_empty() && s.chars().count()<=60))) {return Err(fail("AI 创建的分类或分组别名格式无效"));}
        let normalized=name.to_lowercase();
        let existing=query(c,"SELECT id,name,aliases_json AS aliasesJson FROM quick_note_folders WHERE kind=?1 AND archived=0 ORDER BY sort_order,id",&[&kind])?.into_iter().find(|f|
            text(f,"name").trim().to_lowercase()==normalized || f["aliases"].as_array().is_some_and(|a|a.iter().any(|v|v.as_str().is_some_and(|s|s.trim().to_lowercase()==normalized))));
        let key=if let Some(folder)=existing {text(&folder,"id").to_string()}else{
            let key=id();
            c.execute("INSERT INTO quick_note_folders(id,kind,name,aliases_json,sort_order,archived) VALUES(?1,?2,?3,?4,0,0)",params![key,kind,name,aliases.to_string()])?;
            key
        };
        item[field]=json!(key);
    }
    Ok(())
}

// 预览先在回滚事务中验证，便签与新目录仅在用户确认后落盘。
pub fn preview_capture(paths: &AppPaths,key: &str,result: &Value) -> Result<Value,ManagerError> {
    let mut c=open(paths)?;
    let tx=c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let status:String=tx.query_row("SELECT status FROM quick_note_captures WHERE id=?1",[key],|r|r.get(0))?;
    if status!="running"{return Err(fail("速记已取消或已处理，原文仍保留"));}
    let items=if let Some(parts)=result["notes"].as_array(){parts.clone()}else{vec![result.clone()]};
    for mut item in items {resolve_ai_folders(&tx,&mut item)?;save_in(&tx,&item,"ai")?;}
    tx.rollback()?;
    if c.execute("UPDATE quick_note_captures SET status='preview',result_json=?2 WHERE id=?1 AND status='running'",params![key,result.to_string()])?!=1{return Err(fail("速记已取消或已处理，原文仍保留"));}
    Ok(result.clone())
}

pub fn complete_capture(paths: &AppPaths,key: &str,result: &Value) -> Result<Value,ManagerError> {
    let mut c=open(paths)?;
    let tx=c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let capture=query(&tx,"SELECT note_id AS noteId,status,captured_at AS capturedAt FROM quick_note_captures WHERE id=?1",&[&key])?.pop().ok_or_else(||fail("速记任务不存在"))?;
    if capture["status"]!="running" && capture["status"]!="preview"{return Err(fail("速记已取消或已处理，原文仍保留"));}
    if let Some(parts)=result["notes"].as_array() {
        if parts.is_empty() || parts.len()>20 || !capture["noteId"].is_null(){return Err(fail("拆分便签数量或来源无效"));}
        let mut notes=Vec::new();
        for (index,part) in parts.iter().enumerate() {
            let mut item=part.clone();item["capturedAt"]=capture["capturedAt"].clone();
            resolve_ai_folders(&tx,&mut item)?;
            let note=save_in(&tx,&item,"ai")?;
            if index>0 {
                tx.execute("INSERT INTO quick_note_captures(id,request_id,note_id,source_text,source_json,captured_at,model_json,status,error,result_json)
                    SELECT ?1,?2,?3,source_text,json_set(source_json,'$.batchId',?4),captured_at,model_json,'completed','',?5 FROM quick_note_captures WHERE id=?4",
                    params![id(),format!("{key}:part:{index}"),text(&note,"id"),key,note.to_string()])?;
            }
            notes.push(note);
        }
        let mut result=notes[0].clone();result["notes"]=json!(notes);
        tx.execute("UPDATE quick_note_captures SET note_id=?2,status='completed',result_json=?3 WHERE id=?1",params![key,text(&result,"id"),result.to_string()])?;
        tx.commit()?;
        return Ok(result);
    }
    let mut item=result.clone();
    item["capturedAt"]=capture["capturedAt"].clone();
    if let Some(note_id)=capture["noteId"].as_str() {
        let old=read_note(&tx,note_id)?;
        // 重新整理已有记录只补用户没有填写的分类，不覆盖人工确认的任务和提醒。
        item["id"]=old["id"].clone(); item["revision"]=item["baseRevision"].clone();
        for field in ["title","content","type","tasks","occurredOn","color","pinned","lifecycle","worklog"] {item[field]=old[field].clone();}
        for (field,default) in [("groupId","inbox"),("categoryId","uncategorized")] {
            if old[field]!=default {item[field]=old[field].clone();}
        }
        if old["tags"].as_array().is_some_and(|tags|!tags.is_empty()){item["tags"]=old["tags"].clone();}
        item.as_object_mut().unwrap().remove("reminders");
    }
    resolve_ai_folders(&tx,&mut item)?;
    let note=save_in(&tx,&item,"ai")?;
    tx.execute("UPDATE quick_note_captures SET note_id=?2,status='completed',result_json=?3 WHERE id=?1",params![key,text(&note,"id"),note.to_string()])?;
    tx.commit()?;
    Ok(note)
}

// 每次只推进到未来一个日历触发点；唤醒后错过的多个周期合并为一次通知。
pub fn next_fire(schedule: &Value,previous: i64,after: i64) -> Option<i64> {
    let repeat=text(schedule,"repeat");
    if repeat=="once" || repeat.is_empty(){return None;}
    let anchor=Local.timestamp_millis_opt(previous).single()?;
    let today=Local.timestamp_millis_opt(after).single()?;
    let mut date=today.date_naive().max(anchor.date_naive());
    for _ in 0..370 {
        let weekday=date.weekday().num_days_from_monday();
        let matches=match repeat {
            "daily"=>true,"weekdays"=>weekday<5,
            "weekly"=>schedule["weekdays"].as_array().filter(|a|!a.is_empty()).map(|a|a.iter().any(|d|d.as_u64()==Some(weekday as u64))).unwrap_or(weekday==anchor.weekday().num_days_from_monday()),
            "monthly"=>{
                let next_month=if date.month()==12{NaiveDate::from_ymd_opt(date.year()+1,1,1)?}else{NaiveDate::from_ymd_opt(date.year(),date.month()+1,1)?};
                date.day()==(schedule["day"].as_u64().unwrap_or(anchor.day() as u64) as u32).min((next_month-Duration::days(1)).day())
            },_=>return None,
        };
        if matches {
            let naive=date.and_hms_opt(anchor.hour(),anchor.minute(),anchor.second())?;
            if let Some(at)=Local.from_local_datetime(&naive).earliest().map(|d|d.timestamp_millis()).filter(|t|*t>after && *t>previous) {
                if schedule["endAt"].as_i64().is_some_and(|end|at>end){return None;}
                return Some(at);
            }
        }
        date=date.succ_opt()?;
    }
    None
}

pub fn due_events(paths: &AppPaths,at: i64) -> Result<Vec<Value>,ManagerError> {
    let mut c=open(paths)?;
    let tx=c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for plan in query(&tx,"SELECT r.id,r.note_id AS noteId,r.task_id AS taskId,r.next_fire_at AS nextFireAt,r.schedule_json AS scheduleJson,n.payload_json AS noteJson
        FROM quick_note_reminders r JOIN quick_notes n ON n.id=r.note_id WHERE r.enabled=1 AND r.next_fire_at<=?1 AND n.lifecycle='active'",&[&at])? {
        let event=id();
        tx.execute("INSERT OR IGNORE INTO quick_note_reminder_events(id,reminder_id,note_id,scheduled_at,status,error) VALUES(?1,?2,?3,?4,'pending','')",params![event,text(&plan,"id"),text(&plan,"noteId"),plan["nextFireAt"].as_i64()])?;
        let count:i64=tx.query_row("SELECT count(*) FROM quick_note_reminder_events WHERE reminder_id=?1",[text(&plan,"id")],|r|r.get(0))?;
        let next=if plan["schedule"]["maxCount"].as_i64().is_some_and(|max|count>=max){None}else{next_fire(&plan["schedule"],plan["nextFireAt"].as_i64().unwrap(),at)};
        tx.execute("UPDATE quick_note_reminders SET next_fire_at=coalesce(?2,next_fire_at),enabled=?3 WHERE id=?1",params![text(&plan,"id"),next,next.is_some()])?;
        if next.is_some() && !text(&plan,"taskId").is_empty() {
            let mut note=plan["note"].clone();
            if let Some(task)=note["tasks"].as_array_mut().and_then(|a|a.iter_mut().find(|t|t["id"]==plan["taskId"])) {
                if task["status"]=="completed"{task["status"]=json!("pending");task["completedAt"]=Value::Null;save_in(&tx,&note,"reminder")?;}
            }
        }
    }
    let events=query(&tx,"SELECT e.id,e.note_id AS noteId,e.reminder_id AS reminderId,e.scheduled_at AS scheduledAt,n.title,n.payload_json AS noteJson,r.schedule_json AS scheduleJson FROM quick_note_reminder_events e JOIN quick_notes n ON n.id=e.note_id LEFT JOIN quick_note_reminders r ON r.id=e.reminder_id
        WHERE n.lifecycle='active' AND (e.status='pending' OR (e.status='snoozed' AND e.snoozed_until<=?1)) ORDER BY e.scheduled_at LIMIT 50",&[&at])?;
    for event in &events {tx.execute("UPDATE quick_note_reminder_events SET status='delivering' WHERE id=?1",[text(event,"id")])?;}
    tx.commit()?;
    Ok(events)
}

pub fn dispatch(paths: &AppPaths,action: &str,input: &Value) -> Result<Value,ManagerError> {
    if action.starts_with("calendar-rule-"){return crate::core::quick_notes_calendar_store::dispatch(paths,action,input);}
    match action {
        "list"=>list(paths,input), "detail"=>detail(paths,text(input,"id")),
        "folders"=>Ok(json!({"folders":query(&open(paths)?,"SELECT id,kind,name,aliases_json AS aliasesJson,sort_order AS sortOrder,archived FROM quick_note_folders WHERE (?1='' OR kind=?1) ORDER BY sort_order,name",&[&text(input,"kind")])?})),
        "save"=>save(paths,input,"user"), "organize"=>organize(paths,input),
        "capture-save"=>{
            let capture=capture_state(paths,text(input,"id"))?;
            if capture["status"]!="preview"{return Err(fail("请先完成 AI 分析，再确认保存"));}
            complete_capture(paths,text(&capture,"id"),&input["note"])
        },
        "capture-state"=>capture_state(paths,text(input,"requestId")),
        "capture-discard"=>{let c=open(paths)?;c.execute("UPDATE quick_note_captures SET status='dismissed' WHERE id=?1 AND status<>'running'",[text(input,"id")])?;Ok(json!({"ok":true}))},
        "task-update"=>{
            let mut note=detail(paths,text(input,"id"))?;
            note["revision"]=input["revision"].clone();
            let task=note["tasks"].as_array_mut().and_then(|a|a.iter_mut().find(|t|t["id"]==input["taskId"])).ok_or_else(||fail("待办不存在"))?;
            task["status"]=input["status"].clone();
            note.as_object_mut().unwrap().remove("reminders");
            save(paths,&note,"user")
        },
        "archive"|"trash"|"restore"=>{
            let mut note=detail(paths,text(input,"id"))?;
            note["revision"]=input["revision"].clone();
            note["lifecycle"]=json!(match action{"archive"=>"archived","trash"=>"trash",_=>"active"});
            note.as_object_mut().unwrap().remove("reminders"); save(paths,&note,"user")
        },
        "purge"=>{
            let c=open(paths)?;
            if c.execute("DELETE FROM quick_notes WHERE id=?1 AND lifecycle='trash'",[text(input,"id")])?!=1{return Err(fail("只能彻底删除回收站中的便签"));}
            Ok(json!({"deleted":true}))
        },
        "reminder-action"=>{
            let mut c=open(paths)?;
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let event=query(&tx,"SELECT note_id AS noteId,reminder_id AS reminderId,status FROM quick_note_reminder_events WHERE id=?1",&[&text(input,"id")])?.pop().ok_or_else(||fail("提醒记录不存在"))?;
            if text(input,"action")=="snooze" {
                let at=input["until"].as_i64().filter(|t|*t>now()).ok_or_else(||fail("请选择未来的稍后提醒时间"))?;
                let note=read_note(&tx,text(&event,"noteId"))?;
                if note["lifecycle"]!="active" || event["status"]=="cancelled"{return Err(fail("此提醒已停止，无法延后"));}
                if let Some(plan)=reminders(&tx,text(&event,"noteId"))?.iter().find(|p|p["id"]==event["reminderId"]) {
                    if !text(plan,"taskId").is_empty() && note["tasks"].as_array().unwrap().iter().find(|t|t["id"]==plan["taskId"]).is_none_or(|t|t["status"]=="completed" || t["status"]=="cancelled") {
                        return Err(fail("任务已完成或取消，不能继续稍后提醒"));
                    }
                } else {return Err(fail("提醒规则已移除"));}
                tx.execute("UPDATE quick_note_reminder_events SET status='snoozed',snoozed_until=?2,read_at=NULL,error='' WHERE id=?1",params![text(input,"id"),at])?;
            } else {
                tx.execute("UPDATE quick_note_reminder_events SET read_at=?2 WHERE id=?1",params![text(input,"id"),now()])?;
            }
            tx.commit()?; Ok(json!({"ok":true}))
        },
        "export"=>{
            let c=open(paths)?;
            let mut items=Vec::new();
            for key in input["ids"].as_array().ok_or_else(||fail("请选择导出的便签"))? {items.push(detail(paths,key.as_str().unwrap_or(""))?);}
            if input["format"]=="json"{return Ok(json!({"text":serde_json::to_string_pretty(&items)?}));}
            let mut markdown=String::new();
            for note in items {
                markdown.push_str(&format!("# {}\n\n事项日期：{}\n记录时间：{}\n\n{}\n\n",text(&note,"title"),text(&note,"occurredOn"),Utc.timestamp_millis_opt(note["createdAt"].as_i64().unwrap_or(0)).single().map(|d|d.with_timezone(&Local).to_rfc3339()).unwrap_or_default(),text(&note,"content")));
                if note["type"] == "worklog" {
                    let tracking = &note["workTracking"];
                    if let Some(label) = work_status_label(text(tracking, "status")) {
                        markdown.push_str(&format!("工作状态：{label}\n\n"));
                        if let Some(progress) = tracking["progress"].as_u64() {
                            markdown.push_str(&format!("完成进度：{progress}%\n\n"));
                        }
                    }
                }
                for task in note["tasks"].as_array().into_iter().flatten(){markdown.push_str(&format!("- [{}] {}\n",if task["status"]=="completed"{"x"}else{" "},text(task,"title")));}
                for (field,label) in [("process","处理过程"),("result","实际结果"),("blockers","问题与阻塞"),("next","下一步计划")]{if !text(&note["worklog"],field).is_empty(){markdown.push_str(&format!("\n{label}：{}\n",text(&note["worklog"],field)));}}
                let _=&c;
            }
            Ok(json!({"text":markdown}))
        },
        _=>Err(fail("未知速记接口")),
    }
}

#[cfg(test)]
mod tests;
