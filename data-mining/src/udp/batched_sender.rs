//! 📦 Batched UDP Sender - Event-driven with adaptive flushing
//!
//! Eliminates latency spikes by:
//! - Batching up to BATCH_MAX messages (256)
//! - Flushing after BATCH_MAX_LATENCY_MS (15ms) if batch not full
//! - Using event-driven tokio::select! (no busy waiting)

use anyhow::{Result, Context};
use tokio::sync::mpsc;
use tokio::net::UdpSocket;
use tokio::time::{interval, Duration, MissedTickBehavior};
use tracing::{debug, info, warn};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::os::unix::io::AsRawFd;
use socket2::SockAddr;


/// Maximum batch size before forcing flush
const BATCH_MAX: usize = 256;

/// Maximum latency to hold messages before flushing (ms)
const BATCH_MAX_LATENCY_MS: u64 = 15;

/// UDP message to send
#[derive(Debug, Clone)]
pub struct UdpMessage {
    pub data: Vec<u8>,
    pub target: String,
}

/// Batched UDP sender - runs in dedicated task
pub struct BatchedUdpSender {
    socket: Arc<UdpSocket>,
    batch: Vec<UdpMessage>,
    messages_sent: u64,
    batches_sent: u64,
}

impl BatchedUdpSender {
    /// Create new batched sender
    pub async fn new() -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.set_broadcast(false)?;
        
        Ok(Self {
            socket: Arc::new(socket),
            batch: Vec::with_capacity(BATCH_MAX),
            messages_sent: 0,
            batches_sent: 0,
        })
    }

    /// Main event loop - receives messages and flushes adaptively
    pub async fn run(mut self, mut rx: mpsc::UnboundedReceiver<UdpMessage>) {
        info!("📦 Batched UDP Sender started (max_batch={}, max_latency={}ms)", 
              BATCH_MAX, BATCH_MAX_LATENCY_MS);
        
        let mut flush_timer = interval(Duration::from_millis(BATCH_MAX_LATENCY_MS));
        flush_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                // Receive message from channel
                Some(msg) = rx.recv() => {
                    self.batch.push(msg);
                    
                    // Flush if batch is full (size-based trigger)
                    if self.batch.len() >= BATCH_MAX {
                        self.flush_batch();
                    }
                }
                
                // Periodic flush timer (time-based trigger)
                _ = flush_timer.tick() => {
                    if !self.batch.is_empty() {
                        self.flush_batch();
                    }
                }
            }
        }
    }

    /// Flush the current batch using sendmmsg (Linux-specific optimization)
    fn flush_batch(&mut self) {
        if self.batch.is_empty() {
            return;
        }

        let batch_size = self.batch.len();
        
        // Try to use sendmmsg for batched send (Linux-only)
        #[cfg(target_os = "linux")]
        {
            let success_count = self.flush_batch_sendmmsg();
            self.batches_sent += 1;
            
            if batch_size > 10 {
                debug!("📦 Flushed batch (sendmmsg): {} msgs ({} ok, {} total batches)",
                       batch_size, success_count, self.batches_sent);
            }
        }
        
        // Fallback for non-Linux systems
        #[cfg(not(target_os = "linux"))]
        {
            let success_count = self.flush_batch_fallback();
            self.batches_sent += 1;
            
            if batch_size > 10 {
                debug!("📦 Flushed batch (fallback): {} msgs ({} ok, {} total batches)",
                       batch_size, success_count, self.batches_sent);
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn flush_batch_sendmmsg(&mut self) -> usize {
        use std::mem::MaybeUninit;
        
        if self.batch.is_empty() {
            return 0;
        }

        let fd = self.socket.as_raw_fd();
        let mut success_count = 0;

        // Prepare mmsghdr structures
        let mut msgvec: Vec<libc::mmsghdr> = Vec::with_capacity(self.batch.len());
        let mut iovecs: Vec<libc::iovec> = Vec::with_capacity(self.batch.len());
        let mut addrs: Vec<SockAddr> = Vec::with_capacity(self.batch.len());

        // Build message structures
        for msg in &self.batch {
            // Parse target address
            let addr = match msg.target.parse::<std::net::SocketAddr>() {
                Ok(a) => SockAddr::from(a),
                Err(e) => {
                    warn!("⚠️  Invalid target address '{}': {}", msg.target, e);
                    continue;
                }
            };

            // Create iovec for message data
            let iov = libc::iovec {
                iov_base: msg.data.as_ptr() as *mut libc::c_void,
                iov_len: msg.data.len(),
            };
            iovecs.push(iov);
            addrs.push(addr);
        }

        // Build mmsghdr array
        for i in 0..iovecs.len() {
            let mut msghdr: libc::msghdr = unsafe { MaybeUninit::zeroed().assume_init() };
            msghdr.msg_name = addrs[i].as_ptr() as *mut libc::c_void;
            msghdr.msg_namelen = addrs[i].len();
            msghdr.msg_iov = &mut iovecs[i] as *mut libc::iovec;
            msghdr.msg_iovlen = 1;

            let mmsg = libc::mmsghdr {
                msg_hdr: msghdr,
                msg_len: 0,
            };
            msgvec.push(mmsg);
        }

        // Send batch with sendmmsg
        if !msgvec.is_empty() {
            let result = unsafe {
                libc::sendmmsg(
                    fd,
                    msgvec.as_mut_ptr(),
                    msgvec.len() as libc::c_uint,
                    0, // flags
                )
            };

            if result >= 0 {
                success_count = result as usize;
                self.messages_sent += success_count as u64;
            } else {
                let err = std::io::Error::last_os_error();
                warn!("❌ sendmmsg failed: {}", err);
            }
        }

        self.batch.clear();
        success_count
    }

    #[cfg(not(target_os = "linux"))]
    fn flush_batch_fallback(&mut self) -> usize {
        let mut success_count = 0;

        // Send all messages individually
        for msg in self.batch.drain(..) {
            // Parse target string to SocketAddr
            let addr = match msg.target.parse() {
                Ok(a) => a,
                Err(e) => {
                    warn!("⚠️  Invalid target address '{}': {}", msg.target, e);
                    continue;
                }
            };
            
            match self.socket.try_send_to(&msg.data, addr) {
                Ok(_) => {
                    success_count += 1;
                    self.messages_sent += 1;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Socket buffer full, drop message (non-blocking)
                    debug!("⚠️  UDP send would block, dropping message");
                }
                Err(e) => {
                    warn!("❌ UDP send error: {}", e);
                }
            }
        }

        success_count
    }
}

/// Create batched UDP sender and return channel for sending messages
pub fn spawn_batched_sender() -> mpsc::UnboundedSender<UdpMessage> {
    let (tx, rx) = mpsc::unbounded_channel();
    
    tokio::spawn(async move {
        match BatchedUdpSender::new().await {
            Ok(sender) => {
                sender.run(rx).await;
            }
            Err(e) => {
                warn!("❌ Failed to create batched UDP sender: {}", e);
            }
        }
    });
    
    info!("✅ Batched UDP Sender spawned");
    tx
}

/// Wrapper for advisory sender using batched backend
#[derive(Clone)]
pub struct BatchedAdvisorySender {
    tx: mpsc::UnboundedSender<UdpMessage>,
    target_addr: String,
}

impl BatchedAdvisorySender {
    pub fn new(tx: mpsc::UnboundedSender<UdpMessage>, host: &str, port: u16) -> Self {
        Self {
            tx,
            target_addr: format!("{}:{}", host, port),
        }
    }
    
    /// Send advisory packet (non-blocking, queued for batching)
    pub fn send(&self, data: Vec<u8>) -> Result<()> {
        self.tx.send(UdpMessage {
            data,
            target: self.target_addr.clone(),
        })?;
        Ok(())
    }
}

/// Wrapper for brain signal sender using batched backend
#[derive(Clone)]
pub struct BatchedBrainSignalSender {
    tx: mpsc::UnboundedSender<UdpMessage>,
    target_addr: String,
}

impl BatchedBrainSignalSender {
    pub fn new(tx: mpsc::UnboundedSender<UdpMessage>, host: &str, port: u16) -> Self {
        Self {
            tx,
            target_addr: format!("{}:{}", host, port),
        }
    }
    
    /// Send brain signal packet (non-blocking, queued for batching)
    pub fn send(&self, data: Vec<u8>) -> Result<()> {
        self.tx.send(UdpMessage {
            data,
            target: self.target_addr.clone(),
        })?;
        Ok(())
    }
    
    /// Send momentum detected signal
    pub fn send_momentum_detected(
        &self,
        mint_b58: &str,
        buys_in_last_500ms: u16,
        volume_sol: f32,
        unique_buyers: u16,
        confidence: u8,
    ) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        // Packet: [type(1) | mint(32) | buys(2) | volume(4) | buyers(2) | conf(1) | timestamp(8) | padding(14)]
        let mut msg = vec![0u8; 64];
        msg[0] = 21; // MomentumDetected type
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..35].copy_from_slice(&buys_in_last_500ms.to_le_bytes());
        msg[35..39].copy_from_slice(&volume_sol.to_le_bytes());
        msg[39..41].copy_from_slice(&unique_buyers.to_le_bytes());
        msg[41] = confidence.clamp(0, 100);
        msg[42..50].copy_from_slice(&timestamp_ns.to_le_bytes());
        
        self.send(msg)
    }
    
    /// Send volume spike signal
    pub fn send_volume_spike(
        &self,
        mint_b58: &str,
        total_sol: f32,
        tx_count: u16,
        time_window_ms: u16,
        confidence: u8,
    ) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        // Packet: [type(1) | mint(32) | total_sol(4) | tx_count(2) | window(2) | conf(1) | timestamp(8) | padding(14)]
        let mut msg = vec![0u8; 64];
        msg[0] = 22; // VolumeSpike type
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..37].copy_from_slice(&total_sol.to_le_bytes());
        msg[37..39].copy_from_slice(&tx_count.to_le_bytes());
        msg[39..41].copy_from_slice(&time_window_ms.to_le_bytes());
        msg[41] = confidence.clamp(0, 100);
        msg[42..50].copy_from_slice(&timestamp_ns.to_le_bytes());
        
        self.send(msg)
    }
    
    /// Send wallet activity signal
    pub fn send_wallet_activity(
        &self,
        mint_b58: &str,
        wallet_b58: &str,
        action: u8,
        size_sol: f32,
        wallet_tier: u8,
        confidence: u8,
    ) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        let wallet_bytes = bs58::decode(wallet_b58).into_vec()
            .context("Invalid wallet base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        if wallet_bytes.len() != 32 {
            anyhow::bail!("Wallet must be 32 bytes, got {}", wallet_bytes.len());
        }
        
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        // Packet: [type(1) | mint(32) | wallet(32) | action(1) | size(4) | tier(1) | conf(1) | timestamp(8)]
        let mut msg = vec![0u8; 80];
        msg[0] = 23; // WalletActivity type
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..65].copy_from_slice(&wallet_bytes);
        msg[65] = action;
        msg[66..70].copy_from_slice(&size_sol.to_le_bytes());
        msg[70] = wallet_tier;
        msg[71] = confidence.clamp(0, 100);
        msg[72..80].copy_from_slice(&timestamp_ns.to_le_bytes());
        
        self.send(msg)
    }
    
    /// Send window metrics signal
    pub fn send_window_metrics(
        &self,
        mint_b58: &str,
        volume_sol_1s: f64,
        unique_buyers_1s: u16,
        price_change_bps_2s: i16,
        alpha_wallet_hits_10s: u8,
    ) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        // Scale volume by 1000 to fit in u32
        let volume_scaled = (volume_sol_1s * 1000.0).min(u32::MAX as f64) as u32;
        
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        // Packet: [type(1) | mint(32) | volume(4) | buyers(2) | price_change(2) | alpha(1) | timestamp(8) | padding(13)]
        let mut msg = vec![0u8; 64];
        msg[0] = 29; // WindowMetrics type
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..37].copy_from_slice(&volume_scaled.to_le_bytes());
        msg[37..39].copy_from_slice(&unique_buyers_1s.to_le_bytes());
        msg[39..41].copy_from_slice(&price_change_bps_2s.to_le_bytes());
        msg[41] = alpha_wallet_hits_10s;
        msg[42..50].copy_from_slice(&timestamp_ns.to_le_bytes());
        
        self.send(msg)
    }
}
