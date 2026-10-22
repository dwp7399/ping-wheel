//! 国际化。
//! - 语言：默认按系统时区自动选择（支持 12 种），对不上就用英文；用户可在设置里手动指定
//! - 模型：每次请求后端都带上 X-Lang，后端在模型交互最开头加语言指令
//! - 界面文案：以简体中文原文为键，查 i18n/<语言>.json 对照表；没有对应表的语言用英文表，查不到就显示原文
//!
//! 日志一律保持中文，不走翻译。

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// 支持的语言：(代码, 本地名称)
pub const LANGS: &[(&str, &str)] = &[
    ("zh-CN", "简体中文"),
    ("zh-TW", "繁體中文"),
    ("en", "English"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("es", "Español"),
    ("pt", "Português"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("ru", "Русский"),
    ("vi", "Tiếng Việt"),
    ("id", "Bahasa Indonesia"),
];

/// 时区（IANA 名）→ 语言。按前缀匹配，先匹配到的优先。
const TZ_MAP: &[(&str, &str)] = &[
    ("Asia/Shanghai", "zh-CN"),
    ("Asia/Chongqing", "zh-CN"),
    ("Asia/Chungking", "zh-CN"),
    ("Asia/Harbin", "zh-CN"),
    ("Asia/Urumqi", "zh-CN"),
    ("Asia/Kashgar", "zh-CN"),
    ("PRC", "zh-CN"),
    ("Asia/Taipei", "zh-TW"),
    ("Asia/Hong_Kong", "zh-TW"),
    ("Asia/Macau", "zh-TW"),
    ("Hongkong", "zh-TW"),
    ("ROC", "zh-TW"),
    ("Asia/Tokyo", "ja"),
    ("Japan", "ja"),
    ("Asia/Seoul", "ko"),
    ("ROK", "ko"),
    ("Asia/Ho_Chi_Minh", "vi"),
    ("Asia/Saigon", "vi"),
    ("Asia/Jakarta", "id"),
    ("Asia/Pontianak", "id"),
    ("Asia/Makassar", "id"),
    ("Asia/Jayapura", "id"),
    // 葡语：巴西、葡萄牙、安哥拉、莫桑比克
    ("America/Sao_Paulo", "pt"),
    ("America/Fortaleza", "pt"),
    ("America/Recife", "pt"),
    ("America/Bahia", "pt"),
    ("America/Belem", "pt"),
    ("America/Manaus", "pt"),
    ("America/Cuiaba", "pt"),
    ("America/Campo_Grande", "pt"),
    ("America/Porto_Velho", "pt"),
    ("America/Boa_Vista", "pt"),
    ("America/Rio_Branco", "pt"),
    ("America/Maceio", "pt"),
    ("America/Araguaina", "pt"),
    ("America/Santarem", "pt"),
    ("America/Noronha", "pt"),
    ("Brazil/", "pt"),
    ("Europe/Lisbon", "pt"),
    ("Atlantic/Madeira", "pt"),
    ("Atlantic/Azores", "pt"),
    ("Africa/Luanda", "pt"),
    ("Africa/Maputo", "pt"),
    // 西语：西班牙与拉美
    ("Europe/Madrid", "es"),
    ("Atlantic/Canary", "es"),
    ("Africa/Ceuta", "es"),
    ("America/Mexico_City", "es"),
    ("America/Cancun", "es"),
    ("America/Merida", "es"),
    ("America/Monterrey", "es"),
    ("America/Matamoros", "es"),
    ("America/Chihuahua", "es"),
    ("America/Hermosillo", "es"),
    ("America/Mazatlan", "es"),
    ("America/Tijuana", "es"),
    ("America/Bahia_Banderas", "es"),
    ("America/Argentina/", "es"),
    ("America/Buenos_Aires", "es"),
    ("America/Bogota", "es"),
    ("America/Lima", "es"),
    ("America/Santiago", "es"),
    ("America/Caracas", "es"),
    ("America/Montevideo", "es"),
    ("America/Asuncion", "es"),
    ("America/La_Paz", "es"),
    ("America/Guayaquil", "es"),
    ("America/Guatemala", "es"),
    ("America/Costa_Rica", "es"),
    ("America/Panama", "es"),
    ("America/El_Salvador", "es"),
    ("America/Tegucigalpa", "es"),
    ("America/Managua", "es"),
    ("America/Havana", "es"),
    ("America/Santo_Domingo", "es"),
    ("America/Puerto_Rico", "es"),
    ("Mexico/", "es"),
    // 法语
    ("Europe/Paris", "fr"),
    ("Europe/Brussels", "fr"),
    ("Europe/Monaco", "fr"),
    ("Europe/Luxembourg", "fr"),
    ("Indian/Reunion", "fr"),
    ("Africa/Abidjan", "fr"),
    ("Africa/Dakar", "fr"),
    ("Africa/Algiers", "fr"),
    ("Africa/Tunis", "fr"),
    ("Africa/Casablanca", "fr"),
    ("Africa/Kinshasa", "fr"),
    // 德语
    ("Europe/Berlin", "de"),
    ("Europe/Busingen", "de"),
    ("Europe/Vienna", "de"),
    ("Europe/Zurich", "de"),
    ("Europe/Vaduz", "de"),
    // 俄语：俄罗斯、白俄罗斯
    ("Europe/Moscow", "ru"),
    ("Europe/Kaliningrad", "ru"),
    ("Europe/Samara", "ru"),
    ("Europe/Volgograd", "ru"),
    ("Europe/Kirov", "ru"),
    ("Europe/Saratov", "ru"),
    ("Europe/Ulyanovsk", "ru"),
    ("Europe/Astrakhan", "ru"),
    ("Europe/Minsk", "ru"),
    ("Asia/Yekaterinburg", "ru"),
    ("Asia/Omsk", "ru"),
    ("Asia/Novosibirsk", "ru"),
    ("Asia/Barnaul", "ru"),
    ("Asia/Tomsk", "ru"),
    ("Asia/Novokuznetsk", "ru"),
    ("Asia/Krasnoyarsk", "ru"),
    ("Asia/Irkutsk", "ru"),
    ("Asia/Chita", "ru"),
    ("Asia/Yakutsk", "ru"),
    ("Asia/Khandyga", "ru"),
    ("Asia/Vladivostok", "ru"),
    ("Asia/Ust-Nera", "ru"),
    ("Asia/Magadan", "ru"),
    ("Asia/Sakhalin", "ru"),
    ("Asia/Srednekolymsk", "ru"),
    ("Asia/Kamchatka", "ru"),
    ("Asia/Anadyr", "ru"),
    ("W-SU", "ru"),
];

pub fn detect_timezone() -> Option<String> {
    iana_time_zone::get_timezone().ok()
}

/// 时区对应的语言，对不上就是英文。
pub fn lang_for_timezone(tz: &str) -> &'static str {
    TZ_MAP.iter().find(|(prefix, _)| tz.starts_with(prefix)).map(|(_, l)| *l).unwrap_or("en")
}

pub fn is_supported(code: &str) -> bool {
    LANGS.iter().any(|(c, _)| *c == code)
}

static CURRENT: RwLock<String> = RwLock::new(String::new());

/// 按设置算出实际使用的语言并生效。setting 为 "auto" 时按时区。
pub fn apply(setting: &str) -> String {
    let lang = if setting != "auto" && is_supported(setting) {
        setting.to_string()
    } else {
        detect_timezone().map(|tz| lang_for_timezone(&tz).to_string()).unwrap_or_else(|| "en".into())
    };
    *CURRENT.write().unwrap() = lang.clone();
    lang
}

/// 当前语言（发给后端的 X-Lang）
pub fn current() -> String {
    let c = CURRENT.read().unwrap();
    if c.is_empty() { "en".into() } else { c.clone() }
}

/// 界面文案用的对照表：简体 / 繁体中文直接用原文，其他语言暂时都用英文表。
fn table() -> Option<&'static HashMap<String, String>> {
    static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
    let lang = current();
    if lang.starts_with("zh") {
        return None;
    }
    Some(EN.get_or_init(|| serde_json::from_str(include_str!("../../i18n/en.json")).unwrap_or_default()))
}

/// 翻译并替换 {name} 参数。
pub fn tr(zh: &str, args: &[(&str, String)]) -> String {
    let mut s = table().and_then(|t| t.get(zh)).cloned().unwrap_or_else(|| zh.to_string());
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// 界面文案：t!("已打开 {target}", target = x)
#[macro_export]
macro_rules! t {
    ($zh:literal) => {
        $crate::i18n::tr($zh, &[])
    };
    ($zh:literal, $($k:ident = $v:expr),+ $(,)?) => {
        $crate::i18n::tr($zh, &[$((stringify!($k), ($v).to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timezone_mapping() {
        assert_eq!(lang_for_timezone("Asia/Shanghai"), "zh-CN");
        assert_eq!(lang_for_timezone("Asia/Hong_Kong"), "zh-TW");
        assert_eq!(lang_for_timezone("America/Argentina/Buenos_Aires"), "es");
        assert_eq!(lang_for_timezone("America/Sao_Paulo"), "pt");
        assert_eq!(lang_for_timezone("Europe/Berlin"), "de");
        assert_eq!(lang_for_timezone("America/New_York"), "en");
        assert_eq!(lang_for_timezone("Africa/Nairobi"), "en");
        for (_, l) in TZ_MAP {
            assert!(is_supported(l), "{l} 不在支持列表里");
        }
    }

    #[test]
    fn translation_table_is_valid_json() {
        let t: HashMap<String, String> = serde_json::from_str(include_str!("../../i18n/en.json")).unwrap();
        assert!(!t.is_empty());
        // 参数占位符在译文里要保留
        for (zh, en) in &t {
            for part in zh.split('{').skip(1) {
                let name = part.split('}').next().unwrap();
                assert!(en.contains(&format!("{{{name}}}")), "「{zh}」的译文缺少 {{{name}}}");
            }
        }
    }
}
