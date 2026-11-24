use crate::core::{error::ManagerError, paths::AppPaths, quick_notes_store::{self as notes, fail, now, text}, service_provider_store::query};
use chrono::{NaiveDate, NaiveTime};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};

pub fn initialize(c: &mut Connection) -> Result<(),ManagerError> {
    if c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='quick_note_calendar_rules')",[],|r|r.get::<_,bool>(0))? {return Ok(());}
    let tx=c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(include_str!("quick_notes_calendar_schema.sql"))?;
    save_in(&tx,&json!({"id":"default-weekend","title":"周末双休","kind":"rest","repeat":"weekly","weekdays":[5,6],"priority":0}),"system")?;
    tx.commit()?;
    Ok(())
}

pub fn list_in(c: &Connection) -> Result<Vec<Value>,ManagerError> {
    Ok(query(c,"SELECT payload_json AS payloadJson FROM quick_note_calendar_rules WHERE deleted_at IS NULL ORDER BY created_at,id",&[])?
        .into_iter().map(|row|row["payload"].clone()).collect())
}

fn date(value: &Value,field: &str,required: bool) -> Result<Option<NaiveDate>,ManagerError> {
    let raw=text(value,field);
    if raw.is_empty() && !required{return Ok(None);}
    NaiveDate::parse_from_str(raw,"%Y-%m-%d").ok().filter(|d|d.format("%Y-%m-%d").to_string()==raw)
        .map(Some).ok_or_else(||fail(format!("{field} 必须为有效的 YYYY-MM-DD 日期")))
}

pub fn save_in(c: &Connection,input: &Value,actor: &str) -> Result<Value,ManagerError> {
    if !input.is_object(){return Err(fail("日历规则必须是对象"));}
    let key=if text(input,"id").is_empty(){uuid::Uuid::new_v4().to_string()}else{text(input,"id").to_owned()};
    let previous:Option<(String,Option<i64>)>=c.query_row("SELECT payload_json,deleted_at FROM quick_note_calendar_rules WHERE id=?1",[&key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if previous.as_ref().is_some_and(|old|old.1.is_some()){return Err(fail("日历规则已删除，请重新读取规则列表"));}
    let previous=previous.map(|old|serde_json::from_str::<Value>(&old.0)).transpose()?;
    if previous.as_ref().is_some_and(|old|old["revision"]!=input["revision"]){return Err(fail("日历规则已被其他窗口修改，请重新读取后再保存"));}
    let mut rule=previous.clone().unwrap_or(json!({"id":key,"title":"","kind":"label","calendar":"solar","repeat":"once","startDate":"","endDate":"","year":null,"month":1,"day":1,"weekdays":[],"monthDays":[],"dateParity":"all","anchorDate":"","cycleWeek":0,"leapMonth":"regular","missingDay":"skip","tags":[],"priority":0,"enabled":true,"createdAt":now(),"revision":0}));
    for field in ["title","kind","calendar","repeat","startDate","endDate","startTime","endTime","year","month","day","weekdays","monthDays","dateParity","anchorDate","cycleWeek","leapMonth","missingDay","tags","priority","enabled","color","workStatus"] {
        if let Some(value)=input.get(field){rule[field]=value.clone();}
    }
    let title=text(&rule,"title").trim().to_owned();
    if title.is_empty() || title.chars().count()>80{return Err(fail("日历标签名称须为 1 至 80 字"));}
    rule["title"]=json!(title);
    let kind=text(&rule,"kind").to_owned();
    if !["label","work","rest","holiday","workday","makeup","birthday","leave"].contains(&kind.as_str()){return Err(fail("日历规则类型无效"));}
    let status=match kind.as_str(){"rest"|"holiday"=>"rest","work"|"workday"=>"work","makeup"=>text(&rule,"workStatus"),_=>""}.to_owned();
    if kind=="makeup" && !["rest","work"].contains(&status.as_str()){return Err(fail("调休规则必须明确选择补班或补假休息"));}
    rule["workStatus"]=json!(status);
    if input.get("color").is_none() && previous.is_none() {
        rule["color"]=json!(match kind.as_str(){"work"=>"#14213a","rest"=>"#579b80","holiday"=>"#df6676","workday"=>"#cc893d","makeup"=>if rule["workStatus"]=="rest"{"#8b74bc"}else{"#cc893d"},"birthday"=>"#ad72ae","leave"=>"#467da8",_=>"#568bb6"});
    }
    let color=text(&rule,"color");
    if color.len()!=7 || !color.starts_with('#') || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit){return Err(fail("规则颜色必须为 #RRGGBB"));}
    if !rule["enabled"].is_boolean() || rule["priority"].as_i64().is_none_or(|n|!(0..=100).contains(&n)){return Err(fail("启用状态或优先级无效（优先级 0 至 100）"));}
    if !rule["tags"].as_array().is_some_and(|a|a.len()<=20 && a.iter().all(|v|v.as_str().is_some_and(|s|!s.trim().is_empty() && s.chars().count()<=40))){return Err(fail("规则标签最多 20 项，每项 1 至 40 字"));}
    let calendar=text(&rule,"calendar");
    let repeat=text(&rule,"repeat");
    if !["solar","lunar"].contains(&calendar) || !["once","weekly","monthly","yearly","alternating"].contains(&repeat){return Err(fail("日历或重复方式无效"));}
    if !["all","odd","even"].contains(&text(&rule,"dateParity")) || !["regular","leap","both"].contains(&text(&rule,"leapMonth")) || !["skip","last-day"].contains(&text(&rule,"missingDay")){return Err(fail("单双号、闰月或缺日处理无效"));}
    let start=date(&rule,"startDate",repeat=="once" && calendar=="solar")?;
    let end=date(&rule,"endDate",false)?;
    if end.is_some() && start.is_none() || start.zip(end).is_some_and(|(a,b)|a>b){return Err(fail("结束日期不得早于开始日期，且须有开始日期"));}
    if ["weekly","alternating"].contains(&repeat) {
        if calendar!="solar"{return Err(fail("按星期的规则使用公历"));}
        if !rule["weekdays"].as_array().is_some_and(|a|!a.is_empty() && a.len()<=7 && a.iter().all(|d|d.as_u64().is_some_and(|n|n<7))){return Err(fail("请选择星期，周一至周日使用 0 至 6"));}
        if repeat=="alternating" {
            date(&rule,"anchorDate",true)?;
            if ![Some(0),Some(1)].contains(&rule["cycleWeek"].as_i64()){return Err(fail("大小周规则的 cycleWeek 必须为 0 或 1"));}
        }
    }
    if repeat=="yearly" || repeat=="once" && calendar=="lunar" {
        let month=rule["month"].as_u64().filter(|n|(1..=12).contains(n)).ok_or_else(||fail("请选择 1 至 12 月"))?;
        let day=rule["day"].as_u64().ok_or_else(||fail("生日或标签日期无效"))?;
        let max=if calendar=="lunar"{30}else{[31,29,31,30,31,30,31,31,30,31,30,31][month as usize-1]};
        if day<1 || day>max{return Err(fail("该月份的日期无效"));}
        if repeat=="once" && rule["year"].as_i64().is_none_or(|n|!(1900..=2100).contains(&n)){return Err(fail("指定农历日期需填写 1900 至 2100 年"));}
    }
    if repeat=="monthly" && !rule["monthDays"].as_array().is_some_and(|a|!a.is_empty() && a.len()<=31 && a.iter().all(|v|v.as_u64().is_some_and(|d|d>=1 && d<=if calendar=="lunar"{30}else{31}))){return Err(fail("每月休息日期须为有效日期数组"));}
    if kind == "leave" {
        if calendar != "solar" || repeat != "once" || rule["dateParity"] != "all" {
            return Err(fail("请假须使用指定的公历日期或日期范围"));
        }
        let parse_time = |field| -> Result<Option<NaiveTime>, ManagerError> {
            match rule.get(field) {
                None => Ok(None),
                Some(Value::String(value)) if value.is_empty() => Ok(None),
                Some(Value::String(value)) => NaiveTime::parse_from_str(value, "%H:%M")
                    .ok()
                    .filter(|time| time.format("%H:%M").to_string() == *value)
                    .map(Some)
                    .ok_or_else(|| fail("请假时间必须为有效的 HH:mm")),
                _ => Err(fail("请假时间必须为有效的 HH:mm")),
            }
        };
        let (from, to) = (parse_time("startTime")?, parse_time("endTime")?);
        match (from, to) {
            (None, None) => rule["workStatus"] = json!("rest"),
            (Some(from), Some(to)) if end.is_some_and(|end| Some(end) > start) || from < to => {}
            _ => return Err(fail("请填写开始和结束时间，且结束时间须晚于开始时间")),
        }
    } else if let Some(fields) = rule.as_object_mut() {
        fields.remove("startTime");
        fields.remove("endTime");
    }
    rule["revision"]=json!(previous.as_ref().and_then(|p|p["revision"].as_i64()).unwrap_or(0)+1);
    rule["updatedAt"]=json!(now());
    c.execute("INSERT INTO quick_note_calendar_rules(id,revision,payload_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?5)
        ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,payload_json=excluded.payload_json,updated_at=excluded.updated_at",
        params![key,rule["revision"].as_i64(),rule.to_string(),rule["createdAt"].as_i64(),rule["updatedAt"].as_i64()])?;
    c.execute("INSERT INTO quick_note_calendar_rule_revisions(id,rule_id,operation,actor,before_json,after_json,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![uuid::Uuid::new_v4().to_string(),key,if previous.is_some(){"update"}else{"create"},actor,previous.unwrap_or(Value::Null).to_string(),rule.to_string(),now()])?;
    Ok(rule)
}

fn delete_in(c: &Connection,input: &Value,actor: &str) -> Result<Value,ManagerError> {
    let old: String=c.query_row("SELECT payload_json FROM quick_note_calendar_rules WHERE id=?1 AND deleted_at IS NULL",[text(input,"id")],|r|r.get(0)).optional()?.ok_or_else(||fail("日历规则不存在或已删除"))?;
    let old:Value=serde_json::from_str(&old)?;
    if input["revision"]!=old["revision"]{return Err(fail("日历规则已被修改，请重新读取"));}
    c.execute("UPDATE quick_note_calendar_rules SET deleted_at=?2,updated_at=?2,revision=revision+1 WHERE id=?1",params![text(input,"id"),now()])?;
    c.execute("INSERT INTO quick_note_calendar_rule_revisions(id,rule_id,operation,actor,before_json,after_json,created_at) VALUES(?1,?2,'delete',?3,?4,'null',?5)",
        params![uuid::Uuid::new_v4().to_string(),text(input,"id"),actor,old.to_string(),now()])?;
    Ok(json!({"id":input["id"],"deleted":true}))
}

// 手动和模型操作共用一份事务及 revision 校验，批量规则全部成功后才提交。
pub fn batch_in(c: &Connection,operations: &Value,actor: &str) -> Result<Vec<Value>,ManagerError> {
    let operations=operations.as_array().filter(|a|!a.is_empty() && a.len()<=100).ok_or_else(||fail("每次须提交 1 至 100 项日历变更"))?;
    operations.iter().map(|operation|match text(operation,"operation"){
        "save"=>save_in(c,&operation["rule"],actor),
        "delete"=>delete_in(c,&operation["rule"],actor),
        _=>Err(fail("日历操作只能为 save 或 delete"))
    }).collect()
}

pub fn dispatch(paths: &AppPaths,action: &str,input: &Value) -> Result<Value,ManagerError> {
    let mut c=notes::open(paths)?;
    if action=="calendar-rule-list" {return Ok(json!({"rules":list_in(&c)?}));}
    if action=="calendar-rule-history" {return Ok(json!({"history":query(&c,"SELECT id,operation,actor,before_json AS beforeJson,after_json AS afterJson,created_at AS createdAt FROM quick_note_calendar_rule_revisions WHERE rule_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 100",&[&text(input,"id")])?}));}
    let tx=c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result=match action {
        "calendar-rule-save"=>save_in(&tx,input,"user")?,
        "calendar-rule-delete"=>delete_in(&tx,input,"user")?,
        "calendar-rule-batch"=>json!({"rules":batch_in(&tx,&input["operations"],"ai")?}),
        _=>return Err(fail("日历规则操作不存在"))
    };
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests;
