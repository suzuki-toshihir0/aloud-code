mod config;
mod formatter;
mod hook;
mod webhook;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("hook") => {
            let event = args.get(2).map(|s| s.as_str()).unwrap_or("");
            match event {
                "toggle" => hook::handle_toggle().await?,
                other => hook::handle_hook(other).await?,
            }
        }
        Some("enable") => {
            config::activate()?;
            eprintln!("aloud-code: enabled");
        }
        Some("disable") => {
            config::deactivate()?;
            eprintln!("aloud-code: disabled");
        }
        _ => {
            eprintln!("Usage: aloud-code <hook <event>|enable|disable>");
            eprintln!("  hook toggle         - UserPromptSubmit hook (on/offトグル, 同期)");
            eprintln!("  hook session-start  - SessionStart hook");
            eprintln!("  hook user-prompt    - UserPromptSubmit hook (Webhook送信, 非同期)");
            eprintln!("  hook stop           - Stop hook");
            eprintln!("  hook session-end    - SessionEnd hook");
            eprintln!("  enable              - Slack通知を有効化 (ターミナルから直接実行用)");
            eprintln!("  disable             - Slack通知を無効化 (ターミナルから直接実行用)");
            std::process::exit(1);
        }
    }
    Ok(())
}
