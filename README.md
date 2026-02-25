# aloud-code

***<span style="font-size: 140%">Work Out Loud with your Claude Code!</span>***

Claude Code plugin that streams your conversations to Slack or any webhook endpoint in real-time.

![screenshot](img/screenshot.png)

## Installation

### 1. Add the plugin to Claude Code

```bash
claude plugin marketplace add suzuki-toshihir0/aloud-code && claude plugin install aloud-code
```

The binary is downloaded automatically on first use.

### 2. Configure your webhook

```bash
mkdir -p ~/.config/aloud-code
cat > ~/.config/aloud-code/config.toml << 'EOF'
[webhook]
url = "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
EOF
```

## Usage

### Toggle streaming in a session

```
/aloud-code:on   # start streaming this session
/aloud-code:off  # stop streaming
```

That's it — once enabled, every user prompt and Claude response is automatically sent to your webhook.

## Slack Output Format

Messages appear in Slack with the project name and session ID as the sender:

```
my-project [a1b2c3d4]
👤 User
Help me implement a file watcher in Rust

my-project [a1b2c3d4]
🤖 Claude
I'll help you create a file watcher in Rust...
```

Session lifecycle events are also sent:

```
🟢 Session started
cwd: `/home/user/my-project`
model: `claude-sonnet-4-6`

🔴 Session ended
```

## Configuration

`~/.config/aloud-code/config.toml`:

```toml
[webhook]
url = "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
```

The plugin is **OFF by default** each session. Use `/aloud-code:on` to enable.
