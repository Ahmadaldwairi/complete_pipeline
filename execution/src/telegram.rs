use crate::config::Config;
use reqwest::Client;
use serde_json::json;

pub struct TelegramClient {
    client: Client,
    bot_token: String,
    chat_id: String,
}

impl TelegramClient {
    pub fn new(config: &Config) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(TelegramClient {
            client: Client::new(),
            bot_token: config.telegram_bot_token.clone(),
            chat_id: config.telegram_chat_id.clone(),
        })
    }
    
    pub async fn send_message(&self, text: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );
        
        let payload = json!({
            "chat_id": self.chat_id,
            "text": text,
            "parse_mode": "HTML"
        });
        
        let response = self.client
            .post(&url)
            .json(&payload)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("Telegram API error: {}", error_text).into());
        }
        
        Ok(())
    }
    
    pub async fn send_alert(&self, token: &str, profit: f64, volume: f64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let message = format!(
            "🚨 <b>BIG PUMP DETECTED!</b>\n\n\
            Token: <code>{}</code>\n\
            Current Profit: ${:.2}\n\
            Pending Volume: {:.1} SOL\n\n\
            🔴 <b>AUTO-SELL DISABLED</b>\n\
            Exit manually when ready!",
            &token[..16], profit, volume
        );
        
        self.send_message(&message).await
    }
}