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
            eprintln!("aloud-code: `enable` コマンドは廃止されました。Claude Code 内で `/aloud-code:on` を使用してください");
            std::process::exit(1);
        }
        Some("disable") => {
            config::deactivate_all()?;
            eprintln!("aloud-code: disabled (all sessions)");
        }
        _ => {
            eprintln!("Usage: aloud-code <hook <event>|disable>");
            eprintln!("  hook toggle         - UserPromptSubmit hook (on/offトグル, 同期)");
            eprintln!("  hook user-prompt    - UserPromptSubmit hook (Webhook送信, 非同期)");
            eprintln!("  hook stop           - Stop hook");
            eprintln!("  disable             - 全セッションのSlack通知を無効化");
            eprintln!("");
            eprintln!("  ON/OFFの切り替えは Claude Code 内で /aloud-code:on / /aloud-code:off を使用してください");
            std::process::exit(1);
        }
    }
    Ok(())
}
