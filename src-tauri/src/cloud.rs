//! 后端能力客户端：**所有模型能力都经由这里调用后端服务**，客户端不直连任何模型服务，
//! 也不记录模型调用的内容或日志。功能的系统提示词、模型选择、计费都在后端。
//!
//! 流式接口使用 SSE，事件：thinking（模型开始思考）、delta（正文增量）、done、error。

use std::sync::Mutex;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::settings;

/// 流式事件
pub enum Event<'a> {
    Thinking,
    Delta(&'a str),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Entitlements {
    pub pro: ProEnt,
    pub model: ModelEnt,
    pub currency: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProEnt {
    pub active: bool,
    pub ends_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModelEnt {
    pub active: bool,
    pub plan: Option<String>,
    pub quota: f64,
    pub used: f64,
    pub remaining: f64,
    pub cycle_end: Option<i64>,
    pub ends_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Me {
    pub user: Value,
    pub entitlements: Entitlements,
}

/// 最近一次拉到的账号信息（用于判断能否使用插件等高级功能）。
#[derive(Default)]
pub struct CloudState(pub Mutex<Option<Me>>);

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(timeout)
        .build()
        .map_err(|e| t!("创建 HTTP 客户端失败：{e}", e = e))
}

/// 未登录也能调用的接口（邮箱登录）
async fn post_public(app: &AppHandle, path: &str, body: &Value) -> Result<Value, String> {
    let a = settings::account(app);
    let resp = client(Duration::from_secs(20))?
        .post(format!("{}{path}", a.server_url.trim_end_matches('/')))
        .header("X-Lang", crate::i18n::current())
        .json(body)
        .send()
        .await
        .map_err(|e| t!("连接服务失败：{e}", e = e))?;
    if !resp.status().is_success() {
        return Err(error_of(resp).await);
    }
    resp.json().await.map_err(|e| t!("解析服务响应失败：{e}", e = e))
}

/// 邮箱登录第一步：发送验证码。
#[tauri::command]
pub async fn login_email_start(app: AppHandle, email: String) -> Result<(), String> {
    post_public(&app, "/v1/auth/email/start", &json!({ "email": email.trim() })).await.map(|_| ())
}

/// 邮箱登录第二步：校验验证码，成功后保存令牌并刷新订阅。
#[tauri::command]
pub async fn login_email_verify(app: AppHandle, email: String, code: String) -> Result<Me, String> {
    let v = post_public(&app, "/v1/auth/email/verify", &json!({ "email": email.trim(), "code": code.trim() })).await?;
    let token = v["token"].as_str().ok_or_else(|| t!("登录失败"))?;
    settings::set_token(&app, token)?;
    log::info!("账号：邮箱登录成功");
    refresh_entitlements(&app).await
}

/// 作废当前令牌（失败也不影响本地退出）。
pub async fn logout(app: &AppHandle) {
    if let Ok((url, token)) = endpoint(app, "/v1/auth/logout") {
        if let Ok(c) = client(Duration::from_secs(8)) {
            let _ = c.post(url).bearer_auth(token).send().await;
        }
    }
}

fn endpoint(app: &AppHandle, path: &str) -> Result<(String, String), String> {
    let a = settings::account(app);
    if a.token.is_empty() {
        return Err(t!("还没有登录，请在菜单 → 账号里用邮箱登录"));
    }
    Ok((format!("{}{path}", a.server_url.trim_end_matches('/')), a.token))
}

/// 把后端的错误响应转成可读文字。
async fn error_of(resp: reqwest::Response) -> String {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let msg = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_string))
        .unwrap_or_else(|| text.chars().take(200).collect());
    match status.as_u16() {
        401 => t!("登录已失效，请在菜单 → 账号里重新登录"),
        402 => msg, // 订阅 / 额度不足，后端给出原因
        _ => t!("服务返回 {status}：{msg}", status = status, msg = msg),
    }
}

async fn post_json(app: &AppHandle, path: &str, body: &Value, timeout: Duration) -> Result<Value, String> {
    let (url, token) = endpoint(app, path)?;
    let resp = client(timeout)?
        .post(url)
        .bearer_auth(token)
        .header("X-Lang", crate::i18n::current())
        .json(body)
        .send()
        .await
        .map_err(|e| t!("连接服务失败：{e}", e = e))?;
    if !resp.status().is_success() {
        return Err(error_of(resp).await);
    }
    resp.json().await.map_err(|e| t!("解析服务响应失败：{e}", e = e))
}

/// 流式调用，返回完整正文。
async fn post_stream(
    app: &AppHandle,
    path: &str,
    body: &Value,
    mut on_event: impl FnMut(Event<'_>),
) -> Result<String, String> {
    let (url, token) = endpoint(app, path)?;
    let resp = client(Duration::from_secs(300))?
        .post(url)
        .bearer_auth(token)
        .header("X-Lang", crate::i18n::current())
        .json(body)
        .send()
        .await
        .map_err(|e| t!("连接服务失败：{e}", e = e))?;
    if !resp.status().is_success() {
        return Err(error_of(resp).await);
    }

    let mut content = String::new();
    let mut buf = String::new();
    let mut event = String::new();
    let mut bytes = resp.bytes_stream();
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.map_err(|e| t!("读取服务输出中断：{e}", e = e))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buf.find('\n') {
            let line: String = buf.drain(..=pos).collect();
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(e) = line.strip_prefix("event:") {
                event = e.trim().to_string();
            } else if let Some(d) = line.strip_prefix("data:") {
                let v: Value = serde_json::from_str(d.trim()).unwrap_or(Value::Null);
                match event.as_str() {
                    "thinking" => on_event(Event::Thinking),
                    "delta" => {
                        if let Some(t) = v["text"].as_str() {
                            content.push_str(t);
                            on_event(Event::Delta(t));
                        }
                    }
                    "error" => return Err(v["error"].as_str().map(str::to_string).unwrap_or_else(|| t!("服务出错"))),
                    "done" => return Ok(content),
                    _ => {}
                }
            } else if line.is_empty() {
                event.clear();
            }
        }
    }
    Ok(content)
}

// ---------- 能力 ----------

/// 问号：截图 + 场景理解（流式）。
pub async fn ask(app: &AppHandle, body: &Value, on_event: impl FnMut(Event<'_>)) -> Result<String, String> {
    post_stream(app, "/v1/ask", body, on_event).await
}

/// 对话：messages 为 [{role, content}]，后端不保存历史（流式）。
pub async fn chat(app: &AppHandle, messages: &Value, on_event: impl FnMut(Event<'_>)) -> Result<String, String> {
    post_stream(app, "/v1/chat", &json!({ "messages": messages }), on_event).await
}

/// 收藏归类，返回 {title, category, tags, summary}。
pub async fn classify(app: &AppHandle, body: &Value) -> Result<Value, String> {
    post_json(app, "/v1/classify", body, Duration::from_secs(120)).await
}

/// 通用模型接口（插件用），返回正文。
pub async fn llm(app: &AppHandle, body: &Value) -> Result<String, String> {
    let mut b = body.clone();
    b["stream"] = json!(false);
    let v = post_json(app, "/v1/llm", &b, Duration::from_secs(300)).await?;
    Ok(v["content"].as_str().unwrap_or_default().to_string())
}

pub async fn me(app: &AppHandle) -> Result<Me, String> {
    let (url, token) = endpoint(app, "/v1/me")?;
    let resp = client(Duration::from_secs(15))?
        .get(url)
        .bearer_auth(token)
        .header("X-Lang", crate::i18n::current())
        .send()
        .await
        .map_err(|e| t!("连接服务失败：{e}", e = e))?;
    if !resp.status().is_success() {
        return Err(error_of(resp).await);
    }
    resp.json().await.map_err(|e| t!("解析账号信息失败：{e}", e = e))
}

/// 刷新缓存的账号信息。
pub async fn refresh_entitlements(app: &AppHandle) -> Result<Me, String> {
    let r = me(app).await;
    let state = app.state::<CloudState>();
    match &r {
        Ok(m) => *state.0.lock().unwrap() = Some(m.clone()),
        Err(e) => log::warn!("账号：刷新账号信息失败：{e}"),
    }
    r
}

/// 是否已解锁高级功能（自定义插件）。
pub fn pro_active(app: &AppHandle) -> bool {
    app.state::<CloudState>()
        .0
        .lock()
        .unwrap()
        .as_ref()
        .map(|m| m.entitlements.pro.active)
        .unwrap_or(false)
}

#[tauri::command]
pub async fn account_me(app: AppHandle) -> Result<Me, String> {
    refresh_entitlements(&app).await
}
