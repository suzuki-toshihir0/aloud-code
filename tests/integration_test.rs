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
    state_dir: std::path::PathBuf,
    _temp_dir: tempfile::TempDir,
}

impl TestEnv {
    fn new() -> Self {
        let temp_dir = tempfile::TempDir::new().expect("一時ディレクトリ作成失敗");
        let config_file = temp_dir.path().join("config.toml");
        let state_dir = temp_dir.path().to_path_buf();
        TestEnv {
            config_file,
            state_dir,
            _temp_dir: temp_dir,
        }
    }

    fn set_webhook_url(&self, url: &str) {
        std::fs::write(&self.config_file, format!("[webhook]\nurl = \"{}\"\n", url))
            .expect("config.toml書き込み失敗");
    }

    /// セッションのカーソル値を手動設定する（テスト用）
    fn set_cursor(&self, session_id: &str, value: u64) {
        let sessions_dir = self.state_dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        std::fs::write(
            sessions_dir.join(format!("{}.cursor", session_id)),
            value.to_string(),
        )
        .unwrap();
    }

    /// トランスクリプトファイルを作成してパスを返す
    fn create_transcript(&self, entries: &[serde_json::Value]) -> std::path::PathBuf {
        let transcript_path = self._temp_dir.path().join("transcript.jsonl");
        let mut content = String::new();
        for entry in entries {
            content.push_str(&entry.to_string());
            content.push('\n');
        }
        std::fs::write(&transcript_path, &content).expect("トランスクリプト書き込み失敗");
        transcript_path
    }

    async fn run_hook(&self, event: &str, input_json: &str) -> std::process::Output {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new(binary_path())
            .args(["hook", event])
            .env("ALOUD_CODE_CONFIG_FILE", &self.config_file)
            .env("ALOUD_CODE_STATE_DIR", &self.state_dir)
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

    // /aloud-code:on でONにする
    let toggle_input = json!({
        "session_id": "test-session-12345678",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/aloud-code:on"
    });
    let output = env.run_hook("toggle", &toggle_input.to_string()).await;
    assert!(output.status.success(), "toggle失敗: {:?}", output);

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
    assert!(
        text.contains(":bust_in_silhouette:"),
        "ユーザー絵文字がない"
    );
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

    // OFFのまま（toggleしない）でuser-promptを実行
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
async fn test_stop_hook_sends_assistant_message_via_transcript() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    // /aloud-code:on でONにする
    let toggle_input = json!({
        "session_id": "test-session-stop",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/aloud-code:on"
    });
    let output = env.run_hook("toggle", &toggle_input.to_string()).await;
    assert!(output.status.success(), "toggle失敗");

    // トランスクリプトファイルにassistantメッセージを書き込む
    let transcript_path = env.create_transcript(&[json!({
        "type": "assistant",
        "message": {
            "content": [{"type": "text", "text": "I've completed the task!"}]
        }
    })]);

    // カーソルを0に設定（フラッシュ済みセッションをシミュレート）
    env.set_cursor("test-session-stop", 0);

    let input = json!({
        "session_id": "test-session-stop",
        "cwd": "/home/user/proj",
        "hook_event_name": "Stop",
        "transcript_path": transcript_path.to_str().unwrap(),
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
async fn test_toggle_lifecycle() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    let sessions_dir = env.state_dir.join("sessions");

    // 初期状態はOFF
    assert!(!sessions_dir.exists(), "初期状態はOFFのはず");

    // /aloud-code:on でONにする
    let toggle_on = json!({
        "session_id": "lifecycle-session",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/aloud-code:on"
    });
    let output = env.run_hook("toggle", &toggle_on.to_string()).await;
    assert!(output.status.success());
    assert!(
        sessions_dir.join("lifecycle-session").exists(),
        "enable後はフラグが存在するはず"
    );

    // /aloud-code:off でOFFにする
    let toggle_off = json!({
        "session_id": "lifecycle-session",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/aloud-code:off"
    });
    let output = env.run_hook("toggle", &toggle_off.to_string()).await;
    assert!(output.status.success());
    assert!(
        !sessions_dir.join("lifecycle-session").exists(),
        "disable後はフラグが消えるはず"
    );
}

#[tokio::test]
async fn test_no_webhook_for_different_session() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    // session-a でON
    let toggle_on = json!({
        "session_id": "session-a",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/aloud-code:on"
    });
    let output = env.run_hook("toggle", &toggle_on.to_string()).await;
    assert!(output.status.success());

    // session-b でuser-promptを実行（session-aとは異なるセッションID）
    let input = json!({
        "session_id": "session-b",
        "cwd": "/tmp",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "This should NOT be sent because session-b is not active"
    });
    let output = env.run_hook("user-prompt", &input.to_string()).await;
    assert!(output.status.success());

    // Webhookが届いていないことを確認
    let requests = mock_server.received_requests().await.unwrap();
    assert!(
        requests.is_empty(),
        "異なるセッションIDなのにWebhookが届いた"
    );
}

#[tokio::test]
async fn test_subagent_stop_sends_agent_message() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    // セッションON
    let toggle_on = json!({
        "session_id": "subagent-session",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/aloud-code:on"
    });
    env.run_hook("toggle", &toggle_on.to_string()).await;

    let input = json!({
        "session_id": "subagent-session",
        "cwd": "/home/user/proj",
        "hook_event_name": "SubagentStop",
        "agent_type": "Explore",
        "last_assistant_message": "Found 3 relevant files."
    });
    let output = env.run_hook("subagent-stop", &input.to_string()).await;
    assert!(
        output.status.success(),
        "subagent-stop hook失敗: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert!(!requests.is_empty(), "SubagentStopでWebhookが届いていない");

    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let text = body["blocks"][0]["text"]["text"].as_str().unwrap();
    assert!(text.contains(":gear:"), "gear絵文字がない");
    assert!(text.contains("Explore"), "agent_typeが含まれていない");
    assert!(
        text.contains("Found 3 relevant files."),
        "サブエージェントメッセージが含まれていない: {}",
        text
    );
}

#[tokio::test]
async fn test_notification_sends_message() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    // セッションON
    let toggle_on = json!({
        "session_id": "notif-session",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/aloud-code:on"
    });
    env.run_hook("toggle", &toggle_on.to_string()).await;

    let input = json!({
        "session_id": "notif-session",
        "cwd": "/home/user/proj",
        "hook_event_name": "Notification",
        "message": "Which approach do you prefer?"
    });
    let output = env.run_hook("notification", &input.to_string()).await;
    assert!(
        output.status.success(),
        "notification hook失敗: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert!(!requests.is_empty(), "NotificationでWebhookが届いていない");

    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let text = body["blocks"][0]["text"]["text"].as_str().unwrap();
    assert!(
        text.contains(":speech_balloon:"),
        "speech_balloon絵文字がない"
    );
    assert!(
        text.contains("Which approach do you prefer?"),
        "通知メッセージが含まれていない: {}",
        text
    );
}

#[tokio::test]
async fn test_session_end_deactivates_session() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    // セッションON
    let toggle_on = json!({
        "session_id": "end-session",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/aloud-code:on"
    });
    env.run_hook("toggle", &toggle_on.to_string()).await;

    let sessions_dir = env.state_dir.join("sessions");
    assert!(sessions_dir.join("end-session").exists(), "ONのはず");

    // session-end を実行
    let input = json!({
        "session_id": "end-session",
        "cwd": "/home/user/proj",
        "hook_event_name": "SessionEnd"
    });
    let output = env.run_hook("session-end", &input.to_string()).await;
    assert!(
        output.status.success(),
        "session-end hook失敗: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // セッションが非アクティブになっていることを確認
    assert!(
        !sessions_dir.join("end-session").exists(),
        "session-end後もフラグが残っている"
    );
}

#[tokio::test]
async fn test_no_historical_messages_on_first_activation() {
    // 有効化直後の初回フラッシュで過去メッセージが送信されないことを確認
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    // セッションON
    let toggle_on = json!({
        "session_id": "fresh-session",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/aloud-code:on"
    });
    env.run_hook("toggle", &toggle_on.to_string()).await;

    // 有効化前から存在するトランスクリプト（過去メッセージ）
    let transcript_path = env.create_transcript(&[json!({
        "type": "assistant",
        "message": {
            "content": [{"type": "text", "text": "This is a historical message."}]
        }
    })]);

    // 初回フラッシュ: カーソルファイルなし → 過去メッセージは送信しない
    let input = json!({
        "session_id": "fresh-session",
        "cwd": "/home/user/proj",
        "hook_event_name": "Stop",
        "transcript_path": transcript_path.to_str().unwrap()
    });
    let output = env.run_hook("stop", &input.to_string()).await;
    assert!(
        output.status.success(),
        "stop hook失敗: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert!(
        requests.is_empty(),
        "初回フラッシュで過去メッセージが送信された（{}件）",
        requests.len()
    );
}

#[tokio::test]
async fn test_flush_transcript_sends_assistant_texts() {
    let env = TestEnv::new();
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock_server)
        .await;

    env.set_webhook_url(&format!("{}/webhook", mock_server.uri()));

    // セッションON
    let toggle_on = json!({
        "session_id": "flush-session",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "/aloud-code:on"
    });
    env.run_hook("toggle", &toggle_on.to_string()).await;

    // カーソルを0に設定（フラッシュ済みセッションをシミュレート）
    env.set_cursor("flush-session", 0);

    // トランスクリプトに2つのassistantメッセージ
    let transcript_path = env.create_transcript(&[
        json!({
            "type": "assistant",
            "message": {
                "content": [{"type": "text", "text": "First assistant response."}]
            }
        }),
        json!({
            "type": "user",
            "message": {"content": [{"type": "text", "text": "user msg"}]}
        }),
        json!({
            "type": "assistant",
            "message": {
                "content": [{"type": "text", "text": "Second assistant response."}]
            }
        }),
    ]);

    // SubagentStopイベントでフラッシュが走る
    let input = json!({
        "session_id": "flush-session",
        "cwd": "/home/user/proj",
        "hook_event_name": "SubagentStop",
        "agent_type": "general-purpose",
        "last_assistant_message": "Subagent result.",
        "transcript_path": transcript_path.to_str().unwrap()
    });
    let output = env.run_hook("subagent-stop", &input.to_string()).await;
    assert!(
        output.status.success(),
        "subagent-stop hook失敗: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let requests = mock_server.received_requests().await.unwrap();
    // フラッシュ2件 + サブエージェント固有1件 = 合計3件
    assert_eq!(requests.len(), 3, "送信件数が期待と異なる: {:?}", requests);

    // 最初の2件はフラッシュ（assistantテキスト）
    let text0 = requests[0].body_json::<serde_json::Value>().unwrap();
    let text0 = text0["blocks"][0]["text"]["text"].as_str().unwrap();
    assert!(
        text0.contains("First assistant response."),
        "1件目: {}",
        text0
    );

    let text1 = requests[1].body_json::<serde_json::Value>().unwrap();
    let text1 = text1["blocks"][0]["text"]["text"].as_str().unwrap();
    assert!(
        text1.contains("Second assistant response."),
        "2件目: {}",
        text1
    );

    // 3件目はサブエージェント固有の送信
    let text2 = requests[2].body_json::<serde_json::Value>().unwrap();
    let text2 = text2["blocks"][0]["text"]["text"].as_str().unwrap();
    assert!(text2.contains(":gear:"), "3件目にgear絵文字がない");
    assert!(text2.contains("Subagent result."), "3件目: {}", text2);
}
