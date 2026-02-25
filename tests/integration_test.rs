use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn binary_path() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // deps/
    p.pop(); // debug/
    p.push("aloud-code");
    p
}

struct TestEnv {
    config_file: std::path::PathBuf,
    state_file: std::path::PathBuf,
    _temp_dir: tempfile::TempDir,
}

impl TestEnv {
    fn new() -> Self {
        let temp_dir = tempfile::TempDir::new().expect("一時ディレクトリ作成失敗");
        let config_file = temp_dir.path().join("config.toml");
        let state_file = temp_dir.path().join("active");
        TestEnv {
            config_file,
            state_file,
            _temp_dir: temp_dir,
        }
    }

    fn set_webhook_url(&self, url: &str) {
        std::fs::write(&self.config_file, format!("[webhook]\nurl = \"{}\"\n", url))
            .expect("config.toml書き込み失敗");
    }

    fn run_command(&self, args: &[&str]) -> std::process::Output {
        std::process::Command::new(binary_path())
            .args(args)
            .env("ALOUD_CODE_CONFIG_FILE", &self.config_file)
            .env("ALOUD_CODE_STATE_FILE", &self.state_file)
            .output()
            .expect("コマンド実行失敗")
    }

    async fn run_hook(&self, event: &str, input_json: &str) -> std::process::Output {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new(binary_path())
            .args(["hook", event])
            .env("ALOUD_CODE_CONFIG_FILE", &self.config_file)
            .env("ALOUD_CODE_STATE_FILE", &self.state_file)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("バイナリ起動失敗");

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input_json.as_bytes()).unwrap();
        }

        child.wait_with_output().expect("バイナリ終了待機失敗")
    }
}

#[tokio::test]
async fn test_user_prompt_webhook_when_enabled() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock_server)
        .await;

    let webhook_url = format!("{}/webhook", mock_server.uri());
    env.set_webhook_url(&webhook_url);

    // ONにする
    let output = env.run_command(&["enable"]);
    assert!(output.status.success(), "enable失敗: {:?}", output);

    // user-promptフックを実行
    let input = json!({
        "session_id": "test-session-12345678",
        "cwd": "/home/user/test-project",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "Hello from integration test!",
        "model": "claude-sonnet-4-6"
    });
    let output = env.run_hook("user-prompt", &input.to_string()).await;
    assert!(
        output.status.success(),
        "user-prompt hook失敗: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // モックサーバーにリクエストが届いたか確認
    let requests = mock_server.received_requests().await.unwrap();
    assert!(!requests.is_empty(), "Webhookリクエストが届いていない");

    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let text = body["blocks"][0]["text"]["text"].as_str().unwrap();
    assert!(text.contains(":bust_in_silhouette:"), "ユーザー絵文字がない");
    assert!(
        text.contains("Hello from integration test!"),
        "プロンプトが含まれていない: {}",
        text
    );
    assert_eq!(body["username"], "test-project [test-ses]");
}

#[tokio::test]
async fn test_no_webhook_when_disabled() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    // OFFのまま（enableしない）でuser-promptを実行
    let input = json!({
        "session_id": "test-session-off",
        "cwd": "/tmp",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "This should NOT be sent"
    });
    let output = env.run_hook("user-prompt", &input.to_string()).await;
    assert!(output.status.success(), "hook実行失敗");

    // モックサーバーにリクエストが届いていないことを確認
    let requests = mock_server.received_requests().await.unwrap();
    assert!(
        requests.is_empty(),
        "OFF状態なのにWebhookリクエストが届いた"
    );
}

#[tokio::test]
async fn test_stop_hook_sends_assistant_message() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    // ONにする
    let output = env.run_command(&["enable"]);
    assert!(output.status.success(), "enable失敗");

    let input = json!({
        "session_id": "test-session-stop",
        "cwd": "/home/user/proj",
        "hook_event_name": "Stop",
        "last_assistant_message": "I've completed the task!"
    });
    let output = env.run_hook("stop", &input.to_string()).await;
    assert!(
        output.status.success(),
        "stop hook失敗: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert!(!requests.is_empty(), "stopフックでWebhookが届いていない");

    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let text = body["blocks"][0]["text"]["text"].as_str().unwrap();
    assert!(text.contains(":robot_face:"), "Claude絵文字がない");
    assert!(
        text.contains("I've completed the task!"),
        "アシスタントメッセージが含まれていない: {}",
        text
    );
}

#[tokio::test]
async fn test_enable_disable_lifecycle() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    // 初期状態はOFF
    assert!(!env.state_file.exists(), "初期状態はOFFのはず");

    // ONにする
    let output = env.run_command(&["enable"]);
    assert!(output.status.success());
    assert!(env.state_file.exists(), "enable後はフラグが存在するはず");

    // OFFにする
    let output = env.run_command(&["disable"]);
    assert!(output.status.success());
    assert!(!env.state_file.exists(), "disable後はフラグが消えるはず");

    // もう一度OFFにしてもエラーにならない
    let output = env.run_command(&["disable"]);
    assert!(output.status.success());
}
