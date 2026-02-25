use serde_json::{json, Value};

const MAX_BLOCK_TEXT_LEN: usize = 3000;

#[derive(Debug, Clone)]
pub struct SessionContext {
    pub session_id: String,
    pub cwd: String,
    pub model: Option<String>,
}

impl SessionContext {
    /// session_idの先頭8文字を返す
    pub fn short_id(&self) -> &str {
        let id = self.session_id.as_str();
        &id[..id.len().min(8)]
    }

    /// プロジェクト名（cwdの最後のディレクトリ名）を返す
    pub fn project_name(&self) -> &str {
        std::path::Path::new(&self.cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.cwd)
    }

    /// Slackのusernameフィールド用文字列（"project [session_id短縮]"）
    pub fn username(&self) -> String {
        format!("{} [{}]", self.project_name(), self.short_id())
    }
}

fn truncate(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        // UTF-8の文字境界を考慮してトランケート
        let mut end = max_len - 3; // "..."の分
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &text[..end])
    }
}

pub fn format_user_message(prompt: &str, ctx: &SessionContext) -> Value {
    let text = truncate(prompt, MAX_BLOCK_TEXT_LEN);
    json!({
        "username": ctx.username(),
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!(":bust_in_silhouette: *User*\n{}", text)
                }
            }
        ]
    })
}

pub fn format_assistant_message(message: &str, ctx: &SessionContext) -> Value {
    let text = truncate(message, MAX_BLOCK_TEXT_LEN);
    json!({
        "username": ctx.username(),
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!(":robot_face: *Claude*\n{}", text)
                }
            }
        ]
    })
}

pub fn format_session_start(ctx: &SessionContext) -> Value {
    let model_text = ctx
        .model
        .as_deref()
        .map(|m| format!("\nmodel: `{}`", m))
        .unwrap_or_default();
    json!({
        "username": ctx.username(),
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!(
                        ":green_circle: *Session started*\ncwd: `{}`{}",
                        ctx.cwd, model_text
                    )
                }
            }
        ]
    })
}

pub fn format_session_end(ctx: &SessionContext, reason: Option<&str>) -> Value {
    let reason_text = reason
        .map(|r| format!("\nreason: {}", r))
        .unwrap_or_default();
    json!({
        "username": ctx.username(),
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!(":red_circle: *Session ended*{}", reason_text)
                }
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> SessionContext {
        SessionContext {
            session_id: "abcdef1234567890".to_string(),
            cwd: "/home/suzuki/my-project".to_string(),
            model: Some("claude-sonnet-4-6".to_string()),
        }
    }

    #[test]
    fn test_short_id() {
        let ctx = test_ctx();
        assert_eq!(ctx.short_id(), "abcdef12");
    }

    #[test]
    fn test_project_name() {
        let ctx = test_ctx();
        assert_eq!(ctx.project_name(), "my-project");
    }

    #[test]
    fn test_username() {
        let ctx = test_ctx();
        assert_eq!(ctx.username(), "my-project [abcdef12]");
    }

    #[test]
    fn test_format_user_message_structure() {
        let ctx = test_ctx();
        let payload = format_user_message("Hello, Claude!", &ctx);

        assert_eq!(payload["username"], "my-project [abcdef12]");
        let blocks = payload["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "section");
        let text = &blocks[0]["text"]["text"];
        assert!(text.as_str().unwrap().contains(":bust_in_silhouette:"));
        assert!(text.as_str().unwrap().contains("Hello, Claude!"));
    }

    #[test]
    fn test_format_assistant_message_structure() {
        let ctx = test_ctx();
        let payload = format_assistant_message("Sure, I can help!", &ctx);

        let blocks = payload["blocks"].as_array().unwrap();
        let text = &blocks[0]["text"]["text"];
        assert!(text.as_str().unwrap().contains(":robot_face:"));
        assert!(text.as_str().unwrap().contains("Sure, I can help!"));
    }

    #[test]
    fn test_format_session_start_contains_cwd_and_model() {
        let ctx = test_ctx();
        let payload = format_session_start(&ctx);

        let text = payload["blocks"][0]["text"]["text"].as_str().unwrap();
        assert!(text.contains(":green_circle:"));
        assert!(text.contains("/home/suzuki/my-project"));
        assert!(text.contains("claude-sonnet-4-6"));
    }

    #[test]
    fn test_format_session_start_no_model() {
        let ctx = SessionContext {
            session_id: "abc123".to_string(),
            cwd: "/tmp".to_string(),
            model: None,
        };
        let payload = format_session_start(&ctx);
        let text = payload["blocks"][0]["text"]["text"].as_str().unwrap();
        assert!(text.contains(":green_circle:"));
        assert!(!text.contains("model:"));
    }

    #[test]
    fn test_format_session_end_with_reason() {
        let ctx = test_ctx();
        let payload = format_session_end(&ctx, Some("normal"));

        let text = payload["blocks"][0]["text"]["text"].as_str().unwrap();
        assert!(text.contains(":red_circle:"));
        assert!(text.contains("normal"));
    }

    #[test]
    fn test_format_session_end_no_reason() {
        let ctx = test_ctx();
        let payload = format_session_end(&ctx, None);

        let text = payload["blocks"][0]["text"]["text"].as_str().unwrap();
        assert!(text.contains(":red_circle:"));
    }

    #[test]
    fn test_truncate_long_message() {
        let ctx = test_ctx();
        let long_text = "a".repeat(4000);
        let payload = format_user_message(&long_text, &ctx);

        let text = payload["blocks"][0]["text"]["text"].as_str().unwrap();
        // Slackの3000文字制限内に収まること（prefixとサフィックスも含む）
        assert!(text.len() <= MAX_BLOCK_TEXT_LEN + 50); // prefixの分を加算
    }

    #[test]
    fn test_slack_emoji_codes_not_unicode() {
        // Unicode絵文字でなくSlack絵文字コードを使用していること
        let ctx = test_ctx();
        let user_payload = format_user_message("test", &ctx);
        let assistant_payload = format_assistant_message("test", &ctx);
        let start_payload = format_session_start(&ctx);
        let end_payload = format_session_end(&ctx, None);

        let user_text = user_payload["blocks"][0]["text"]["text"].as_str().unwrap();
        let assistant_text = assistant_payload["blocks"][0]["text"]["text"].as_str().unwrap();
        let start_text = start_payload["blocks"][0]["text"]["text"].as_str().unwrap();
        let end_text = end_payload["blocks"][0]["text"]["text"].as_str().unwrap();

        // Slack絵文字コード（:xxx:形式）を使っていること
        assert!(user_text.contains(":bust_in_silhouette:"));
        assert!(assistant_text.contains(":robot_face:"));
        assert!(start_text.contains(":green_circle:"));
        assert!(end_text.contains(":red_circle:"));

        // Unicode絵文字（👤🤖）を使っていないこと
        assert!(!user_text.contains('👤'));
        assert!(!assistant_text.contains('🤖'));
    }
}
