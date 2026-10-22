//! 轮盘配置：方向项（1～12 个）保存在应用配置目录的 wheel.json。
//! 每项有固定方位 angle（从正上方顺时针的角度，取 22.5° 的整数倍，共 16 个方位）；
//! 选中范围按「离哪项最近」划分，所以新增一项只会切分它两侧相邻项之间的空间。

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

pub const MAX_ITEMS: usize = 12;
/// 方位最小间隔（度）
pub const ANGLE_STEP: f64 = 22.5;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Action {
    /// 「?」：截图 + 场景交给后端理解。mode 为主要目的（见后端 ask_modes），
    /// custom 时附加用户自己写的要求；prompt 为固定的问题，空则理解标记处内容
    Ask {
        #[serde(default)]
        prompt: String,
        #[serde(default = "default_ask_mode")]
        mode: String,
        #[serde(default)]
        custom: String,
    },
    /// 记录当前场景
    Mark,
    /// 截图收藏
    Collect,
    /// 新开对话
    Chat,
    /// 打开链接、文件或应用
    Open {
        #[serde(default)]
        target: String,
    },
    /// 运行插件的某个动作
    Plugin {
        plugin: String,
        action: String,
    },
}

fn default_ask_mode() -> String {
    "auto".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WheelItem {
    pub id: String,
    /// 方位：从正上方顺时针的角度
    pub angle: f64,
    pub label: String,
    pub icon: String,
    pub action: Action,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WheelConfig {
    pub items: Vec<WheelItem>,
}

impl Default for WheelConfig {
    fn default() -> Self {
        let item = |id: &str, angle: f64, label: &str, icon: &str, action: Action| WheelItem {
            id: id.into(),
            angle,
            label: label.into(),
            icon: icon.into(),
            action,
        };
        Self {
            items: vec![
                item("mark", 0.0, &t!("标记"), "mark", Action::Mark),
                item("collect", 90.0, &t!("收藏"), "collect", Action::Collect),
                item("chat", 180.0, &t!("对话"), "chat", Action::Chat),
                item("ask", 270.0, "?", "ask", Action::Ask { prompt: String::new(), mode: default_ask_mode(), custom: String::new() }),
            ],
        }
    }
}

impl WheelConfig {
    fn validate(&self) -> Result<(), String> {
        let n = self.items.len();
        if n == 0 || n > MAX_ITEMS {
            return Err(t!("方向项数量需在 1～{max} 之间，当前 {n}", max = MAX_ITEMS, n = n));
        }
        let mut ids = std::collections::HashSet::new();
        let mut slots = std::collections::HashSet::new();
        for it in &self.items {
            let slot = it.angle / ANGLE_STEP;
            if !(0.0..360.0).contains(&it.angle) || (slot - slot.round()).abs() > 1e-6 {
                return Err(t!("「{label}」的方位 {angle} 无效，需为 0～360 之间 22.5 的整数倍", label = it.label, angle = it.angle));
            }
            if !slots.insert(slot.round() as i64) {
                return Err(t!("「{label}」与其他项方位重复", label = it.label));
            }
            if it.label.trim().is_empty() {
                return Err(t!("名称不能为空"));
            }
            if let Action::Plugin { plugin, action } = &it.action {
                if plugin.is_empty() || action.is_empty() {
                    return Err(t!("「{label}」还没有选择插件动作", label = it.label));
                }
            }
            if !ids.insert(it.id.as_str()) {
                return Err(t!("重复的 id：{id}", id = it.id));
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct ConfigState(pub Mutex<WheelConfig>);

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("wheel.json"))
}

pub fn load(app: &AppHandle) -> WheelConfig {
    let Some(path) = config_path(app) else {
        log::warn!("配置：无法确定配置目录，使用默认配置");
        return WheelConfig::default();
    };
    match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<WheelConfig>(&text) {
            Ok(mut cfg) if cfg.validate().is_ok() => {
                // 迁移：问号的名称统一为「?」
                for it in cfg.items.iter_mut() {
                    if matches!(it.action, Action::Ask { .. }) && it.label == "问号" {
                        it.label = "?".into();
                    }
                }
                log::info!("配置：已读取 {}（{} 项）", path.display(), cfg.items.len());
                cfg
            }
            Ok(cfg) => {
                log::warn!("配置：{} 校验失败（{:?}），使用默认配置", path.display(), cfg.validate().err());
                WheelConfig::default()
            }
            Err(e) => {
                log::warn!("配置：解析 {} 失败：{e}，使用默认配置", path.display());
                WheelConfig::default()
            }
        },
        Err(_) => WheelConfig::default(),
    }
}

fn save(app: &AppHandle, cfg: &WheelConfig) -> Result<(), String> {
    let path = config_path(app).ok_or("无法确定配置目录")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败：{e}"))?;
    }
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("写入 {} 失败：{e}", path.display()))?;
    log::info!("配置：已保存 {}（{} 项）", path.display(), cfg.items.len());
    Ok(())
}

/// 卸载插件时，把轮盘上引用它的方向项去掉（至少保留一项）。
pub fn remove_plugin_items(app: &AppHandle, plugin_id: &str) {
    let state = app.state::<ConfigState>();
    let mut cfg = state.0.lock().unwrap().clone();
    let before = cfg.items.len();
    cfg.items.retain(|it| !matches!(&it.action, Action::Plugin { plugin, .. } if plugin == plugin_id));
    if cfg.items.len() == before {
        return;
    }
    if cfg.items.is_empty() {
        cfg = WheelConfig::default();
    }
    if save(app, &cfg).is_ok() {
        *state.0.lock().unwrap() = cfg.clone();
        let _ = app.emit("config:changed", cfg);
    }
}

#[tauri::command]
pub fn get_config(state: State<'_, ConfigState>) -> WheelConfig {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_config(app: AppHandle, state: State<'_, ConfigState>, config: WheelConfig) -> Result<(), String> {
    config.validate()?;
    save(&app, &config)?;
    *state.0.lock().unwrap() = config.clone();
    let _ = app.emit("config:changed", config);
    Ok(())
}

#[tauri::command]
pub fn reset_config(app: AppHandle, state: State<'_, ConfigState>) -> Result<WheelConfig, String> {
    let config = WheelConfig::default();
    save(&app, &config)?;
    *state.0.lock().unwrap() = config.clone();
    let _ = app.emit("config:changed", config.clone());
    Ok(config)
}
