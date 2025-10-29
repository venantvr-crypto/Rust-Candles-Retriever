# TODO - Architecture Improvements

## Current Architecture Issues

### 3 Sources of Truth

1. **app.ts state** (app.currentPair, app.currentTimeframe, app.isLoading)
2. **ChartEngine state** (chart.state.data, viewStart, viewEnd)
3. **localStorage** (selectedTimeframe)

Synchronization is manual everywhere, leading to fragile state management.

### Specific Problems

1. **Manual Synchronization**
    - When changing timeframe: must update app.currentTimeframe, chart.state.currentTimeframe, localStorage
    - Easy to forget one, causing desync bugs

2. **No Error Boundaries**
    - If loadCandles() fails, app.isLoading may stay true forever
    - No retry logic

3. **Naive Caching**
    - Server cache: 5000 entries with 300s TTL
    - No intelligent prefetching
    - No request deduplication (parallel identical requests hit server multiple times)

4. **Fragile Navigation**
    - Margin-based prefetching works but is reactive
    - No proactive loading of adjacent data
    - No notion of "hot" timeframes (most used should stay cached longer)

5. **No Real-time Streaming to Client**
    - Rust backend has WebSocket to Binance
    - Frontend doesn't receive live updates
    - User must refresh to see new data

---

## Proposed Improvements

### 1. Centralized State Management

**Pattern**: Single source of truth with computed properties

```typescript
interface AppStore {
    // State
    pair: string | null;
    timeframe: string;
    availableTimeframes: string[];
    loadingState: 'idle' | 'loading' | 'error' | 'success';
    error: Error | null;
    dataCache: Map<CacheKey, CandleData>;
    visibleRange: { start: number; end: number };

    // Actions
    setPair(pair: string): Promise<void>;
    setTimeframe(tf: string): Promise<void>;
    panLeft(): Promise<void>;
    panRight(): Promise<void>;
    zoomIn(): void;
    zoomOut(): void;

    // Computed
    get currentData(): Candle[];
    get needsMoreData(): boolean;
    get earliestAvailable(): number;
    get latestAvailable(): number;
}

type CacheKey = `${string}_${string}_${number}_${number}`; // symbol_tf_start_end
```

**Benefits**:

- localStorage sync automatic via store subscription
- ChartEngine becomes pure renderer (no state)
- Single place to manage loading/error states

### 2. Intelligent Data Layer

**Pattern**: LRU Cache + Request Deduplication + Prefetching

```typescript
class DataManager {
    private cache: LRUCache<CacheKey, CandleData>;
    private inflightRequests: Map<CacheKey, Promise<CandleData>>;

    async fetch(symbol: string, tf: string, start: number, end: number): Promise<Candle[]> {
        const key = this.makeKey(symbol, tf, start, end);

        // 1. Check cache
        if (this.cache.has(key)) {
            return this.cache.get(key)!;
        }

        // 2. Deduplicate identical requests
        if (this.inflightRequests.has(key)) {
            return this.inflightRequests.get(key)!;
        }

        // 3. Fetch
        const promise = this.fetchFromAPI(symbol, tf, start, end);
        this.inflightRequests.set(key, promise);

        try {
            const data = await promise;
            this.cache.set(key, data);
            return data;
        } finally {
            this.inflightRequests.delete(key);
        }
    }

    // Proactive prefetching
    prefetchAdjacent(symbol: string, tf: string, currentRange: Range): void {
        const rangeWidth = currentRange.end - currentRange.start;

        // Prefetch previous chunk
        this.fetch(symbol, tf, currentRange.start - rangeWidth, currentRange.start);

        // Prefetch next chunk
        this.fetch(symbol, tf, currentRange.end, currentRange.end + rangeWidth);
    }
}
```

**Benefits**:

- No duplicate requests
- Automatic prefetching
- LRU eviction when memory constrained

### 3. Real-time Streaming

**Pattern**: WebSocket/SSE from Rust backend to frontend

```rust
// In web_server.rs
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.realtime_manager.subscribe_updates();

    while let Ok(update) = rx.recv().await {
        let msg = serde_json::to_string(&update).unwrap();
        if socket.send(Message::Text(msg)).await.is_err() {
            break;
        }
    }
}
```

```typescript
// In app.ts
class RealtimeStream {
    private ws: WebSocket;

    connect(symbol: string, timeframes: string[]): void {
        this.ws = new WebSocket(`ws://localhost:8080/ws?symbol=${symbol}`);

        this.ws.onmessage = (event) => {
            const update: CandleUpdate = JSON.parse(event.data);

            // Update store
            store.updateCandle(update);

            // Trigger render if visible
            if (this.isVisible(update)) {
                chart.render();
            }
        };
    }

    private isVisible(update: CandleUpdate): boolean {
        const { start, end } = store.visibleRange;
        return update.candle.time >= start && update.candle.time <= end;
    }
}
```

**Benefits**:

- Live updates without refresh
- Reduced API polling
- Better user experience

### 4. Virtual Scrolling

**Pattern**: Only render visible candles

```typescript
class VirtualRenderer {
    render(data: Candle[], visibleRange: Range, pixelWidth: number): void {
        const visibleCandles = this.getVisibleCandles(data, visibleRange);

        // Clear canvas
        this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);

        // Render only visible
        for (const candle of visibleCandles) {
            this.drawCandle(candle);
        }
    }

    private getVisibleCandles(data: Candle[], range: Range): Candle[] {
        // Binary search for start index
        const startIdx = this.binarySearch(data, range.start);

        // Linear scan until end (or use binary search again)
        const result: Candle[] = [];
        for (let i = startIdx; i < data.length && data[i].time <= range.end; i++) {
            result.push(data[i]);
        }

        return result;
    }
}
```

**Benefits**:

- Constant-time rendering regardless of dataset size
- Smoother pan/zoom

### 5. Optimistic UI Updates

**Pattern**: Update UI immediately, rollback on error

```typescript
async function changePair(newPair: string): Promise<void> {
    const oldPair = store.pair;

    // 1. Optimistic update
    store.pair = newPair;
    store.loadingState = 'loading';

    try {
        // 2. Fetch data
        const data = await dataManager.fetch(newPair, store.timeframe, 0, 0);

        // 3. Commit
        store.loadingState = 'success';
        store.dataCache.set(makeKey(newPair, store.timeframe), data);

    } catch (error) {
        // 4. Rollback on error
        store.pair = oldPair;
        store.loadingState = 'error';
        store.error = error;

        // Show toast notification
        toast.error(`Failed to load ${newPair}: ${error.message}`);
    }
}
```

**Benefits**:

- Instant perceived performance
- Graceful error handling

---

## Migration Plan

### Phase 1: Centralized State (1-2 days)

- Create AppStore class
- Migrate all state from app.ts global to store
- Add store subscriptions for localStorage sync
- Remove duplicate state from ChartEngine

### Phase 2: Intelligent Data Layer (2-3 days)

- Implement DataManager with LRU cache
- Add request deduplication
- Implement proactive prefetching
- Add retry logic with exponential backoff

### Phase 3: Real-time Streaming (2-3 days)

- Add WebSocket endpoint in web_server.rs
- Connect frontend to WebSocket
- Handle reconnection logic
- Add "Live" indicator in UI

### Phase 4: Virtual Scrolling (1-2 days)

- Implement binary search for visible range
- Optimize render loop
- Add performance metrics (FPS counter)

### Phase 5: Polish & Monitoring (1 day)

- Add error boundaries
- Add loading skeletons
- Add performance monitoring (Sentry/LogRocket)
- Add user analytics (most used pairs/timeframes)

---

## Current Working Features

✅ Auto-backfill on server startup
✅ Database verification for all pairs
✅ Enhanced caching (5000 entries, 300s TTL)
✅ RAF throttling for canvas rendering
✅ Timeframe dropdown with localStorage
✅ Lateral navigation with smart prefetching
✅ Real-time data persistence (closed candles only)
✅ Consistent candle wick colors

---

## Notes

- Current system is **functional but fragile**
- Improvements above would make it **production-ready**
- Estimated total effort: **8-12 days** for complete refactor
- Can be done incrementally without breaking existing functionality
