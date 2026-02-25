use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub webhook: WebhookConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct WebhookConfig {
    pub url: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = config_file_path()?;
        if !config_path.exists() {
            return Ok(Config::default());
        }
        let content = std::fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

pub fn is_active() -> bool {
    active_flag_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

pub fn activate() -> Result<()> {
    let path = active_flag_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, "")?;
    Ok(())
}

pub fn deactivate() -> Result<()> {
    let path = active_flag_path()?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn config_file_path() -> Result<PathBuf> {
    // テストや特殊環境での上書きをサポート
    if let Ok(path) = std::env::var("ALOUD_CODE_CONFIG_FILE") {
        return Ok(PathBuf::from(path));
    }
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("設定ディレクトリが見つかりません"))?;
    Ok(config_dir.join("aloud-code").join("config.toml"))
}

fn active_flag_path() -> Result<PathBuf> {
    // テストや特殊環境での上書きをサポート
    if let Ok(path) = std::env::var("ALOUD_CODE_STATE_FILE") {
        return Ok(PathBuf::from(path));
    }
    let state_dir = dirs::state_dir()
        .ok_or_else(|| anyhow::anyhow!("ステートディレクトリが見つかりません"))?;
    Ok(state_dir.join("aloud-code").join("active"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_when_no_file() {
        // デフォルト設定が返ることを確認（ファイルが存在しない場合）
        let config = Config::default();
        assert!(config.webhook.url.is_none());
    }

    #[test]
    fn test_config_parse_webhook_url() {
        // TOMLパースのテスト
        let toml_str = r#"
[webhook]
url = "https://hooks.slack.com/services/test"
"#;
        let config: Config = toml::from_str(toml_str).expect("パース失敗");
        assert_eq!(
            config.webhook.url.as_deref(),
            Some("https://hooks.slack.com/services/test")
        );
    }

    #[test]
    fn test_config_parse_invalid_toml() {
        let invalid_toml = "not valid toml {{{{";
        let result: Result<Config, _> = toml::from_str(invalid_toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_active_flag_lifecycle() {
        // フラグの作成・確認・削除をテスト
        // テスト後にクリーンアップするため、フラグが存在する場合は先に削除
        let _ = deactivate();
        assert!(!is_active());

        activate().expect("activate失敗");
        assert!(is_active());

        deactivate().expect("deactivate失敗");
        assert!(!is_active());
    }

    #[test]
    fn test_deactivate_idempotent() {
        // フラグが存在しなくてもdeactivateはエラーにならない
        let _ = deactivate();
        let result = deactivate();
        assert!(result.is_ok());
    }
}
