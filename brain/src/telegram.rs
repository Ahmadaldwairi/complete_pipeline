//! 📱 Telegram Notification Client
//! 
//! Sends real-time trade alerts to configured Telegram chat.
//! Moved from Executor to Brain to enable immediate notifications
//! based on Brain's decision-making and position monitoring.

use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Telegram client for sending trade notifications
pub struct TelegramClient {
    client: Client,
    bot_token: String,
    chat_id: String,
    /// Rate limiting: Track last message timestamp
    last_message_time: Arc<RwLock<std::time::Instant>>,
    /// Minimum delay between messages (milliseconds)
    min_message_delay_ms: u64,
}

impl TelegramClient {
    /// Create a new Telegram client from config
    pub fn new(bot_token: String, chat_id: String) -> Self {
        TelegramClient {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to build HTTP client"),
            bot_token,
            chat_id,
            last_message_time: Arc::new(RwLock::new(std::time::Instant::now())),
            min_message_delay_ms: 100, // 100ms minimum between messages
        }
    }
    
    /// Send a raw text message to Telegram
    pub async fn send_message(&self, text: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Rate limiting
        {
            let mut last_time = self.last_message_time.write().await;
            let elapsed = last_time.elapsed();
            if elapsed.as_millis() < self.min_message_delay_ms as u128 {
                let wait_time = std::time::Duration::from_millis(
                    self.min_message_delay_ms - elapsed.as_millis() as u64
                );
                tokio::time::sleep(wait_time).await;
            }
            *last_time = std::time::Instant::now();
        }
        
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
    
    /// Send BUY confirmation notification
    pub async fn notify_buy_confirmed(
        &self,
        mint: &str,
        size_sol: f64,
        size_usd: f64,
        price: f64,
        tokens: f64,
        entry_strategy: &str,
        confidence: u8,
        signature: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let message = format!(
            "🟢 <b>BUY CONFIRMED</b> ✅\n\n\
            Token: <code>{}</code>\n\
            Size: {:.4} SOL (${:.2})\n\
            Price: {:.10} SOL/token\n\
            Tokens: {:.2}\n\
            Strategy: <b>{}</b>\n\
            Confidence: {}%\n\
            Signature: <code>{}</code>",
            &mint[..16],
            size_sol,
            size_usd,
            price,
            tokens,
            entry_strategy,
            confidence,
            &signature[..16]
        );
        
        self.send_message(&message).await
    }
    
    /// Send SELL confirmation notification
    pub async fn notify_sell_confirmed(
        &self,
        mint: &str,
        size_sol: f64,
        price: f64,
        profit_pct: f64,
        profit_sol: f64,
        profit_usd: f64,
        hold_time_secs: u64,
        exit_reason: &str,
        signature: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let profit_emoji = if profit_pct >= 0.0 { "💰" } else { "📉" };
        let profit_sign = if profit_pct >= 0.0 { "+" } else { "" };
        
        let message = format!(
            "{} <b>SELL CONFIRMED</b> ✅\n\n\
            Token: <code>{}</code>\n\
            Size: {:.4} SOL\n\
            Exit Price: {:.10} SOL/token\n\
            Profit: {}{:.2}% ({}{:.4} SOL / ${:.2})\n\
            Hold Time: {}s\n\
            Reason: <b>{}</b>\n\
            Signature: <code>{}</code>",
            profit_emoji,
            &mint[..16],
            size_sol,
            price,
            profit_sign,
            profit_pct,
            profit_sign,
            profit_sol,
            profit_usd,
            hold_time_secs,
            exit_reason,
            &signature[..16]
        );
        
        self.send_message(&message).await
    }
    
    /// Send BUY failed notification
    pub async fn notify_buy_failed(
        &self,
        mint: &str,
        reason: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let message = format!(
            "❌ <b>BUY FAILED</b>\n\n\
            Token: <code>{}</code>\n\
            Reason: {}\n\
            Status: Not entered",
            &mint[..16],
            reason
        );
        
        self.send_message(&message).await
    }
    
    /// Send SELL failed notification
    pub async fn notify_sell_failed(
        &self,
        mint: &str,
        retry_count: u8,
        max_retries: u8,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let message = format!(
            "⚠️ <b>SELL FAILED</b>\n\n\
            Token: <code>{}</code>\n\
            Retry: {}/{}\n\
            Status: Position still open",
            &mint[..16],
            retry_count,
            max_retries
        );
        
        self.send_message(&message).await
    }
    
    /// Send startup notification
    pub async fn notify_startup(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let message = "🤖 <b>Brain Started</b>\n\n\
            Decision Engine: Active\n\
            gRPC Monitor: Connected\n\
            Position Tracker: Ready\n\n\
            Status: Monitoring for opportunities...";
        
        self.send_message(message).await
    }
    
    /// Send emergency exit alert
    pub async fn notify_emergency_exit(
        &self,
        mint: &str,
        reason: &str,
        age_secs: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let message = format!(
            "🚨 <b>EMERGENCY EXIT</b>\n\n\
            Token: <code>{}</code>\n\
            Reason: {}\n\
            Age: {}s\n\n\
            Forcing exit immediately!",
            &mint[..16],
            reason,
            age_secs
        );
        
        self.send_message(&message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_telegram_client_creation() {
        let client = TelegramClient::new(
            "test_token".to_string(),
            "test_chat_id".to_string(),
        );
        
        assert_eq!(client.bot_token, "test_token");
        assert_eq!(client.chat_id, "test_chat_id");
    }
}
