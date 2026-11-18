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
    assert!(save_in(&c,&json!({"id":holiday["id"],"revision":0,"title":"旧版本"}),"user").is_err());
    assert!(save_in(&c,&json!({"title":"错误日期","startDate":"2026-02-30"}),"user").is_err());
    assert!(save_in(&c,&json!({"title":"未指定调休状态","kind":"makeup","startDate":"2026-11-20"}),"user").is_err());
}
