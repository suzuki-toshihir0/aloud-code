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
    // SubagentStop用
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    // Notification用
    pub message: Option<String>,
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

/// トランスクリプトから未送信のassistantテキストをフラッシュして送信する
/// カーソルロックで同一セッションの並行実行を直列化する
async fn flush_transcript(
    session_id: &str,
    transcript_path: &str,
    ctx: &SessionContext,
    sender: &WebhookSender,
) -> Result<()> {
    let (lock, maybe_cursor) = config::CursorLockGuard::acquire(session_id)?;

    let cursor = match maybe_cursor {
        None => {
            // カーソルファイルが存在しない = 有効化直後の初回フラッシュ
            // 過去メッセージは送信せず、現在のファイル末尾にカーソルを設定する
            let file_size = std::fs::metadata(transcript_path)
                .map(|m| m.len())
                .unwrap_or(0);
            lock.commit(file_size)?;
            return Ok(());
        }
        Some(c) => c,
    };

    let (messages, new_cursor) =
        crate::transcript::read_new_assistant_texts(transcript_path, cursor)?;

    for msg in messages {
        let payload = formatter::format_assistant_message(&msg, ctx);
        sender.send(payload).await?;
    }

    // 送信成功後にのみカーソルを更新（at-least-once保証）
    lock.commit(new_cursor)?;
    Ok(())
}

pub async fn handle_hook(event: &str) -> Result<()> {
    let input = HookInput::from_stdin()?;
    let session_id = match input.session_id.as_deref() {
        Some(id) if !id.is_empty() => id,
        _ => {
            // session_id欠損時は処理中断（セッション混線防止）
            eprintln!("aloud-code: session_idが空のため処理をスキップ");
            println!("{{}}");
            return Ok(());
        }
    };

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

    // トランスクリプト・フラッシュ（Stop以外の全イベント共通）
    // Stop hookはtranscript書き込み完了前に発火するレースコンディションがあるため除外し、
    // last_assistant_messageを直接使う（Stopアームで処理）
    if event != "stop" {
        if let Some(transcript_path) = &input.transcript_path {
            if let Err(e) = flush_transcript(session_id, transcript_path, &ctx, &sender).await {
                eprintln!("aloud-code: トランスクリプトフラッシュエラー: {}", e);
            }
        }
    }

    match event {
        "user-prompt" => {
            let prompt = input.prompt.as_deref().unwrap_or("");
            // トグルコマンドはhandle_toggleで処理済みのためスキップ
            if !prompt.is_empty() && !is_toggle_command(prompt) {
                let payload = formatter::format_user_message(prompt, &ctx);
                sender.send(payload).await?;
            }
        }
        "pre-tool-use" => {
            // フラッシュのみ（上で実行済み）
        }
        "stop" => {
            // transcriptへの書き込み完了を待たずにlast_assistant_messageを直接送信
            let message = input.last_assistant_message.as_deref().unwrap_or("");
            if !message.is_empty() {
                let payload = formatter::format_assistant_message(message, &ctx);
                sender.send(payload).await?;
            }
            // カーソルをファイル末尾に進め、次回UserPromptSubmitでの重複送信を防ぐ
            if let Some(transcript_path) = &input.transcript_path {
                let file_size = std::fs::metadata(transcript_path.as_str())
                    .map(|m| m.len())
                    .unwrap_or(0);
                if let Ok((lock, _)) = config::CursorLockGuard::acquire(session_id) {
                    let _ = lock.commit(file_size);
                }
            }
        }
        "subagent-stop" => {
            let agent_type = input.agent_type.as_deref().unwrap_or("Agent");
            let message = input.last_assistant_message.as_deref().unwrap_or("");
            if !message.is_empty() {
                let payload = formatter::format_subagent_message(agent_type, message, &ctx);
                sender.send(payload).await?;
            }
        }
        "notification" => {
            let message = input.message.as_deref().unwrap_or("");
            if !message.is_empty() {
                let payload = formatter::format_notification_message(message, &ctx);
                sender.send(payload).await?;
            }
        }
        "session-end" => {
            // セッション非アクティブ化（カーソル+ロックファイルも削除）
            config::deactivate(session_id)?;
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
    fn test_deserialize_subagent_stop_input() {
        let json = r#"{
            "session_id": "abc123",
            "cwd": "/home/user/project",
            "hook_event_name": "SubagentStop",
            "agent_id": "agent-xyz",
            "agent_type": "Explore",
            "last_assistant_message": "Found 5 matching files."
        }"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.agent_id.as_deref(), Some("agent-xyz"));
        assert_eq!(input.agent_type.as_deref(), Some("Explore"));
        assert_eq!(
            input.last_assistant_message.as_deref(),
            Some("Found 5 matching files.")
        );
    }

    #[test]
    fn test_deserialize_notification_input() {
        let json = r#"{
            "session_id": "abc123",
            "cwd": "/home/user/project",
            "hook_event_name": "Notification",
            "message": "Which option do you prefer?"
        }"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(
            input.message.as_deref(),
            Some("Which option do you prefer?")
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

    #[test]
    fn test_empty_session_id_skipped() {
        // session_idが空のHookInputはガード条件に引っかかる
        let input = HookInput {
            session_id: Some("".to_string()),
            ..Default::default()
        };
        let id = input.session_id.as_deref();
        let is_empty_or_missing = matches!(id, Some("") | None);
        assert!(is_empty_or_missing);
    }
}
