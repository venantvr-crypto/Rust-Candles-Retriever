# Rust Candles Retriever - Project Documentation

## Overview

This project is a Rust-based candlestick data retriever for cryptocurrency markets, specifically designed to fetch historical OHLCV (Open, High, Low, Close, Volume) data
from Binance and store it in a SQLite database with automatic gap detection and interpolation.

## Project Genesis

This application was developed collaboratively with Claude Code, addressing several technical challenges and evolving requirements throughout the development process.

## Key Features

### 1. **Historical Data Retrieval**

- Fetches candlestick data from Binance API
- Supports multiple timeframes (5m, 15m, 30m, 1h)
- Retrieves data in batches of 1000 candles
- Works backwards in time from a specified start date
- Automatic resume capability from last stored candle

### 2. **Data Integrity**

- **Gap Detection**: Identifies missing data points in the time series
- **Linear Interpolation**: Automatically fills gaps with interpolated values
- **Idempotent Operations**: INSERT OR IGNORE with UNIQUE constraints prevents duplicates
- **Tracking**: Distinguishes real API data (interpolated=0) from synthetic data (interpolated=1)

### 3. **Multi-Provider Support**

- Database schema includes provider column for future multi-exchange support
- Currently configured for Binance

### 4. **Verification Tools**

- Standalone binary (`verify_data`) to check data spacing
- Detects gaps and overlaps
- Provides detailed statistics and anomaly reports
- Test binary (`test_gap_fill`) to demonstrate interpolation algorithm

## Architecture

### Core Components

```
src/
├── main.rs                # Main retrieval binary with resume mode
├── verify.rs              # Data verification module
└── bin/
    ├── verify_data.rs     # Standalone verification CLI
    ├── test_gap_fill.rs   # Interpolation test program
    └── test_resume_mode.rs # Resume mode test program
```

### Database Schema

```sql
CREATE TABLE candlesticks (
    provider TEXT NOT NULL,
    symbol TEXT NOT NULL,
    timeframe TEXT NOT NULL,
    open_time INTEGER NOT NULL,
    open REAL NOT NULL,
    high REAL NOT NULL,
    low REAL NOT NULL,
    close REAL NOT NULL,
    volume REAL NOT NULL,
    close_time INTEGER NOT NULL,
    quote_asset_volume REAL NOT NULL,
    number_of_trades INTEGER NOT NULL,
    taker_buy_base_asset_volume REAL NOT NULL,
    taker_buy_quote_asset_volume REAL NOT NULL,
    interpolated INTEGER NOT NULL DEFAULT 0,
    UNIQUE(provider, symbol, timeframe, open_time)
)
```

## Key Algorithms

### 1. Backward Time Traversal

The retrieval algorithm works backwards from a start date:

- More efficient for historical data collection
- Uses `end_time_ms` parameter to specify where to start
- Each batch moves further back in time
- Continues until no more data or maximum limit reached

### 2. Linear Interpolation

When gaps are detected, missing candles are filled using linear interpolation:

```
interpolated_value = value_before + (value_after - value_before) × ratio
```

Where `ratio = position / (total_missing_candles + 1)`

This formula is applied to all fields: OHLC prices, volumes, trades, etc.

**Rationale**: Simple, fast, and acceptable for small gaps. Provides continuity for time-series analysis.

### 3. Gap Detection

After each batch insertion, the algorithm:

1. Queries all candles in the current range
2. Calculates expected interval based on timeframe
3. Identifies gaps where `actual_interval > expected_interval`
4. Generates interpolated candles to fill gaps
5. Marks synthetic data with `interpolated=1`

### 4. Smart Resume Mode

The application implements intelligent resume capability that automatically detects and continues from where it left off:

**Algorithm**:

1. Query database for `MAX(open_time)` for the specific (provider, symbol, timeframe)
2. If data exists → **RESUME MODE**: Start from last stored candle timestamp
3. If no data → **FIRST EXECUTION MODE**: Start from current time
4. Continue backwards in time until reaching `start_date` limit or no more data

**Benefits**:

- **Efficiency**: Avoids re-downloading existing data
- **Interruption Recovery**: Automatically resumes after crashes, Ctrl+C, or network failures
- **Bandwidth Optimization**: Respects API rate limits by only fetching new data
- **Clear Feedback**: Visual indicators show which mode is active

**Example Output**:

```
╔════════════════════════════════════════════════════════════
║ MODE REPRISE ACTIVÉ
╠════════════════════════════════════════════════════════════
║ Dernière bougie en base: 2024-10-20 14:35:00
║ Récupération: depuis cette date vers le passé
║ Limite de récupération: 2024-10-15 00:00:00
╚════════════════════════════════════════════════════════════
```

## Technical Decisions

### Why Synchronous Code?

- binance-rs v0.21.0 uses synchronous API
- No need for async overhead for sequential batch processing
- Simpler error handling and control flow

### Why SQLite?

- Embedded database, no separate server needed
- ACID transactions ensure data consistency
- Excellent for time-series data
- Built-in UNIQUE constraint for idempotence

### Why Linear Interpolation?

- Simplest algorithm that provides continuity
- Fast computation
- Acceptable for small gaps (typically a few missing candles)
- Applied uniformly to all data fields

### Why INSERT OR IGNORE?

- Allows safe re-runs without duplicate checks
- Combined with UNIQUE constraint provides automatic deduplication
- Idempotent by design

## Development Journey

### Phase 1: Basic Setup (Messages 1-3)

- Fixed import errors (binance API changes)
- Resolved clap argument conflicts
- Migrated from async to sync code

### Phase 2: Configuration (Messages 4-5)

- Added BTCUSDT to IDE run configuration
- Enhanced schema with provider column

### Phase 3: Verification (Messages 6-8)

- Created verification module and CLI tool
- Confirmed contiguous data retrieval algorithm

### Phase 4: Data Integrity (Messages 9-11)

- Implemented gap detection and linear interpolation
- Added comprehensive Rust documentation
- Added interpolated column to track synthetic data

### Phase 5: Smart Resume Mode (Message 12)

- Created `get_last_candle_time()` function to query last stored candle
- Implemented intelligent resume algorithm
- Added visual indicators for execution modes (PREMIÈRE EXÉCUTION vs MODE REPRISE)
- Optimized bandwidth by avoiding re-download of existing data
- Created test binary (`test_resume_mode`) to demonstrate functionality

## Usage

### Retrieve Data

```bash
cargo run --release -- \
  --symbol BTCUSDT \
  --start-date "2024-01-01 00:00:00" \
  --limit 100000 \
  --db-file candlesticks.db
```

### Verify Data

```bash
cargo run --bin verify_data -- \
  --symbol BTCUSDT \
  --provider binance \
  --timeframes 5m,15m,1h \
  --db-file candlesticks.db
```

### Test Interpolation

```bash
cargo run --bin test_gap_fill
```

### Test Resume Mode

```bash
cargo run --bin test_resume_mode
```

This test demonstrates:

- First execution mode (no existing data)
- Resume mode (with existing data)
- Per-timeframe resume capability

### Query Database

```bash
sqlite3 candlesticks.db "
  SELECT
    datetime(open_time/1000, 'unixepoch') as time,
    open, high, low, close, volume,
    CASE interpolated
      WHEN 0 THEN 'Real'
      WHEN 1 THEN 'Interpolated'
    END as data_type
  FROM candlesticks
  WHERE symbol='BTCUSDT' AND timeframe='5m'
  ORDER BY open_time DESC
  LIMIT 20
"
```

## Rust Language Features Demonstrated

This project showcases numerous Rust concepts:

- **Ownership & Borrowing**: Efficient memory management
- **Pattern Matching**: Exhaustive, type-safe control flow
- **Error Handling**: Result<T> with ? operator
- **Type Safety**: Option<T> instead of null values
- **Derive Macros**: Automatic trait implementations
- **Module System**: Clean code organization
- **CLI Development**: clap with derive macros
- **Database Integration**: rusqlite with prepared statements

See [RUST_COMMENTS.md](RUST_COMMENTS.md) for detailed explanations of 25+ Rust subtleties used throughout the codebase.

## Dependencies

- **binance** v0.21.0 - Binance API client (synchronous)
- **rusqlite** v0.37.0 - SQLite database driver
- **chrono** v0.4 - Date and time handling
- **clap** v4.5 - CLI argument parsing
- **anyhow** v1.0 - Error handling
- **tokio** v1 - (minimal usage, mainly for compatibility)

## Future Enhancements

Potential areas for expansion:

1. **Multi-Exchange Support**: Add other exchanges (Coinbase, Kraken, etc.)
2. **Parallel Downloads**: Fetch multiple timeframes concurrently
3. **Advanced Interpolation**: Cubic splines or other methods for larger gaps
4. **Real-time Updates**: WebSocket integration for live data
5. **Data Export**: CSV, Parquet, or other formats
6. **Visualization**: Generate charts and analytics
7. **Gap Policies**: Configurable strategies (interpolate, mark as missing, skip, etc.)

## Code Quality

- All code includes comprehensive comments
- Rust subtleties are explained inline
- Algorithmic decisions are documented
- Test programs validate core functionality
- No unsafe code used

## License

[Specify license here]

## Contributing

This project was developed with AI assistance. When contributing:

- Maintain the comprehensive commenting style
- Document Rust-specific patterns and reasoning
- Preserve idempotent operation guarantees
- Ensure backward compatibility with existing database schema

## Acknowledgments

Developed with Claude Code, demonstrating collaborative AI-assisted software development with emphasis on:

- Clear architectural decisions
- Comprehensive documentation
- Robust error handling
- Test-driven validation
