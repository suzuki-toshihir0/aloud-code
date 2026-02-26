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

pub fn is_active(session_id: &str) -> bool {
    sessions_dir()
        .map(|d| d.join(session_id).exists())
        .unwrap_or(false)
}

pub fn activate(session_id: &str) -> Result<()> {
    let dir = sessions_dir()?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(session_id), "")?;
    Ok(())
}

pub fn deactivate(session_id: &str) -> Result<()> {
    let path = sessions_dir()?.join(session_id);
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
    let config_dir =
        dirs::config_dir().ok_or_else(|| anyhow::anyhow!("設定ディレクトリが見つかりません"))?;
    Ok(config_dir.join("aloud-code").join("config.toml"))
}

fn sessions_dir() -> Result<PathBuf> {
    // テストや特殊環境での上書きをサポート
    if let Ok(dir) = std::env::var("ALOUD_CODE_STATE_DIR") {
        return Ok(PathBuf::from(dir).join("sessions"));
    }
    let state_dir =
        dirs::state_dir().ok_or_else(|| anyhow::anyhow!("ステートディレクトリが見つかりません"))?;
    Ok(state_dir.join("aloud-code").join("sessions"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // テスト間でのALOUD_CODE_STATE_DIR環境変数競合を防ぐMutex
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// テスト用に一時ディレクトリをALOUD_CODE_STATE_DIRに設定してクロージャを実行する
    fn with_temp_state_dir<F: FnOnce()>(f: F) {
        let _guard = ENV_MUTEX.lock().unwrap();
        let temp_dir = tempfile::TempDir::new().expect("一時ディレクトリ作成失敗");
        std::env::set_var("ALOUD_CODE_STATE_DIR", temp_dir.path());
        f();
        std::env::remove_var("ALOUD_CODE_STATE_DIR");
    }

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
        // セッションIDごとのフラグ作成・確認・削除をテスト
        with_temp_state_dir(|| {
            let session_id = "test-session-lifecycle";
            let _ = deactivate(session_id);
            assert!(!is_active(session_id));

            activate(session_id).expect("activate失敗");
            assert!(is_active(session_id));

            deactivate(session_id).expect("deactivate失敗");
            assert!(!is_active(session_id));
        });
    }

    #[test]
    fn test_deactivate_idempotent() {
        // フラグが存在しなくてもdeactivateはエラーにならない
        with_temp_state_dir(|| {
            let _ = deactivate("nonexistent-session");
            let result = deactivate("nonexistent-session");
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_multiple_sessions_concurrent() {
        // 複数セッションが同時にONにできることを確認
        with_temp_state_dir(|| {
            activate("session-a").expect("session-a activate失敗");
            activate("session-b").expect("session-b activate失敗");

            assert!(is_active("session-a"), "session-aがアクティブでない");
            assert!(is_active("session-b"), "session-bがアクティブでない");
            assert!(!is_active("session-c"), "session-cがアクティブになっている");
        });
    }
}
