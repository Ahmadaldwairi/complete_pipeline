# Configuration Guide

## Overview

The Brain service uses environment variables for configuration, loaded from a `.env` file or the system environment. All parameters have sensible defaults, so the service can run without a `.env` file, but production deployments should customize these values.

## Quick Start

1. **Copy the example configuration:**
   ```bash
   cp .env.example .env
   ```

2. **Edit `.env` with your values:**
   ```bash
   nano .env  # or your preferred editor
   ```

3. **Required changes for production:**
   - `POSTGRES_PASSWORD`: Set a strong password for PostgreSQL
   - `POSTGRES_HOST`: Update if PostgreSQL is not on localhost
   - `SQLITE_PATH`: Ensure path exists and has write permissions

## Configuration Sections

### 1. Decision Engine Thresholds

Controls when the Brain will approve trading decisions.

```env
MIN_DECISION_CONF=75              # Minimum confidence (0-100) for rank/momentum trades
MIN_COPYTRADE_CONFIDENCE=70        # Minimum confidence for copy trades (can be lower)
MIN_FOLLOW_THROUGH_SCORE=55        # Minimum follow-through score (0-100)
```

**Tuning guidance:**
- **Higher confidence** = Fewer but higher-quality trades
- **Lower confidence** = More trades but potentially lower win rate
- **Follow-through score** filters tokens with sustained buyer activity

### 2. Validation Parameters

Pre-trade validation thresholds to prevent excessive fees and slippage.

```env
FEE_MULTIPLIER=2.2                # Actual fees are typically 2.2x base estimate
IMPACT_CAP_MULTIPLIER=0.45        # Max impact as fraction of TP (0.45 = 45%)
MIN_LIQUIDITY_USD=5000.0          # Minimum liquidity required
MAX_SLIPPAGE=0.15                 # Maximum slippage tolerance (0.15 = 15%)
```

**Tuning guidance:**
- `FEE_MULTIPLIER`: Increase if fees are consistently underestimated
- `IMPACT_CAP_MULTIPLIER`: Lower to avoid excessive price impact
- `MIN_LIQUIDITY_USD`: Raise for larger position sizes
- `MAX_SLIPPAGE`: Lower for better execution, higher for more opportunities

### 3. Guardrails

Anti-churn protections to prevent overtrading and loss spirals.

```env
MAX_CONCURRENT_POSITIONS=3        # Total concurrent positions allowed
MAX_ADVISOR_POSITIONS=2           # Max positions from copy trades
RATE_LIMIT_MS=100                 # Min milliseconds between decisions
ADVISOR_RATE_LIMIT_MS=30000       # Min milliseconds between copy trades (30s)
LOSS_BACKOFF_THRESHOLD=3          # Consecutive losses trigger pause
LOSS_BACKOFF_WINDOW_SECS=180      # Time window to track losses (3 min)
LOSS_BACKOFF_PAUSE_SECS=120       # Pause duration after losses (2 min)
WALLET_COOLING_SECS=90            # Min time between copying same wallet
```

**Tuning guidance:**
- **Lower rate limits** = More aggressive trading (higher gas costs)
- **Higher rate limits** = More conservative (better for choppy markets)
- **Loss backoff** prevents "revenge trading" after consecutive losses
- **Wallet cooling** prevents over-copying successful wallets

### 4. Database Connections

#### PostgreSQL (WalletTracker)

Tracks wallet performance history for copy trading decisions.

```env
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
POSTGRES_USER=trader
POSTGRES_PASSWORD=your_secure_password_here
POSTGRES_DB=wallet_tracker
```

**Setup:**
```bash
# Create database and user
createdb wallet_tracker
psql -d wallet_tracker -c "CREATE USER trader WITH PASSWORD 'your_password';"
psql -d wallet_tracker -c "GRANT ALL PRIVILEGES ON DATABASE wallet_tracker TO trader;"
```

#### SQLite (LaunchTracker)

Stores token launch data and market features.

```env
SQLITE_PATH=./data/launch_tracker.db
```

**Setup:**
```bash
# Ensure data directory exists
mkdir -p ./data
# LaunchTracker bot will create the database automatically
```

### 5. UDP Communication

Brain communicates with other bots via UDP messages.

```env
ADVICE_BUS_PORT=45100             # Receives advice from RankBot/AdvisorBot
DECISION_BUS_PORT=45110           # Sends decisions to ExecutionBot
UDP_BIND_ADDRESS=127.0.0.1        # Localhost for same-machine communication
UDP_RECV_BUFFER_SIZE=8192         # UDP receive buffer (bytes)
UDP_SEND_BUFFER_SIZE=8192         # UDP send buffer (bytes)
```

**Network topology:**
```
RankBot/AdvisorBot --[port 45100]--> Brain --[port 45110]--> ExecutionBot
```

### 6. Logging

```env
DECISION_LOG_PATH=./data/brain_decisions.csv
LOG_LEVEL=info                    # Options: error, warn, info, debug, trace
```

**Log levels:**
- `error`: Only critical errors
- `warn`: Warnings + errors
- `info`: General info + warnings + errors (recommended)
- `debug`: Detailed debug info (verbose)
- `trace`: Everything (very verbose, use for debugging only)

### 7. Feature Caches

In-memory caches for fast lookups of token and wallet features.

```env
MINT_CACHE_CAPACITY=10000         # Number of tokens to cache
WALLET_CACHE_CAPACITY=5000        # Number of wallets to cache
CACHE_REFRESH_INTERVAL_SECS=30    # How often to refresh from databases
```

**Tuning guidance:**
- **Higher capacity** = More memory usage, better hit rate
- **Lower refresh interval** = More database load, fresher data
- Typical memory usage: ~5MB for mint cache, ~3MB for wallet cache

### 8. Performance Tuning

```env
WORKER_THREADS=0                  # 0 = auto-detect CPU cores
```

Set `WORKER_THREADS` to a specific number if you want to limit CPU usage.

## Usage in Code

```rust
use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration from .env
    let config = Config::from_env()?;
    
    // Validate configuration
    config.validate()?;
    
    // Access configuration values
    println!("Min confidence: {}", config.decision.min_decision_conf);
    println!("Max positions: {}", config.guardrails.max_concurrent_positions);
    
    // Get PostgreSQL connection string
    let conn_str = config.database.postgres_connection_string();
    
    Ok(())
}
```

## Environment Variable Precedence

1. **System environment variables** (highest priority)
2. **`.env` file** in working directory
3. **Default values** in code (lowest priority)

This allows overriding `.env` values with system environment variables for Docker/Kubernetes deployments.

## Docker/Kubernetes

For containerized deployments, you can either:

**Option 1: Mount `.env` file**
```yaml
volumes:
  - ./brain/.env:/app/.env:ro
```

**Option 2: Use environment variables**
```yaml
environment:
  - MIN_DECISION_CONF=80
  - POSTGRES_HOST=postgres-service
  - POSTGRES_PASSWORD=${POSTGRES_PASSWORD}
```

## Validation

The Brain validates all configuration on startup and will refuse to start if:

- Confidence scores are > 100
- Multipliers are out of valid ranges (0.0-1.0 for fractions)
- Port numbers are 0 or conflicting
- Required fields are missing

Check logs for validation errors:
```bash
cargo run 2>&1 | grep -i error
```

## Testing Configuration

Test your configuration without running the full service:

```bash
# Run config tests
cargo test config::tests

# Validate your .env file
cargo run --bin validate_config  # (if implemented)
```

## Troubleshooting

### "Failed to connect to PostgreSQL"
- Check `POSTGRES_HOST`, `POSTGRES_PORT`, `POSTGRES_USER`, `POSTGRES_PASSWORD`
- Verify PostgreSQL is running: `systemctl status postgresql`
- Test connection: `psql -h localhost -U trader -d wallet_tracker`

### "SQLite path not found"
- Ensure directory exists: `mkdir -p ./data`
- Check write permissions: `ls -la ./data`

### "Port already in use"
- Check if ports are free: `netstat -tulpn | grep 4510`
- Change ports in `.env` if needed

### "Configuration validation failed"
- Check log output for specific validation errors
- Ensure all numeric values are within valid ranges
- Verify port numbers don't conflict

## Security Considerations

1. **Never commit `.env` to version control**
   - `.env` is in `.gitignore` by default
   - Use `.env.example` for documentation

2. **Use strong passwords**
   - `POSTGRES_PASSWORD` should be randomly generated
   - Minimum 16 characters recommended

3. **Restrict file permissions**
   ```bash
   chmod 600 .env  # Owner read/write only
   ```

4. **Use secrets management in production**
   - HashiCorp Vault
   - Kubernetes Secrets
   - AWS Secrets Manager
   - Azure Key Vault

## Performance Monitoring

Monitor these metrics to optimize configuration:

1. **Decision rate**: Adjust `RATE_LIMIT_MS` based on load
2. **Cache hit rate**: Adjust `*_CACHE_CAPACITY` if hit rate < 90%
3. **Validation rejection rate**: Tune thresholds if too many trades rejected
4. **Loss backoff frequency**: Lower thresholds if backing off too often

## Example Configurations

### Conservative (Low Risk)
```env
MIN_DECISION_CONF=85
MIN_COPYTRADE_CONFIDENCE=80
FEE_MULTIPLIER=2.5
IMPACT_CAP_MULTIPLIER=0.35
MAX_CONCURRENT_POSITIONS=2
RATE_LIMIT_MS=500
```

### Aggressive (High Volume)
```env
MIN_DECISION_CONF=65
MIN_COPYTRADE_CONFIDENCE=60
FEE_MULTIPLIER=2.0
IMPACT_CAP_MULTIPLIER=0.55
MAX_CONCURRENT_POSITIONS=5
RATE_LIMIT_MS=50
```

### Balanced (Recommended)
```env
MIN_DECISION_CONF=75
MIN_COPYTRADE_CONFIDENCE=70
FEE_MULTIPLIER=2.2
IMPACT_CAP_MULTIPLIER=0.45
MAX_CONCURRENT_POSITIONS=3
RATE_LIMIT_MS=100
```
