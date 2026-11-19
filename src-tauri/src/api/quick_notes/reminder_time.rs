use regex::Regex;
use std::sync::OnceLock;

// 只识别用户直接提出的单次相对提醒，避免模型把明确时长写成“待确认”。
// 多个提醒、否定句和日历时刻仍交给模型处理，不猜测日期或补造时间。
pub(super) fn explicit_seconds(source: &str) -> Option<i64> {
    let source = source.trim();
    if source.matches("提醒").count() != 1 { return None; }
    static RELATIVE: OnceLock<Regex> = OnceLock::new();
    let pattern = RELATIVE.get_or_init(|| Regex::new(
        r"^(?:请|帮我|麻烦)?\s*(?P<before>提醒我)?\s*(?:在|再过|过)?\s*(?P<count>\d+(?:\.\d+)?|半|[一二两三四五六七八九十]{1,3})\s*(?P<unit>秒钟?|分钟?|小时|天)\s*(?:之后|以后|后)\s*(?P<after>提醒我?)?"
    ).expect("相对提醒表达式无效"));
    let matched = pattern.captures(source)?;
    if matched.name("before").is_none() && matched.name("after").is_none() { return None; }
    let digit = |value: &str| match value {
        "一" => Some(1.0), "二" | "两" => Some(2.0), "三" => Some(3.0),
        "四" => Some(4.0), "五" => Some(5.0), "六" => Some(6.0),
        "七" => Some(7.0), "八" => Some(8.0), "九" => Some(9.0),
        _ => None,
    };
    let count = &matched["count"];
    let amount = if count == "半" { 0.5 }
        else if let Some((tens, ones)) = count.split_once('十') {
            let tens = if tens.is_empty() { 1.0 } else { digit(tens)? };
            let ones = if ones.is_empty() { 0.0 } else { digit(ones)? };
            tens * 10.0 + ones
        } else { digit(count).or_else(|| count.parse::<f64>().ok())? };
    let seconds = amount * match &matched["unit"] {
        "秒" | "秒钟" => 1.0, "分" | "分钟" => 60.0,
        "小时" => 3600.0, "天" => 86400.0, _ => return None,
    };
    if !seconds.is_finite() || seconds < 1.0 || seconds >= i64::MAX as f64 || seconds.fract() != 0.0 { return None; }
    Some(seconds as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_only_explicit_single_relative_reminders() {
        for (source, seconds) in [
            ("2 分钟后提醒我吃饭", 120), ("两分钟后提醒我吃饭", 120),
            ("半小时后提醒我开会", 1800), ("请提醒我在十五分钟后喝水", 900),
            ("过 30 秒后提醒我", 30), ("1.5小时后提醒我出门", 5400),
        ] { assert_eq!(explicit_seconds(source), Some(seconds), "{source}"); }
        for source in ["明天下午提醒我", "不要两分钟后提醒我", "2分钟后开会", "2分钟前提醒过我", "2分钟后提醒我，十分钟后再提醒一次", "提醒我十几分钟后吃饭", "0分钟后提醒我"] {
            assert_eq!(explicit_seconds(source), None, "{source}");
        }
    }
}
