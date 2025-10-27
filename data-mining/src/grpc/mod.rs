use anyhow::{Context, Result};
use futures::{Stream, StreamExt};
use std::collections::HashMap;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, SubscribeRequest, SubscribeUpdate,
    SubscribeRequestFilterTransactions,
    SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterSlots, CommitmentLevel,
};

use crate::config::GrpcConfig;

pub struct YellowstoneClient {
    endpoint: String,
    max_retries: u32,
    retry_delay: Duration,
}

impl YellowstoneClient {
    pub fn new(config: &GrpcConfig) -> Self {
        Self {
            endpoint: config.endpoint.clone(),
            max_retries: config.max_retries,
            retry_delay: Duration::from_secs(config.retry_delay_secs),
        }
    }

    pub async fn connect_and_subscribe(
        &self,
        request: SubscribeRequest,
    ) -> Result<impl futures::Stream<Item = Result<UpdateOneof>>> {
        let mut attempts = 0;
        
        loop {
            attempts += 1;
            
            match GeyserGrpcClient::build_from_shared(self.endpoint.clone()) {
                Ok(client_builder) => {
                    match client_builder.connect().await {
                        Ok(mut client) => {
                            info!("🔗 Connected to Yellowstone gRPC at {}", self.endpoint);
                            
                            // Subscribe
                            let (_subscribe_tx, stream) = client
                                .subscribe_with_request(Some(request.clone()))
                                .await
                                .context("Failed to create subscription")?;
                            
                            info!("✅ Subscription established");
                            return Ok(Self::create_stream(stream));
                        }
                        Err(e) => {
                            if attempts >= self.max_retries {
                                return Err(anyhow::anyhow!(
                                    "Failed to connect after {} attempts: {}",
                                    self.max_retries,
                                    e
                                ));
                            }
                            warn!(
                                "⚠️  Connection attempt {}/{} failed: {}. Retrying in {:?}...",
                                attempts, self.max_retries, e, self.retry_delay
                            );
                            sleep(self.retry_delay).await;
                        }
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Failed to build gRPC client: {}", e));
                }
            }
        }
    }

    fn create_stream(
        mut stream: impl Stream<Item = Result<SubscribeUpdate, tonic::Status>> + Unpin + Send + 'static,
    ) -> impl futures::Stream<Item = Result<UpdateOneof>> {
        async_stream::stream! {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(msg) => {
                        if let Some(update) = msg.update_oneof {
                            yield Ok(update);
                        }
                    }
                    Err(e) => {
                        error!("Stream error: {}", e);
                        yield Err(anyhow::anyhow!("Stream error: {}", e));
                        break;
                    }
                }
            }
        }
    }

    pub fn create_subscription_request(
        &self,
        pump_program: &str,
        _spl_token_program: &str,
        raydium_program: Option<&str>,
        _start_slot: Option<u64>,
    ) -> SubscribeRequest {
        let accounts = HashMap::new();
        let mut transactions = HashMap::new();
        let blocks = HashMap::new();
        let mut slots = HashMap::new();
        let mut blocks_meta = HashMap::new();

        // Subscribe to transactions involving pump.fun program
        let mut account_include = vec![pump_program.to_string()];
        if let Some(raydium) = raydium_program {
            account_include.push(raydium.to_string());
        }

        transactions.insert(
            "pump_txs".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include,
                account_exclude: vec![],
                account_required: vec![],
            },
        );

        // Subscribe to slot updates for slot tracking
        slots.insert("slots".to_string(), SubscribeRequestFilterSlots {
            filter_by_commitment: None,
            interslot_updates: Some(false),
        });

        // Subscribe to block metadata
        blocks_meta.insert("blocks_meta".to_string(), SubscribeRequestFilterBlocksMeta {});

        SubscribeRequest {
            accounts,
            slots,
            transactions,
            blocks,
            blocks_meta,
            entry: HashMap::new(),
            commitment: Some(CommitmentLevel::Confirmed as i32),
            accounts_data_slice: vec![],
            ping: None,
            transactions_status: HashMap::new(),
            from_slot: None,
        }
    }
}
