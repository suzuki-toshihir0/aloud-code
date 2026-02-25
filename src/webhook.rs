use anyhow::Result;
use serde_json::Value;
use std::time::Duration;

pub struct WebhookSender {
    url: String,
    client: reqwest::Client,
}

impl WebhookSender {
    pub fn new(url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("HTTPクライアントの初期化に失敗");
        WebhookSender { url, client }
    }

    pub async fn send(&self, payload: Value) -> Result<()> {
        let mut last_err = None;
        let delays = [100u64, 200, 400];

        for (attempt, delay_ms) in delays.iter().enumerate() {
            match self.client.post(&self.url).json(&payload).send().await {
                Ok(resp) if resp.status().is_success() => {
                    return Ok(());
                }
                Ok(resp) => {
                    let status = resp.status();
                    last_err = Some(anyhow::anyhow!("HTTPエラー: {}", status));
                    if attempt < delays.len() - 1 {
                        tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
                    }
                }
                Err(e) => {
                    last_err = Some(anyhow::anyhow!("送信エラー: {}", e));
                    if attempt < delays.len() - 1 {
                        tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("不明なエラー")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_send_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let url = format!("{}/webhook", mock_server.uri());
        let sender = WebhookSender::new(url);
        let result = sender.send(json!({"text": "test"})).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_retry_then_success() {
        let mock_server = MockServer::start().await;
        // 最初の2回は500、3回目は200を返す
        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let url = format!("{}/webhook", mock_server.uri());
        let sender = WebhookSender::new(url);
        let result = sender.send(json!({"text": "test"})).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_all_retries_fail() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let url = format!("{}/webhook", mock_server.uri());
        let sender = WebhookSender::new(url);
        let result = sender.send(json!({"text": "test"})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_invalid_url() {
        let sender = WebhookSender::new("http://localhost:1".to_string());
        let result = sender.send(json!({"text": "test"})).await;
        assert!(result.is_err());
    }
}
