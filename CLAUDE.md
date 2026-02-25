# aloud-code

Claude Codeプラグイン。hookイベントを受け取り、Slack等のwebhookにユーザーとClaudeの会話を送信する。

## アーキテクチャ

純Hook型プラグイン。Claude Codeのhookイベント（stdin）からデータを取得し、Slack Block Kit形式でWebhookに送信する。

## 主要ファイル

- `src/main.rs`: エントリポイント。`hook <event>` / `enable` / `disable` サブコマンドを処理
- `src/hook.rs`: hookハンドラ。stdinからHookInputをデシリアライズし、処理を分岐
- `src/formatter.rs`: Slack Block Kit メッセージ整形
- `src/webhook.rs`: HTTP POST送信（リトライ付き）
- `src/config.rs`: 設定読み込み（TOML）+ ON/OFFフラグ管理

## 設定

`~/.config/aloud-code/config.toml`:
```toml
[webhook]
url = "https://hooks.slack.com/services/..."
```

ON/OFFフラグ: `~/.local/state/aloud-code/active`（ファイル存在=ON）

## 開発コマンド

```bash
cargo build
cargo test
```
