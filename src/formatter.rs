use serde_json::{json, Value};

const MAX_BLOCK_TEXT_LEN: usize = 3000;

#[derive(Debug, Clone)]
pub struct SessionContext {
    pub session_id: String,
    pub cwd: String,
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

pub fn format_subagent_message(agent_type: &str, message: &str, ctx: &SessionContext) -> Value {
    let text = truncate(message, MAX_BLOCK_TEXT_LEN);
    json!({
        "username": ctx.username(),
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!(":gear: *Agent ({})*\n{}", agent_type, text)
                }
            }
        ]
    })
}

pub fn format_notification_message(message: &str, ctx: &SessionContext) -> Value {
    let text = truncate(message, MAX_BLOCK_TEXT_LEN);
    json!({
        "username": ctx.username(),
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!(":speech_balloon: *Claude → User*\n{}", text)
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
            cwd: "/home/user/my-project".to_string(),
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
    fn test_truncate_long_message() {
        let ctx = test_ctx();
        let long_text = "a".repeat(4000);
        let payload = format_user_message(&long_text, &ctx);

        let text = payload["blocks"][0]["text"]["text"].as_str().unwrap();
        assert!(text.len() <= MAX_BLOCK_TEXT_LEN + 50);
    }

    #[test]
    fn test_format_subagent_message_structure() {
        let ctx = test_ctx();
        let payload = format_subagent_message("Explore", "Found relevant files.", &ctx);

        let blocks = payload["blocks"].as_array().unwrap();
        let text = &blocks[0]["text"]["text"];
        assert!(text.as_str().unwrap().contains(":gear:"));
        assert!(text.as_str().unwrap().contains("Agent (Explore)"));
        assert!(text.as_str().unwrap().contains("Found relevant files."));
    }

    #[test]
    fn test_format_subagent_message_truncation() {
        let ctx = test_ctx();
        let long_text = "x".repeat(4000);
        let payload = format_subagent_message("Agent", &long_text, &ctx);

        let text = payload["blocks"][0]["text"]["text"].as_str().unwrap();
        assert!(text.len() <= MAX_BLOCK_TEXT_LEN + 50);
    }

    #[test]
    fn test_format_notification_message_structure() {
        let ctx = test_ctx();
        let payload = format_notification_message("Which approach do you prefer?", &ctx);

        let blocks = payload["blocks"].as_array().unwrap();
        let text = &blocks[0]["text"]["text"];
        assert!(text.as_str().unwrap().contains(":speech_balloon:"));
        assert!(text.as_str().unwrap().contains("Claude → User"));
        assert!(text
            .as_str()
            .unwrap()
            .contains("Which approach do you prefer?"));
    }

    #[test]
    fn test_slack_emoji_codes_not_unicode() {
        let ctx = test_ctx();
        let user_payload = format_user_message("test", &ctx);
        let assistant_payload = format_assistant_message("test", &ctx);

        let user_text = user_payload["blocks"][0]["text"]["text"].as_str().unwrap();
        let assistant_text = assistant_payload["blocks"][0]["text"]["text"]
            .as_str()
            .unwrap();

        assert!(user_text.contains(":bust_in_silhouette:"));
        assert!(assistant_text.contains(":robot_face:"));
        assert!(!user_text.contains('👤'));
        assert!(!assistant_text.contains('🤖'));
    }
}
