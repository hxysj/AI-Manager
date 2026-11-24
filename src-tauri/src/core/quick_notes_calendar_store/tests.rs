use super::*;
#[test]
fn defaults_and_revision_conflicts() {
    let mut c=Connection::open_in_memory().unwrap();
    initialize(&mut c).unwrap();
    let rules=list_in(&c).unwrap();
    assert_eq!(rules.len(),1);
    assert_eq!(rules[0]["weekdays"],json!([5,6]));
    let holiday=save_in(&c,&json!({"title":"假期","kind":"holiday","startDate":"2026-11-20"}),"user").unwrap();
    assert_eq!(holiday["color"],"#df6676");
    let work=save_in(&c,&json!({"title":"正常上班","kind":"work","repeat":"weekly","weekdays":[0,1,2,3,4]}),"user").unwrap();
    assert_eq!(work["workStatus"],"work");
    assert_eq!(work["color"],"#14213a");
    let makeup=save_in(&c,&json!({"title":"假期补班","kind":"workday","startDate":"2026-11-21"}),"user").unwrap();
    assert_eq!(makeup["workStatus"],"work");
    assert_ne!(makeup["color"],work["color"]);
    assert!(save_in(&c,&json!({"id":holiday["id"],"revision":0,"title":"旧版本"}),"user").is_err());
    assert!(save_in(&c,&json!({"title":"错误日期","startDate":"2026-02-30"}),"user").is_err());
    assert!(save_in(&c,&json!({"title":"未指定调休状态","kind":"makeup","startDate":"2026-11-20"}),"user").is_err());
}

#[test]
fn leave_supports_single_days_and_cross_day_ranges() {
    let mut c = Connection::open_in_memory().unwrap();
    initialize(&mut c).unwrap();
    let all_day = save_in(&c, &json!({
        "title":"请假","kind":"leave","startDate":"2026-11-25","endDate":"2026-11-27"
    }), "user").unwrap();
    assert_eq!(all_day["workStatus"], "rest");
    let timed = save_in(&c, &json!({
        "id":all_day["id"],"revision":all_day["revision"],"startTime":"14:00","endTime":"10:00"
    }), "ai").unwrap();
    assert_eq!(timed["workStatus"], "");
    assert_eq!(timed["endDate"], "2026-11-27");
    assert_eq!(timed["endTime"], "10:00");
    assert_eq!(list_in(&c).unwrap().len(), 2);
    let all_day_again = save_in(&c, &json!({
        "id":timed["id"],"revision":timed["revision"],"startTime":"","endTime":""
    }), "user").unwrap();
    assert_eq!(all_day_again["workStatus"], "rest");
    for (from, to) in [("14:00", "10:00"), ("09:00", ""), ("9:00", "10:00"), ("12:60", "15:00")] {
        assert!(save_in(&c, &json!({
            "title":"请假","kind":"leave","startDate":"2026-11-25","startTime":from,"endTime":to
        }), "user").is_err());
    }
    let single_day = save_in(&c, &json!({
        "title":"请假","kind":"leave","startDate":"2026-11-28","startTime":"09:00","endTime":"10:00"
    }), "user").unwrap();
    assert_eq!(single_day["workStatus"], "");
}
