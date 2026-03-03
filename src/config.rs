use anyhow::Result;
use fs2::FileExt;
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

/// セッションごとのカーソルロックガード
/// acquire() でファイルロックを取得し、commit() で更新・解放する
pub struct CursorLockGuard {
    lock_file: std::fs::File,
    session_id: String,
}

impl CursorLockGuard {
    /// ファイルロックを排他取得してカーソル値を返す
    /// カーソルファイルが存在しない（初回有効化・再有効化後）場合は None を返す
    pub fn acquire(session_id: &str) -> Result<(Self, Option<u64>)> {
        let dir = sessions_dir()?;
        std::fs::create_dir_all(&dir)?;
        let lock_path = dir.join(format!("{}.cursor.lock", session_id));
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        lock_file.lock_exclusive()?;
        let cursor = read_cursor_inner(session_id);
        Ok((
            CursorLockGuard {
                lock_file,
                session_id: session_id.to_string(),
            },
            cursor,
        ))
    }

    /// カーソルを新しい値に更新してロックを解放する（送信成功後に呼び出す）
    pub fn commit(self, new_cursor: u64) -> Result<()> {
        write_cursor_inner(&self.session_id, new_cursor)?;
        self.lock_file.unlock()?;
        Ok(())
    }
}

impl Drop for CursorLockGuard {
    fn drop(&mut self) {
        let _ = self.lock_file.unlock();
    }
}

/// カーソルファイルが存在しない場合は None を返す（初回有効化を検出するため）
fn read_cursor_inner(session_id: &str) -> Option<u64> {
    let path = cursor_path(session_id).ok()?;
    if !path.exists() {
        return None;
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn write_cursor_inner(session_id: &str, cursor: u64) -> Result<()> {
    let path = cursor_path(session_id)?;
    std::fs::write(path, cursor.to_string())?;
    Ok(())
}

fn cursor_path(session_id: &str) -> Result<PathBuf> {
    Ok(sessions_dir()?.join(format!("{}.cursor", session_id)))
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
    let dir = sessions_dir()?;
    let paths = [
        dir.join(session_id),
        dir.join(format!("{}.cursor", session_id)),
        dir.join(format!("{}.cursor.lock", session_id)),
    ];
    for path in &paths {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
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
        let config = Config::default();
        assert!(config.webhook.url.is_none());
    }

    #[test]
    fn test_config_parse_webhook_url() {
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
        with_temp_state_dir(|| {
            let _ = deactivate("nonexistent-session");
            let result = deactivate("nonexistent-session");
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_multiple_sessions_concurrent() {
        with_temp_state_dir(|| {
            activate("session-a").expect("session-a activate失敗");
            activate("session-b").expect("session-b activate失敗");

            assert!(is_active("session-a"), "session-aがアクティブでない");
            assert!(is_active("session-b"), "session-bがアクティブでない");
            assert!(!is_active("session-c"), "session-cがアクティブになっている");
        });
    }

    #[test]
    fn test_cursor_read_write() {
        with_temp_state_dir(|| {
            let session_id = "cursor-test-session";
            // セッションを有効化してディレクトリを作成
            activate(session_id).expect("activate失敗");
            write_cursor_inner(session_id, 12345).expect("cursor書き込み失敗");
            let cursor = read_cursor_inner(session_id);
            assert_eq!(cursor, Some(12345));
        });
    }

    #[test]
    fn test_cursor_default_none() {
        with_temp_state_dir(|| {
            // カーソルファイルがない場合は None（初回有効化を検出）
            activate("no-cursor-session").expect("activate失敗");
            let cursor = read_cursor_inner("no-cursor-session");
            assert!(cursor.is_none());
        });
    }

    #[test]
    fn test_cursor_deleted_on_deactivate() {
        with_temp_state_dir(|| {
            let session_id = "deactivate-cursor-session";
            activate(session_id).expect("activate失敗");
            write_cursor_inner(session_id, 999).expect("cursor書き込み失敗");

            let dir = sessions_dir().unwrap();
            assert!(dir.join(format!("{}.cursor", session_id)).exists());

            deactivate(session_id).expect("deactivate失敗");
            assert!(
                !dir.join(session_id).exists(),
                "セッションファイルが残っている"
            );
            assert!(
                !dir.join(format!("{}.cursor", session_id)).exists(),
                "cursorファイルが残っている"
            );
        });
    }

    #[test]
    fn test_cursor_lock_acquire_commit() {
        with_temp_state_dir(|| {
            let session_id = "lock-test-session";
            activate(session_id).expect("activate失敗");

            // 初回: cursor=None（カーソルファイルなし = 初回有効化）
            let (guard, cursor) = CursorLockGuard::acquire(session_id).expect("acquire失敗");
            assert!(cursor.is_none());
            guard.commit(500).expect("commit失敗");

            // 2回目: cursor=Some(500)
            let (guard2, cursor2) = CursorLockGuard::acquire(session_id).expect("acquire失敗");
            assert_eq!(cursor2, Some(500));
            guard2.commit(1000).expect("commit失敗");

            // 3回目: cursor=Some(1000)
            let (_, cursor3) = CursorLockGuard::acquire(session_id).expect("acquire失敗");
            assert_eq!(cursor3, Some(1000));
        });
    }
}
