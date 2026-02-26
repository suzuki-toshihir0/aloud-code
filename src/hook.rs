use anyhow::Result;
use serde::Deserialize;
use std::io::Read;

use crate::config::{self, Config};
use crate::formatter::{self, SessionContext};
use crate::webhook::WebhookSender;

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct HookInput {
    pub session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub cwd: Option<String>,
    pub hook_event_name: Option<String>,
    pub prompt: Option<String>,
    pub last_assistant_message: Option<String>,
    pub reason: Option<String>,
    pub model: Option<String>,
}

impl HookInput {
    pub fn from_stdin() -> Result<Self> {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        let input: HookInput = if buf.trim().is_empty() {
            HookInput::default()
        } else {
            serde_json::from_str(&buf)?
        };
        Ok(input)
    }

    pub fn to_session_context(&self) -> SessionContext {
        SessionContext {
            session_id: self.session_id.clone().unwrap_or_default(),
            cwd: self.cwd.clone().unwrap_or_default(),
        }
    }
}

/// `/aloud-code:on` / `/aloud-code:off` コマンドかどうかを判定
fn is_toggle_command(prompt: &str) -> bool {
    matches!(prompt.trim(), "/aloud-code:on" | "/aloud-code:off")
}

/// UserPromptSubmit hook (同期): トグルコマンドを処理
pub async fn handle_toggle() -> Result<()> {
    let input = HookInput::from_stdin()?;
    let prompt = input.prompt.as_deref().unwrap_or("");
    let session_id = input.session_id.as_deref().unwrap_or("");

    match prompt.trim() {
        "/aloud-code:on" => {
            config::activate(session_id)?;
        }
        "/aloud-code:off" => {
            config::deactivate(session_id)?;
        }
        _ => {}
    }
    println!("{{}}");
    Ok(())
}

pub async fn handle_hook(event: &str) -> Result<()> {
    let input = HookInput::from_stdin()?;
    let session_id = input.session_id.as_deref().unwrap_or("");

    if !config::is_active(session_id) {
        println!("{{}}");
        return Ok(());
    }

    let config = Config::load()?;
    let webhook_url = match &config.webhook.url {
        Some(url) if !url.is_empty() => url.clone(),
        _ => {
            println!("{{}}");
            return Ok(());
        }
    };

    let ctx = input.to_session_context();
    let sender = WebhookSender::new(webhook_url);

    match event {
        "user-prompt" => {
            let prompt = input.prompt.as_deref().unwrap_or("");
            // トグルコマンドはhandle_toggleで処理済みのためスキップ
            if !prompt.is_empty() && !is_toggle_command(prompt) {
                let payload = formatter::format_user_message(prompt, &ctx);
                sender.send(payload).await?;
            }
        }
        "stop" => {
            let message = input.last_assistant_message.as_deref().unwrap_or("");
            if !message.is_empty() {
                let payload = formatter::format_assistant_message(message, &ctx);
                sender.send(payload).await?;
            }
        }
        unknown => {
            eprintln!("aloud-code: 未知のhookイベント: {}", unknown);
        }
    }

    println!("{{}}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_user_prompt_input() {
        let json = r#"{
            "session_id": "abc123",
            "transcript_path": "/tmp/test.jsonl",
            "cwd": "/home/user/project",
            "hook_event_name": "UserPromptSubmit",
            "prompt": "Hello, Claude!",
            "model": "claude-sonnet-4-6"
        }"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.session_id.as_deref(), Some("abc123"));
        assert_eq!(input.prompt.as_deref(), Some("Hello, Claude!"));
        assert_eq!(input.model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn test_deserialize_stop_input() {
        let json = r#"{
            "session_id": "abc123",
            "cwd": "/home/user/project",
            "hook_event_name": "Stop",
            "last_assistant_message": "I can help with that!"
        }"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(
            input.last_assistant_message.as_deref(),
            Some("I can help with that!")
        );
    }

    #[test]
    fn test_deserialize_session_end_input() {
        let json = r#"{
            "session_id": "abc123",
            "cwd": "/tmp",
            "hook_event_name": "SessionEnd",
            "reason": "normal"
        }"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.reason.as_deref(), Some("normal"));
    }

    #[test]
    fn test_to_session_context() {
        let input = HookInput {
            session_id: Some("xyz789".to_string()),
            cwd: Some("/home/user/proj".to_string()),
            ..Default::default()
        };
        let ctx = input.to_session_context();
        assert_eq!(ctx.session_id, "xyz789");
        assert_eq!(ctx.cwd, "/home/user/proj");
    }

    #[test]
    fn test_empty_stdin_uses_default() {
        let input: HookInput = if "".trim().is_empty() {
            HookInput::default()
        } else {
            serde_json::from_str("").unwrap()
        };
        assert!(input.session_id.is_none());
        assert!(input.prompt.is_none());
    }

    #[test]
    fn test_is_toggle_command() {
        assert!(is_toggle_command("/aloud-code:on"));
        assert!(is_toggle_command("/aloud-code:off"));
        assert!(is_toggle_command("  /aloud-code:on  ")); // 前後スペース
        assert!(!is_toggle_command("hello"));
        assert!(!is_toggle_command("/aloud-code:on extra")); // 余分なテキスト
        assert!(!is_toggle_command(""));
    }
}
