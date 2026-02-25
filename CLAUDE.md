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

## リリース手順

```bash
git tag -a vX.Y.Z -m "vX.Y.Z" && git push origin vX.Y.Z
```

タグを push すると GitHub Actions が以下を自動実行する:
1. `plugin.json` のバージョンを `X.Y.Z` に更新して main に commit
2. Linux/macOS 向けバイナリをビルドして GitHub Releases にアップロード

ユーザーが `claude plugin update aloud-code` を実行すると、次回 hook 発火時に新バイナリが自動ダウンロードされる（バージョンファイル: `~/.local/state/aloud-code/installed_version`）。
