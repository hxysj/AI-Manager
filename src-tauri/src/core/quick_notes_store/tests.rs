use super::*;
struct Fixture { root: std::path::PathBuf, paths: AppPaths }
impl Fixture {fn new()->Self{let root=std::env::temp_dir().join(format!("quick-notes-test-{}",id()));let paths=crate::core::paths::resolve_app_paths(&root);initialize(&paths).unwrap();Self{root,paths}}}
impl Drop for Fixture {fn drop(&mut self){assert_eq!(self.root.parent(),Some(std::env::temp_dir().as_path()));std::fs::remove_dir_all(&self.root).ok();}}

#[test]
fn date_fields_require_real_canonical_dates_and_report_the_invalid_field() {
    let f=Fixture::new();
    for field in ["occurredOn","dueDate"] {
        for date in [json!("今天"),json!("2026/11/18"),json!("2026-1-2"),json!("2026-02-29"),json!("2026-11-18T15:00:00+08:00"),json!("null"),json!(20261118)] {
            let mut input=json!({"content":"日期校验","tasks":[{"title":"处理事项"}]});
            if field=="occurredOn"{input[field]=date;}else{input["tasks"][0][field]=date;}
            let error=save(&f.paths,&input,"user").unwrap_err().to_string();
            assert!(error.contains(field),"{error}");
            assert!(list(&f.paths,&json!({})).unwrap()["notes"].as_array().unwrap().is_empty());
        }
    }
    for date in [Value::Null,json!(""),json!("2026-11-18"),json!("2028-02-29")] {
        assert!(save(&f.paths,&json!({"content":"有效日期","occurredOn":date,"tasks":[{"title":"处理事项","dueDate":date}]}),"user").is_ok());
    }
}

#[test]
fn ai_folders_reuse_aliases_create_atomically_and_preserve_manual_classification() {
    let f=Fixture::new();
    let group=organize(&f.paths,&json!({"kind":"group","name":"AI Manager","aliases":["桌宠"]})).unwrap();
    let capture=capture(&f.paths,&json!({"requestId":"folders","text":"整理桌宠发布"}),&json!({})).unwrap();
    let result=complete_capture(&f.paths,text(&capture,"id"),&json!({"notes":[
        {"content":"第一项","newGroup":{"name":"桌宠"},"newCategory":{"name":"客户交付","aliases":["交付"]}},
        {"content":"第二项","newGroup":{"name":"AI Manager"},"newCategory":{"name":"交付"}}
    ]})).unwrap();
    let notes=result["notes"].as_array().unwrap();
    assert_eq!(notes[0]["groupId"],group["id"]);
    assert_eq!(notes[0]["categoryId"],notes[1]["categoryId"]);
    let folders=dispatch(&f.paths,"folders",&json!({"kind":"category"})).unwrap();
    assert_eq!(folders["folders"].as_array().unwrap().iter().filter(|f|f["name"]=="客户交付").count(),1);
    assert!(folders["folders"].as_array().unwrap().iter().all(|f|f["kind"]=="category"));
    let cancelled=super::capture(&f.paths,&json!({"requestId":"rollback-folder","text":"无效结果"}),&json!({})).unwrap();
    assert!(complete_capture(&f.paths,text(&cancelled,"id"),&json!({"content":"","newGroup":{"name":"不应保存"}})).is_err());
    assert!(!dispatch(&f.paths,"folders",&json!({})).unwrap()["folders"].as_array().unwrap().iter().any(|f|f["name"]=="不应保存"));
    let original=&notes[0];
    let classify=super::capture(&f.paths,&json!({"requestId":"keep-folder","text":"人工已确认","noteId":original["id"]}),&json!({})).unwrap();
    let updated=complete_capture(&f.paths,text(&classify,"id"),&json!({"baseRevision":original["revision"],"newGroup":{"name":"不覆盖人工"},"newCategory":{"name":"不覆盖人工"}})).unwrap();
    assert_eq!(updated["groupId"],original["groupId"]);
    assert_eq!(updated["categoryId"],original["categoryId"]);
    assert!(!dispatch(&f.paths,"folders",&json!({})).unwrap()["folders"].as_array().unwrap().iter().any(|f|f["name"]=="不覆盖人工"));
}

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
fn work_tracking_preserves_history_and_manual_progress() {
    let fixture = Fixture::new();
    let capture = capture(
        &fixture.paths,
        &json!({"requestId":"progress","text":"需要修复同步问题"}),
        &json!({}),
    ).unwrap();
    let due_at = now() + 3_600_000;
    let suggestion = json!({
        "type":"worklog","content":"修复同步问题",
        "tasks":[{"id":"test","title":"回归测试","dueAt":due_at}],
        "reminders":[{"taskId":"test","nextFireAt":due_at - 900_000,"schedule":{"repeat":"once","offsetMinutes":15}}],
        "workTracking":{"status":"planned","progress":null}
    });
    preview_capture(&fixture.paths, text(&capture, "id"), &suggestion).unwrap();
    let original = dispatch(&fixture.paths, "capture-save", &json!({
        "id":capture["id"],"note":suggestion
    })).unwrap();
    assert_eq!(original["workTracking"], suggestion["workTracking"]);
    let key = text(&original, "id");
    let reminders_before = detail(&fixture.paths, key).unwrap()["reminders"].clone();
    let change = json!({
        "id":key,"revision":original["revision"],
        "workTracking":{"status":"in_progress","progress":40},
        "progressNote":"已修复接口，等待联调"
    });
    let updated = save(&fixture.paths, &change, "user").unwrap();
    assert_eq!(updated["content"], original["content"]);
    assert_eq!(updated["tasks"], original["tasks"]);
    assert!(updated.get("progressNote").is_none());
    let updated_detail = detail(&fixture.paths, key).unwrap();
    assert_eq!(updated_detail["reminders"], reminders_before);
    let entry = &updated_detail["history"][0];
    assert_eq!(entry["operation"], "progress");
    assert_eq!(entry["before"]["workTracking"], original["workTracking"]);
    assert_eq!(entry["after"]["workTracking"], change["workTracking"]);
    assert_eq!(entry["after"]["progressNote"], change["progressNote"]);

    let old = save(&fixture.paths, &json!({"content":"旧工作记录","type":"worklog"}), "user").unwrap();
    save(&fixture.paths, &json!({"content":"普通便签"}), "user").unwrap();
    for (status, note_id) in [("in_progress", &updated["id"]), ("untracked", &old["id"])] {
        let filtered = list(&fixture.paths, &json!({"workStatus":status,"limit":1})).unwrap();
        assert_eq!(&filtered["notes"][0]["id"], note_id);
    }
    for (status, progress) in [("in_progress", json!(100)), ("completed", json!(40)), ("in_progress", json!(40.5))] {
        let invalid = json!({"id":key,"revision":updated["revision"],"workTracking":{"status":status,"progress":progress}});
        assert!(save(&fixture.paths, &invalid, "user").is_err());
    }
    let classify = super::capture(&fixture.paths, &json!({
        "requestId":"classify-progress","text":"修复同步问题","noteId":key
    }), &json!({})).unwrap();
    let classified = complete_capture(&fixture.paths, text(&classify, "id"), &json!({
        "baseRevision":updated["revision"],"categoryId":"development",
        "workTracking":{"status":"completed","progress":100}
    })).unwrap();
    assert_eq!(classified["workTracking"], updated["workTracking"]);
    let completed = save(&fixture.paths, &json!({
        "id":key,"revision":classified["revision"],"workTracking":{"status":"completed","progress":100}
    }), "user").unwrap();
    assert_eq!(completed["workTracking"]["progress"], 100);
}

#[test]
fn quick_reminder_appends_without_rewriting_existing_plans() {
    let f = Fixture::new();
    let due_at = now() + 3_600_000;
    let note = save(&f.paths, &json!({
        "content": "卡片提醒",
        "tasks": [{"id": "task", "title": "复核", "dueAt": due_at}],
        "reminders": [{
            "taskId": "task", "nextFireAt": due_at - 600_000,
            "schedule": {"repeat": "once", "offsetMinutes": 10}
        }]
    }), "user").unwrap();
    let key = text(&note, "id");
    let existing = detail(&f.paths, key).unwrap()["reminders"][0].clone();
    let invalid = json!({"id": key, "revision": note["revision"], "appendReminder": {"at": now() - 1000}});
    assert!(dispatch(&f.paths, "save", &invalid).is_err());
    let quick_at = now() + 120_000;
    let append = json!({"id": key, "revision": note["revision"], "appendReminder": {"at": quick_at}});
    let updated = dispatch(&f.paths, "save", &append).unwrap();
    assert!(dispatch(&f.paths, "save", &append).is_err());
    assert_eq!(updated["tasks"], note["tasks"]);
    let plans = detail(&f.paths, key).unwrap()["reminders"].clone();
    assert_eq!(plans.as_array().unwrap().len(), 2);
    assert_eq!(plans[0]["nextFireAt"], quick_at);
    assert_eq!(plans[0]["schedule"]["repeat"], "once");
    assert_eq!(plans[1], existing);
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
fn enabled_reminders_must_be_strictly_future_for_creation_changes_and_offsets() {
    let f=Fixture::new();
    for at in [now()-60_000,now()] {
        let error=save(&f.paths,&json!({"content":"无效提醒","reminders":[{"nextFireAt":at,"schedule":{"at":at,"repeat":"once"}}]}),"user").unwrap_err().to_string();
        assert!(error.contains("必须晚于当前时间"));
        assert!(list(&f.paths,&json!({})).unwrap()["notes"].as_array().unwrap().is_empty());
    }
    let at=now()+3_600_000;
    let note=save(&f.paths,&json!({"content":"未来提醒","tasks":[{"id":"task","title":"交付","dueAt":now()+60_000}],"reminders":[{"nextFireAt":at,"schedule":{"at":at,"repeat":"once"}}]}),"user").unwrap();
    let plan=detail(&f.paths,text(&note,"id")).unwrap()["reminders"][0].clone();
    // 即使保留原规则，仅改触发点为当前时间，也必须拒绝并回滚修订。
    let mut changed=note.clone();let mut past=plan.clone();past["nextFireAt"]=json!(now());changed["reminders"]=json!([past]);
    assert!(save(&f.paths,&changed,"user").unwrap_err().to_string().contains("必须晚于当前时间"));
    let unchanged=detail(&f.paths,text(&note,"id")).unwrap();
    assert_eq!(unchanged["revision"],note["revision"]);assert_eq!(unchanged["reminders"][0]["nextFireAt"],at);
    let mut offset=note.clone();offset["reminders"]=json!([{"taskId":"task","nextFireAt":at,"schedule":{"at":at,"repeat":"once","offsetMinutes":5}}]);
    assert!(save(&f.paths,&offset,"user").unwrap_err().to_string().contains("必须晚于当前时间"));
    past["enabled"]=json!(false);changed["reminders"]=json!([past]);
    save(&f.paths,&changed,"user").unwrap();
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
fn summary_capture_listing_keeps_source_note_ids_until_completion() {
    let f = Fixture::new();
    let source = json!({"type": "summary", "noteIds": ["first", "second"]});
    let job = capture(
        &f.paths,
        &json!({"text": "小结输入", "requestId": "summary", "source": source}),
        &json!({}),
    )
    .unwrap();

    let captures = list(&f.paths, &json!({})).unwrap()["captures"].clone();
    assert_eq!(captures[0]["status"], "running");
    assert_eq!(captures[0]["source"], source);

    complete_capture(
        &f.paths,
        text(&job, "id"),
        &json!({"content": "工作小结", "type": "worklog"}),
    )
    .unwrap();
    assert_eq!(list(&f.paths, &json!({})).unwrap()["captures"], json!([]));
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

#[test]
fn ai_preview_validates_without_writing_and_confirmation_preserves_source() {
    let f=Fixture::new();
    let raw="修复分值同步问题并补充验证";
    let capture=capture(&f.paths,&json!({"requestId":"preview","text":raw}),&json!({})).unwrap();
    let key=text(&capture,"id");
    assert!(dispatch(&f.paths,"capture-save",&json!({"id":key,"note":{"content":"未经分析"}})).is_err());
    assert!(preview_capture(&f.paths,key,&json!({"content":"无效预览","occurredOn":"今天","newGroup":{"name":"预览项目"}})).is_err());
    assert_eq!(capture_state(&f.paths,key).unwrap()["status"],"running");
    let draft=json!({"title":"模型标题","content":"整理正文","newGroup":{"name":"预览项目"},"tasks":[{"id":"task","title":"回归验证","status":"pending"}]});
    preview_capture(&f.paths,key,&draft).unwrap();
    let state=capture_state(&f.paths,key).unwrap();
    assert_eq!(state["status"],"preview");assert_eq!(state["result"],draft);
    assert!(list(&f.paths,&json!({})).unwrap()["notes"].as_array().unwrap().is_empty());
    assert!(!dispatch(&f.paths,"folders",&json!({})).unwrap()["folders"].as_array().unwrap().iter().any(|folder|folder["name"]=="预览项目"));
    assert!(dispatch(&f.paths,"capture-save",&json!({"id":key,"note":{"content":""}})).is_err());
    assert_eq!(capture_state(&f.paths,key).unwrap()["status"],"preview");
    let mut edited=draft.clone();edited["title"]=json!("确认后的标题");
    let note=dispatch(&f.paths,"capture-save",&json!({"id":key,"note":edited})).unwrap();
    assert_eq!(note["title"],"确认后的标题");assert_eq!(note["createdAt"],capture["capturedAt"]);
    assert_eq!(detail(&f.paths,text(&note,"id")).unwrap()["captures"][0]["sourceText"],raw);
    assert_eq!(capture_state(&f.paths,key).unwrap()["status"],"completed");
    assert!(dispatch(&f.paths,"capture-save",&json!({"id":key,"note":draft})).is_err());
    let dismissed=super::capture(&f.paths,&json!({"requestId":"dismiss-preview","text":"保留原文"}),&json!({})).unwrap();
    preview_capture(&f.paths,text(&dismissed,"id"),&json!({"content":"待确认"})).unwrap();
    dispatch(&f.paths,"capture-discard",&json!({"id":dismissed["id"]})).unwrap();
    assert!(dispatch(&f.paths,"capture-save",&json!({"id":dismissed["id"],"note":{"content":"不应保存"}})).is_err());
}
