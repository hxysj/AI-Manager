use super::{failure, ManagerError};
use serde_json::{json, Value};

// 语言名称与提示模板依据 NiuTrans/LMT 的 src/utils.py，范围限定为模型卡声明的 60 种语言。
const LANGUAGES: &[(&str, &str, &str, &str)] = &[
    ("zh", "Chinese", "简体中文", "cmn"),
    ("en", "English", "English", "eng"),
    ("ja", "Japanese", "日本語", "jpn"),
    ("ko", "Korean", "한국어", "kor"),
    ("fr", "French", "Français", "fra"),
    ("de", "German", "Deutsch", "deu"),
    ("es", "Spanish", "Español", "spa"),
    ("ar", "Arabic", "阿拉伯语", "ara"),
    ("it", "Italian", "意大利语", "ita"),
    ("nl", "Dutch", "荷兰语", "nld"),
    ("pl", "Polish", "波兰语", "pol"),
    ("pt", "Portuguese", "葡萄牙语", "por"),
    ("ru", "Russian", "俄语", "rus"),
    ("tr", "Turkish", "土耳其语", "tur"),
    ("bg", "Bulgarian", "保加利亚语", "bul"),
    ("bn", "Bengali", "孟加拉语", "ben"),
    ("cs", "Czech", "捷克语", "ces"),
    ("da", "Danish", "丹麦语", "dan"),
    ("el", "Modern Greek", "希腊语", "ell"),
    ("fa", "Persian", "波斯语", "pes"),
    ("fi", "Finnish", "芬兰语", "fin"),
    ("hi", "Hindi", "印地语", "hin"),
    ("hu", "Hungarian", "匈牙利语", "hun"),
    ("id", "Indonesian", "印度尼西亚语", "ind"),
    ("nb", "Norwegian Bokmål", "挪威语（博克马尔）", "nob"),
    ("ro", "Romanian", "罗马尼亚语", "ron"),
    ("sk", "Slovak", "斯洛伐克语", "slk"),
    ("sv", "Swedish", "瑞典语", "swe"),
    ("th", "Thai", "泰语", "tha"),
    ("uk", "Ukrainian", "乌克兰语", "ukr"),
    ("vi", "Vietnamese", "越南语", "vie"),
    ("am", "Amharic", "阿姆哈拉语", "amh"),
    ("az", "Azerbaijani", "阿塞拜疆语", "aze"),
    ("bo", "Tibetan", "藏语", "bod"),
    ("he", "Modern Hebrew", "希伯来语", "heb"),
    ("hr", "Croatian", "克罗地亚语", "hrv"),
    ("hy", "Armenian", "亚美尼亚语", "hye"),
    ("is", "Icelandic", "冰岛语", "isl"),
    ("jv", "Javanese", "爪哇语", "jav"),
    ("ka", "Georgian", "格鲁吉亚语", "kat"),
    ("kk", "Kazakh", "哈萨克语", "kaz"),
    ("km", "Central Khmer", "高棉语", "khm"),
    ("ky", "Kirghiz", "吉尔吉斯语", "kir"),
    ("lo", "Lao", "老挝语", "lao"),
    ("mvf", "Inner Mongolian", "内蒙古语", "mvf"),
    ("mr", "Marathi", "马拉地语", "mar"),
    ("ms", "Malay", "马来语", "zlm"),
    ("my", "Burmese", "缅甸语", "mya"),
    ("ne", "Nepali", "尼泊尔语", "nep"),
    ("ps", "Pashto", "普什图语", "pus"),
    ("si", "Sinhala", "僧伽罗语", "sin"),
    ("sw", "Swahili", "斯瓦希里语", "swh"),
    ("ta", "Tamil", "泰米尔语", "tam"),
    ("te", "Telugu", "泰卢固语", "tel"),
    ("tg", "Tajik", "塔吉克语", "tgk"),
    ("tl", "Tagalog", "他加禄语", "tgl"),
    ("ug", "Uighur", "维吾尔语", "uig"),
    ("ur", "Urdu", "乌尔都语", "urd"),
    ("uz", "Uzbek", "乌兹别克语", "uzb"),
    ("yue", "Yue Chinese", "粤语", "yue"),
];

pub(super) fn languages() -> Value {
    json!(LANGUAGES
        .iter()
        .map(|item| json!({
            "code": item.0, "name": item.1, "label": item.2
        }))
        .collect::<Vec<_>>())
}

fn language(
    value: &str,
) -> Result<&'static (&'static str, &'static str, &'static str, &'static str), ManagerError> {
    LANGUAGES
        .iter()
        .find(|item| {
            item.0.eq_ignore_ascii_case(value)
                || item.1.eq_ignore_ascii_case(value)
                || item.2 == value
        })
        .ok_or_else(|| failure(format!("LMT 不支持此语言：{value}，请在翻译配置中重新选择")))
}

pub(crate) fn check_languages(source: &str, target: &str) -> Result<(), ManagerError> {
    if !source.is_empty() && source != "auto" {
        language(source)?;
    }
    language(target)?;
    Ok(())
}

pub(crate) fn plan(
    text: &str,
    source: &str,
    target: &str,
) -> Result<Vec<(String, String)>, ManagerError> {
    let target = language(target)?;
    let source = if source.is_empty() || source == "auto" {
        // 短句或未知语言检测不确定时，由翻译模型根据原文自动判断，不要求用户补选。
        whatlang::detect(text)
            .filter(|info| info.is_reliable())
            .and_then(|info| LANGUAGES.iter().find(|item| item.3 == info.lang().code()))
            .map(|item| item.0)
            .unwrap_or("auto")
    } else {
        language(source)?.0
    };
    if source == target.0 {
        return Ok(Vec::new());
    }
    // 官方 234 个方向以中文、英文为中心，其他语言之间通过英语中转。
    let pairs = if !["zh", "en"].contains(&source) && !["zh", "en"].contains(&target.0) {
        vec![(source, "en"), ("en", target.0)]
    } else {
        vec![(source, target.0)]
    };
    Ok(pairs
        .into_iter()
        .map(|(source, target)| (source.into(), target.into()))
        .collect())
}

pub(super) fn prompt(text: &str, source: &str, target: &str) -> Result<String, ManagerError> {
    let target = language(target)?.1;
    if source.is_empty() || source == "auto" {
        return Ok(format!(
            "Translate the following text into {target}:\n{text}\n{target}:"
        ));
    }
    let source = language(source)?.1;
    Ok(format!(
        "Translate the following text from {source} into {target}:\n{source}: {text}\n{target}:"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_official_template_and_declared_language_names() {
        assert_eq!(LANGUAGES.len(), 60);
        assert_eq!(
            prompt("Hello!", "en", "简体中文").unwrap(),
            "Translate the following text from English into Chinese:\nEnglish: Hello!\nChinese:"
        );
        assert!(prompt("שלום", "he", "en")
            .unwrap()
            .contains("Modern Hebrew: שלום"));
        assert!(check_languages("auto", "繁體中文").is_err());
        assert!(check_languages("xx", "zh").is_err());
        assert_eq!(
            prompt("Hello, world!", "auto", "zh").unwrap(),
            "Translate the following text into Chinese:\nHello, world!\nChinese:"
        );
    }

    #[test]
    fn detects_sources_and_routes_only_supported_directions() {
        for (text, source) in [
            (
                "This is a local translation model. Please translate the whole sentence.",
                "en",
            ),
            ("这是一个用于离线翻译的本地模型。", "zh"),
            ("これは日本語の翻訳テストです。", "ja"),
            ("이것은 한국어 번역 테스트입니다.", "ko"),
        ] {
            assert_eq!(
                plan(text, "auto", if source == "zh" { "en" } else { "zh" }).unwrap()[0].0,
                source
            );
        }
        assert_eq!(
            plan("Bonjour", "fr", "ja").unwrap(),
            vec![("fr".into(), "en".into()), ("en".into(), "ja".into())]
        );
        assert!(plan("你好", "zh", "zh").unwrap().is_empty());
        assert_eq!(
            plan("123", "auto", "zh").unwrap(),
            vec![("auto".into(), "zh".into())]
        );
        assert_eq!(
            plan("Hello, world!", "auto", "zh").unwrap(),
            vec![("auto".into(), "zh".into())]
        );
        assert_eq!(
            plan("Hello, world!", "auto", "ja").unwrap(),
            vec![("auto".into(), "en".into()), ("en".into(), "ja".into())]
        );
        assert_eq!(
            plan("Hello, world!", "en", "zh").unwrap(),
            vec![("en".into(), "zh".into())]
        );
    }
}
