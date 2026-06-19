//! 供应商「真实模型测试」服务（real check）
//!
//! 与 `stream_check`（reachability，仅探测 base_url 可达）不同：本服务**发送一次
//! 真实的最小模型请求**，因此能区分：
//! - 200 可用
//! - 401 key 错
//! - 403 无权限 / 余额不足 / 分组不可用
//! - 404 模型不存在
//! - 429 频率限制
//! - 5xx 上游故障
//!
//! ## 与故障转移的关系
//!
//! 本服务**不触碰熔断器**（与 reachability 一致）。它只是给用户一个明确的
//! 「这个 key + 模型 + endpoint 现在能不能真用」的回答，是否据此切换由调用方决定。
//!
//! ## 代价
//!
//! 会真实打一次模型端点，可能产生极小额计费（请求 max_tokens=1）。因此它是
//! 用户**显式触发**的操作，不在批量/自动路径里调用。

use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::app_config::AppType;
use crate::error::AppError;
use crate::provider::Provider;
use crate::proxy::providers::{get_adapter, ClaudeAdapter, ProviderAdapter};

/// 真实测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealCheckResult {
    /// 是否可用（HTTP 2xx）
    pub success: bool,
    /// 人类可读消息
    pub message: String,
    /// 真实 HTTP 状态码（网络层失败时为 None）
    pub http_status: Option<u16>,
    /// 实际使用的模型名
    pub model_used: String,
    /// 往返延迟（毫秒）
    pub response_time_ms: Option<u64>,
    /// 错误分类：auth / forbidden / not_found / rate_limit / server / network / unknown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_category: Option<String>,
    pub tested_at: i64,
}

/// 真实测试服务
pub struct RealCheckService;

impl RealCheckService {
    /// 对单个供应商执行真实模型测试。
    ///
    /// `model_override`：调用方可指定测试用模型；为空时从供应商配置推断。
    /// `timeout_secs`：超时（真实生成可能比可达性探测慢，默认调用方给 30s）。
    pub async fn check(
        app_type: &AppType,
        provider: &Provider,
        model_override: Option<String>,
        timeout_secs: u64,
    ) -> Result<RealCheckResult, AppError> {
        let tested_at = chrono::Utc::now().timestamp();

        // 1. 解析 base_url + 鉴权（复用转发路径同一套 adapter）
        let adapter = Self::adapter_for(app_type);
        let base_url = adapter
            .extract_base_url(provider)
            .map_err(|e| AppError::Message(format!("提取 base_url 失败: {e}")))?;

        let auth = adapter.extract_auth(provider).ok_or_else(|| {
            AppError::Message("无法提取鉴权信息（缺少 api key？）".to_string())
        })?;
        let auth_headers = adapter
            .get_auth_headers(&auth)
            .map_err(|e| AppError::Message(format!("构造鉴权头失败: {e}")))?;

        // 2. 选模型 + 构造最小请求体（按协议族）
        let model = model_override
            .filter(|m| !m.trim().is_empty())
            .or_else(|| Self::infer_model(provider))
            .unwrap_or_else(|| "gpt-4o-mini".to_string());

        let (endpoint, body) = Self::build_min_request(app_type, &model);
        let url = adapter.build_url(&base_url, &endpoint);

        // 3. 发请求
        let client = crate::proxy::http_client::get();
        let timeout = std::time::Duration::from_secs(timeout_secs.max(1));
        let mut req = client
            .post(&url)
            .timeout(timeout)
            .header("content-type", "application/json")
            .json(&body);
        for (name, value) in auth_headers {
            req = req.header(name, value);
        }
        if let Some(ua) = Self::custom_user_agent(provider) {
            req = req.header("user-agent", ua);
        }

        let start = Instant::now();
        let resp = req.send().await;
        let elapsed = start.elapsed().as_millis() as u64;

        match resp {
            Ok(r) => {
                let status = r.status().as_u16();
                let body_text = r.text().await.unwrap_or_default();
                Ok(Self::classify(status, &body_text, &model, elapsed, tested_at))
            }
            Err(e) => {
                let msg = if e.is_timeout() {
                    "请求超时".to_string()
                } else if e.is_connect() {
                    format!("连接失败: {e}")
                } else {
                    e.to_string()
                };
                Ok(RealCheckResult {
                    success: false,
                    message: msg,
                    http_status: None,
                    model_used: model,
                    response_time_ms: Some(elapsed),
                    error_category: Some("network".to_string()),
                    tested_at,
                })
            }
        }
    }

    /// 按 HTTP 状态码分类结果。
    fn classify(
        status: u16,
        body: &str,
        model: &str,
        elapsed: u64,
        tested_at: i64,
    ) -> RealCheckResult {
        let snippet = Self::body_snippet(body);
        let (success, category, msg) = match status {
            200..=299 => (true, None, "可用".to_string()),
            401 => (false, Some("auth"), format!("鉴权失败 (401)：{snippet}")),
            403 => (
                false,
                Some("forbidden"),
                format!("无权限/余额不足/分组不可用 (403)：{snippet}"),
            ),
            404 => (
                false,
                Some("not_found"),
                format!("模型或端点不存在 (404)：{snippet}"),
            ),
            429 => (
                false,
                Some("rate_limit"),
                format!("频率限制 (429)：{snippet}"),
            ),
            500..=599 => (
                false,
                Some("server"),
                format!("上游故障 ({status})：{snippet}"),
            ),
            _ => (
                false,
                Some("unknown"),
                format!("非预期状态 ({status})：{snippet}"),
            ),
        };

        RealCheckResult {
            success,
            message: msg,
            http_status: Some(status),
            model_used: model.to_string(),
            response_time_ms: Some(elapsed),
            error_category: category.map(|s| s.to_string()),
            tested_at,
        }
    }

    /// 截取响应体片段（去多余空白、限长），便于在 UI 显示错误原因。
    /// 按字符截断，避免在多字节 UTF-8 边界 panic。
    fn body_snippet(body: &str) -> String {
        let cleaned: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
        if cleaned.chars().count() > 200 {
            let truncated: String = cleaned.chars().take(200).collect();
            format!("{truncated}…")
        } else {
            cleaned
        }
    }

    /// 选择适配器（与转发路径一致；ClaudeDesktop 复用 Claude adapter）。
    fn adapter_for(app_type: &AppType) -> Box<dyn ProviderAdapter> {
        match app_type {
            AppType::ClaudeDesktop => Box::new(ClaudeAdapter::new()),
            _ => get_adapter(app_type),
        }
    }

    /// 按协议族构造最小请求（endpoint, body）。max_tokens 取 1，尽量省额度。
    fn build_min_request(app_type: &AppType, model: &str) -> (String, serde_json::Value) {
        match app_type {
            // Claude：Anthropic Messages 协议
            AppType::Claude | AppType::ClaudeDesktop => (
                "/v1/messages".to_string(),
                serde_json::json!({
                    "model": model,
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "hi"}],
                }),
            ),
            // Gemini：OpenAI 兼容层（多数中转站支持）
            AppType::Gemini => (
                "/v1/chat/completions".to_string(),
                serde_json::json!({
                    "model": model,
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "hi"}],
                }),
            ),
            // Codex 及其余：OpenAI Chat Completions
            _ => (
                "/v1/chat/completions".to_string(),
                serde_json::json!({
                    "model": model,
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "hi"}],
                }),
            ),
        }
    }

    /// 从供应商配置推断默认测试模型（取模型映射/目录里的第一个）。
    fn infer_model(provider: &Provider) -> Option<String> {
        // 1) modelCatalog.models[0].model
        if let Some(m) = provider
            .settings_config
            .pointer("/modelCatalog/models")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.get("model").or_else(|| first.get("displayName")))
            .and_then(|v| v.as_str())
        {
            return Some(m.to_string());
        }
        // 2) config 文本里的 model = "xxx"
        if let Some(cfg) = provider.settings_config.get("config").and_then(|v| v.as_str()) {
            for line in cfg.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("model") {
                    if let Some(eq) = rest.trim_start().strip_prefix('=') {
                        let val = eq.trim().trim_matches('"').trim();
                        if !val.is_empty() {
                            return Some(val.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    fn custom_user_agent(provider: &Provider) -> Option<reqwest::header::HeaderValue> {
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.custom_user_agent_header().ok().flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_maps_status_to_category() {
        let cases = [
            (200u16, true, None),
            (401, false, Some("auth")),
            (403, false, Some("forbidden")),
            (404, false, Some("not_found")),
            (429, false, Some("rate_limit")),
            (500, false, Some("server")),
            (503, false, Some("server")),
            (418, false, Some("unknown")),
        ];
        for (status, success, cat) in cases {
            let r = RealCheckService::classify(status, "{}", "m", 10, 0);
            assert_eq!(r.success, success, "status {status}");
            assert_eq!(r.http_status, Some(status));
            assert_eq!(r.error_category.as_deref(), cat, "status {status}");
        }
    }

    #[test]
    fn body_snippet_collapses_and_truncates() {
        let long = "a ".repeat(200);
        let s = RealCheckService::body_snippet(&long);
        // 截断到 200 个字符 + 省略号
        assert_eq!(s.chars().count(), 201, "chars: {}", s.chars().count());
        assert!(s.ends_with('…'));
        assert_eq!(
            RealCheckService::body_snippet("line1\n  line2\t x"),
            "line1 line2 x"
        );
        // 多字节不 panic
        let cjk = "中".repeat(300);
        let r = RealCheckService::body_snippet(&cjk);
        assert_eq!(r.chars().count(), 201);
    }

    #[test]
    fn infer_model_from_catalog() {
        let p = Provider::with_id(
            "t".into(),
            "T".into(),
            serde_json::json!({
                "modelCatalog": { "models": [{"model": "glm-5.1"}] }
            }),
            None,
        );
        assert_eq!(RealCheckService::infer_model(&p).as_deref(), Some("glm-5.1"));
    }

    #[test]
    fn infer_model_from_config_text() {
        let p = Provider::with_id(
            "t".into(),
            "T".into(),
            serde_json::json!({
                "config": "model_provider = \"x\"\nmodel = \"deepseek-chat\"\n"
            }),
            None,
        );
        assert_eq!(
            RealCheckService::infer_model(&p).as_deref(),
            Some("deepseek-chat")
        );
    }

    #[test]
    fn build_min_request_uses_messages_for_claude() {
        let (endpoint, body) = RealCheckService::build_min_request(&AppType::Claude, "claude-x");
        assert_eq!(endpoint, "/v1/messages");
        assert_eq!(body["max_tokens"], 1);
        let (endpoint2, _) = RealCheckService::build_min_request(&AppType::Codex, "gpt-x");
        assert_eq!(endpoint2, "/v1/chat/completions");
    }
}
