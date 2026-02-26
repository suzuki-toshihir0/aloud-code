mod config;
mod formatter;
mod hook;
mod webhook;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("hook") {
        let event = args.get(2).map(|s| s.as_str()).unwrap_or("");
        match event {
            "toggle" => hook::handle_toggle().await?,
            other => hook::handle_hook(other).await?,
        }
    }
    Ok(())
}
