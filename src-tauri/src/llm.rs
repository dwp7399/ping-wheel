//! 统一的模型调用入口。**所有模型调用都必须经过这里**，不要在别处直接请求模型服务。
//! 这样以后切换到自建网关、统一计量 / 限流 / 审计 / 重试，只需要改这一个文件。
//!
//! 当前协议：OpenAI 兼容的 /chat/completions（流式 SSE）。每次调用都会：
//! - 按 purpose 记日志（耗时、token 用量、成败）
//! - 写一条 llm_calls 记录到本地数据库，供以后统计和计费对账

use std::time::Instant;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::settings::{self, LlmSettings};

/// 调用用途，用于日志和计量。
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// 问号：解释截图
    Ask,
    /// 对话
    Chat,
    /// 收藏：自动归类
    Classify,
    /// 设置页：连通性测试
    Test,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::Ask => "ask",
            Purpose::Chat => "chat",
            Purpose::Classify => "classify",
            Purpose::Test => "test",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Clone, Debug)]
pub enum Part {
    Text(String),
    /// data:image/...;base64,... 或 http(s) 地址
    Image(String),
}

#[derive(Clone, Debug)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Part>,
}

impl Message {
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self { role, parts: vec![Part::Text(text.into())] }
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub purpose: Purpose,
    pub messages: Vec<Message>,
    pub max_tokens: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub reasoning_tokens: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Reply {
    pub content: String,
    pub reasoning: String,
    pub model: String,
    pub usage: Usage,
}

/// 流式增量
pub enum Delta<'a> {
    Content(&'a str),
    Reasoning(&'a str),
}

fn message_json(m: &Message) -> Value {
    let role = match m.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    // 纯文本用字符串，兼容性最好；带图时用多段格式
    if let [Part::Text(t)] = m.parts.as_slice() {
        return json!({ "role": role, "content": t });
    }
    let parts: Vec<Value> = m
        .parts
        .iter()
        .map(|p| match p {
            Part::Text(t) => json!({ "type": "text", "text": t }),
            Part::Image(url) => json!({ "type": "image_url", "image_url": { "url": url } }),
        })
        .collect();
    json!({ "role": role, "content": parts })
}

/// 非流式调用（内部也走流式，只是不回调）。
pub async fn complete(app: &AppHandle, req: Request) -> Result<Reply, String> {
    stream(app, req, |_| {}).await
}

/// 流式调用：每收到一段内容就回调 on_delta，结束后返回完整结果。
pub async fn stream(
    app: &AppHandle,
    req: Request,
    mut on_delta: impl FnMut(Delta<'_>),
) -> Result<Reply, String> {
    let cfg = settings::llm(app);
    let started = Instant::now();
    let result = run(&cfg, &req, &mut on_delta).await;
    let ms = started.elapsed().as_millis() as u64;

    match &result {
        Ok(r) => log::info!(
            "模型：{} 调用成功，模型 {}，耗时 {ms}ms，输入 {} / 输出 {}（思考 {}）tokens",
            req.purpose.as_str(),
            r.model,
            r.usage.prompt_tokens,
            r.usage.completion_tokens,
            r.usage.reasoning_tokens
        ),
        Err(e) => log::warn!(
            "模型：{} 调用失败，模型 {}，地址 {}，耗时 {ms}ms：{e}",
            req.purpose.as_str(),
            cfg.model,
            cfg.base_url
        ),
    }
    crate::store::record_llm_call(app, req.purpose.as_str(), &cfg.model, ms, &result);
    result
}

async fn run(
    cfg: &LlmSettings,
    req: &Request,
    on_delta: &mut impl FnMut(Delta<'_>),
) -> Result<Reply, String> {
    let key = cfg
        .effective_key()
        .ok_or("还没有配置模型 API Key，请在菜单 → 模型设置里填写")?;
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let mut body = json!({
        "model": cfg.model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": req.messages.iter().map(message_json).collect::<Vec<_>>(),
    });
    if let Some(n) = req.max_tokens {
        body["max_tokens"] = json!(n);
    }

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败：{e}"))?;
    let resp = client
        .post(&url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求模型服务失败：{e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
            .unwrap_or(text);
        return Err(format!("模型服务返回 {status}：{}", msg.chars().take(300).collect::<String>()));
    }

    let mut reply = Reply { model: cfg.model.clone(), ..Default::default() };
    let mut buf = String::new();
    let mut bytes = resp.bytes_stream();
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.map_err(|e| format!("读取模型输出中断：{e}"))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buf.find('\n') {
            let line: String = buf.drain(..=pos).collect();
            let line = line.trim();
            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                return Ok(reply);
            }
            let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
            if let Some(m) = v["model"].as_str() {
                reply.model = m.to_string();
            }
            if v["usage"].is_object() {
                let u = &v["usage"];
                reply.usage = Usage {
                    prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0),
                    completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0),
                    reasoning_tokens: u["completion_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0),
                };
            }
            let delta = &v["choices"][0]["delta"];
            if let Some(r) = delta["reasoning_content"].as_str().filter(|s| !s.is_empty()) {
                reply.reasoning.push_str(r);
                on_delta(Delta::Reasoning(r));
            }
            if let Some(c) = delta["content"].as_str().filter(|s| !s.is_empty()) {
                reply.content.push_str(c);
                on_delta(Delta::Content(c));
            }
        }
    }
    Ok(reply)
}

/// 设置页的连通性测试。
#[tauri::command]
pub async fn llm_test(app: AppHandle) -> Result<String, String> {
    let reply = complete(
        &app,
        Request {
            purpose: Purpose::Test,
            messages: vec![Message::text(Role::User, "回复「连接正常」四个字即可。")],
            max_tokens: Some(400),
        },
    )
    .await?;
    let text = reply.content.trim();
    Ok(format!(
        "{}（模型 {}）",
        if text.is_empty() { "已连通，但没有返回正文" } else { text },
        reply.model
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实调用测试（会产生少量费用），手动运行：
    /// PING_WHEEL_API_KEY=... cargo test llm_live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn llm_live() {
        let cfg = LlmSettings::default();
        let req = Request {
            purpose: Purpose::Test,
            messages: vec![Message::text(Role::User, "回复「连接正常」四个字即可。")],
            max_tokens: Some(400),
        };
        let mut deltas = 0;
        let reply = tauri::async_runtime::block_on(run(&cfg, &req, &mut |_| deltas += 1)).expect("调用失败");
        println!("模型 {}，回复：{}，增量 {deltas} 段，用量 {:?}", reply.model, reply.content, reply.usage);
        assert!(!reply.content.is_empty() || !reply.reasoning.is_empty());
        assert!(deltas > 0);
    }
}
