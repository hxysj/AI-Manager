use super::*;
struct Fixture { root: std::path::PathBuf, paths: AppPaths }
impl Fixture {fn new()->Self{let root=std::env::temp_dir().join(format!("quick-notes-test-{}",id()));let paths=crate::core::paths::resolve_app_paths(&root);initialize(&paths).unwrap();Self{root,paths}}}
impl Drop for Fixture {fn drop(&mut self){assert_eq!(self.root.parent(),Some(std::env::temp_dir().as_path()));std::fs::remove_dir_all(&self.root).ok();}}

#[test]
fn notes_preserve_creation_history_tasks_and_conflicts() {
    let f=Fixture::new();
    let first=save(&f.paths,&json!({"content":"修复登录问题","type":"worklog","tasks":[{"title":"回归测试"}]}),"user").unwrap();
    let mut edit=first.clone();edit["content"]=json!("修复并测试");
    let next=save(&f.paths,&edit,"user").unwrap();
    assert_eq!(next["createdAt"],first["createdAt"]);
    assert!(save(&f.paths,&edit,"user").is_err());
    let task=dispatch(&f.paths,"task-update",&json!({"id":next["id"],"revision":next["revision"],"taskId":next["tasks"][0]["id"],"status":"completed"})).unwrap();
    assert!(task["tasks"][0]["completedAt"].is_i64());
    assert_eq!(detail(&f.paths,text(&first,"id")).unwrap()["history"].as_array().unwrap().len(),3);
}

#[test]
fn reminders_are_persistent_idempotent_and_stop_on_completion() {
    let f=Fixture::new();let at=now()+60_000;
    let note=save(&f.paths,&json!({"content":"回归","tasks":[{"id":"t","title":"测试"}],"reminders":[{"taskId":"t","nextFireAt":at,"schedule":{"at":at,"repeat":"once"}}]}),"user").unwrap();
    assert!(due_events(&f.paths,at-1).unwrap().is_empty());
    let events=due_events(&f.paths,at+1).unwrap();assert_eq!(events.len(),1);
    assert!(due_events(&f.paths,at+2).unwrap().is_empty());
    dispatch(&f.paths,"reminder-action",&json!({"id":events[0]["id"],"action":"snooze","until":at+70_000})).unwrap();
    assert_eq!(due_events(&f.paths,at+80_000).unwrap().len(),1);
    dispatch(&f.paths,"task-update",&json!({"id":note["id"],"revision":note["revision"],"taskId":"t","status":"completed"})).unwrap();
    assert!(due_events(&f.paths,at+90_000).unwrap().is_empty());
    assert_eq!(detail(&f.paths,text(&note,"id")).unwrap()["reminders"][0]["enabled"],0);
}

#[test]
fn capture_retries_do_not_duplicate_and_cancel_keeps_original() {
    let f=Fixture::new();let input=json!({"text":"原文","requestId":"one"});
    let capture=capture(&f.paths,&input,&json!({})).unwrap();
    assert_eq!(super::capture(&f.paths,&input,&json!({})).unwrap()["id"],capture["id"]);
    let note=complete_capture(&f.paths,text(&capture,"id"),&json!({"content":"整理结果"})).unwrap();
    assert!(complete_capture(&f.paths,text(&capture,"id"),&json!({"content":"重复"})).is_err());
    assert_eq!(detail(&f.paths,text(&note,"id")).unwrap()["captures"][0]["sourceText"],"原文");
    let cancelled=super::capture(&f.paths,&json!({"text":"保留我","requestId":"cancel"}),&json!({})).unwrap();
    capture_failure(&f.paths,text(&cancelled,"id"),"cancelled","取消").unwrap();
    assert!(complete_capture(&f.paths,text(&cancelled,"id"),&json!({"content":"不应保存"})).is_err());
    assert_eq!(capture_state(&f.paths,"cancel").unwrap()["sourceText"],"保留我");
}

#[test]
fn model_text_checks_completion_and_protocols() {
    use crate::api::quick_notes::model_text;
    assert_eq!(model_text(br#"{"choices":[{"message":{"content":"hello"}}]}"#,false).unwrap(),"hello");
    assert_eq!(model_text(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\ndata: [DONE]\n",true).unwrap(),"ok");
    assert!(model_text(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n",true).is_err());
}

#[test]
fn recurrence_uses_calendar_month_end_weekdays_and_end_time() {
    let local=|year,month,day|Local.with_ymd_and_hms(year,month,day,15,0,0).single().unwrap().timestamp_millis();
    let january=local(2026,1,31);
    let february=next_fire(&json!({"repeat":"monthly","day":31}),january,january).unwrap();
    assert_eq!(february,local(2026,2,28));
    assert_eq!(next_fire(&json!({"repeat":"monthly","day":31}),february,february),Some(local(2026,3,31)));
    let friday=local(2026,11,20);
    assert_eq!(next_fire(&json!({"repeat":"weekdays"}),friday,friday),Some(local(2026,11,23)));
    assert_eq!(next_fire(&json!({"repeat":"daily","endAt":friday}),friday,friday),None);
}

#[test]
fn archived_notes_pause_reminders_restore_does_not_resend_and_bad_plans_roll_back() {
    let f=Fixture::new();let at=now()+120_000;
    assert!(save(&f.paths,&json!({"content":"不会被保存","reminders":[{"nextFireAt":now()-1,"schedule":{"repeat":"once"}}]}),"user").is_err());
    assert!(list(&f.paths,&json!({})).unwrap()["notes"].as_array().unwrap().is_empty());
    let note=save(&f.paths,&json!({"content":"测试归档","reminders":[{"nextFireAt":at,"schedule":{"repeat":"once","at":at}}]}),"user").unwrap();
    let archived=dispatch(&f.paths,"archive",&json!({"id":note["id"],"revision":note["revision"]})).unwrap();
    let restored=dispatch(&f.paths,"restore",&json!({"id":note["id"],"revision":archived["revision"]})).unwrap();
    assert!(due_events(&f.paths,at+1).unwrap().is_empty());
    assert_eq!(detail(&f.paths,text(&restored,"id")).unwrap()["reminders"][0]["enabled"],0);
}

#[test]
fn ai_classification_preserves_manual_content_tasks_dates_and_original_time() {
    let f=Fixture::new();
    let note=save(&f.paths,&json!({"title":"人工标题","content":"人工原文","occurredOn":"2026-11-12","tasks":[{"title":"人工待办"}],"tags":["保留"]}),"user").unwrap();
    let capture=capture(&f.paths,&json!({"text":"AI 原文","requestId":"classify","noteId":note["id"]}),&json!({})).unwrap();
    let updated=complete_capture(&f.paths,text(&capture,"id"),&json!({"title":"模型标题","content":"模型正文","baseRevision":note["revision"],"categoryId":"debugging","tags":["替换"]})).unwrap();
    assert_eq!(updated["content"],"人工原文");assert_eq!(updated["categoryId"],"debugging");
    assert_eq!(updated["createdAt"],note["createdAt"]);assert_eq!(updated["tasks"],note["tasks"]);
    assert_eq!(updated["occurredOn"],"2026-11-12");assert_eq!(updated["tags"],json!(["保留"]));
    assert_eq!(list(&f.paths,&json!({"search":"AI 原文"})).unwrap()["notes"].as_array().unwrap().len(),1);
}

#[test]
fn recurring_task_reopens_next_period_and_completed_task_cannot_snooze() {
    let f=Fixture::new();let at=now()+60_000;
    let note=save(&f.paths,&json!({"content":"日报","tasks":[{"id":"daily","title":"写日报"}],"reminders":[{"taskId":"daily","nextFireAt":at,"schedule":{"at":at,"repeat":"daily"}}]}),"user").unwrap();
    let events=due_events(&f.paths,at+1).unwrap();
    dispatch(&f.paths,"task-update",&json!({"id":note["id"],"revision":note["revision"],"taskId":"daily","status":"completed"})).unwrap();
    assert!(dispatch(&f.paths,"reminder-action",&json!({"id":events[0]["id"],"action":"snooze","until":at+500_000})).is_err());
    let detail=detail(&f.paths,text(&note,"id")).unwrap();
    assert_eq!(detail["reminders"][0]["enabled"],1);
    let next=detail["reminders"][0]["nextFireAt"].as_i64().unwrap();
    assert_eq!(due_events(&f.paths,next+1).unwrap().len(),1);
    assert_eq!(super::detail(&f.paths,text(&note,"id")).unwrap()["tasks"][0]["status"],"pending");
}

#[test]
fn business_backup_keeps_notes_and_rules_but_not_device_delivery_state() {
    let f=Fixture::new();
    crate::core::provider_store::initialize(&f.paths).unwrap();
    crate::core::skill_store::initialize(&f.paths).unwrap();
    crate::core::usage_store::initialize(&f.paths).unwrap();
    crate::core::rule_store::initialize(&f.paths).unwrap();
    crate::core::service_provider_store::initialize(&f.paths).unwrap();
    let at=now()+60_000;
    let note=save(&f.paths,&json!({"content":"需要备份的便签","reminders":[{"nextFireAt":at,"schedule":{"at":at,"repeat":"once"}}]}),"user").unwrap();
    due_events(&f.paths,at+1).unwrap();
    let bytes=database::backup(&f.paths).unwrap();
    let target=Fixture::new();
    database::restore_merged(&target.paths,&bytes,&serde_json::Map::from_iter([("*".into(),json!("backup"))])).unwrap();
    let restored=detail(&target.paths,text(&note,"id")).unwrap();
    assert_eq!(restored["content"],note["content"]);
    assert_eq!(restored["reminders"][0]["enabled"],0);
    assert!(list(&target.paths,&json!({})).unwrap()["events"].as_array().unwrap().is_empty());
}

#[test]
fn deadline_changes_move_relative_reminders_and_stop_old_snoozes() {
    let f=Fixture::new();let due=now()+7_200_000;
    let first=save(&f.paths,&json!({"content":"提前提醒","tasks":[{"id":"t","title":"交付","dueAt":due}],"reminders":[{"taskId":"t","nextFireAt":due,"schedule":{"at":due,"repeat":"once","offsetMinutes":15}}]}),"user").unwrap();
    let first_detail=detail(&f.paths,text(&first,"id")).unwrap();
    assert_eq!(first_detail["reminders"][0]["nextFireAt"],due-15*60_000);
    let mut edited=first.clone();edited["tasks"][0]["dueAt"]=json!(due+3_600_000);
    save(&f.paths,&edited,"user").unwrap();
    assert_eq!(detail(&f.paths,text(&first,"id")).unwrap()["reminders"][0]["nextFireAt"],due+3_600_000-15*60_000);
    assert!(due_events(&f.paths,due).unwrap().is_empty());
}

#[test]
fn split_capture_is_atomic_and_every_note_keeps_the_source() {
    let f=Fixture::new();
    let first=capture(&f.paths,&json!({"requestId":"split","text":"第一件事和第二件事","source":{"scope":"split"}}),&json!({})).unwrap();
    assert!(complete_capture(&f.paths,text(&first,"id"),&json!({"notes":[{"content":"有效"},{"content":""}]})).is_err());
    assert!(list(&f.paths,&json!({})).unwrap()["notes"].as_array().unwrap().is_empty());
    let batch=complete_capture(&f.paths,text(&first,"id"),&json!({"notes":[{"content":"第一件事"},{"content":"第二件事"}]})).unwrap();
    assert_eq!(batch["notes"].as_array().unwrap().len(),2);
    for note in batch["notes"].as_array().unwrap(){
        let detail=detail(&f.paths,text(note,"id")).unwrap();
        assert_eq!(detail["captures"][0]["sourceText"],"第一件事和第二件事");
    }
    assert!(complete_capture(&f.paths,text(&first,"id"),&json!({"notes":[{"content":"不能重复"}]})).is_err());
}
