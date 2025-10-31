import {Candle} from './types';

interface CacheKey {
    symbol: string;
    timeframe: string;
    start: number;
    end: number;
}

interface CacheEntry {
    data: Candle[];
    timestamp: number;
    accessCount: number;
}

/**
 * DataManager - Intelligent Data Layer with LRU Cache + Request Deduplication
 */
export class DataManager {
    private cache: Map<string, CacheEntry> = new Map();
    private inflightRequests: Map<string, Promise<Candle[]>> = new Map();
    private readonly maxCacheSize: number = 100;
    private readonly cacheTTL: number = 5 * 60 * 1000; // 5 minutes
    private readonly apiBase: string = '/api';

    // --- PUBLIC API ---

    async fetch(
        symbol: string,
        timeframe: string,
        start: number | null,
        end: number | null
    ): Promise<Candle[]> {
        const key = this.makeKey(symbol, timeframe, start, end);

        // 1. Check cache
        const cached = this.cache.get(key);
        if (cached && Date.now() - cached.timestamp < this.cacheTTL) {
            console.log(`[DataManager] Cache HIT: ${key}`);
            cached.accessCount++;
            return cached.data;
        }

        // 2. Deduplicate identical requests
        if (this.inflightRequests.has(key)) {
            console.log(`[DataManager] Deduplicating request: ${key}`);
            return await this.inflightRequests.get(key)!;
        }

        // 3. Fetch from API
        console.log(`[DataManager] Cache MISS: ${key}`);
        const promise = this.fetchFromAPI(symbol, timeframe, start, end);
        this.inflightRequests.set(key, promise);

        try {
            const data = await promise;

            // Store in cache
            this.cache.set(key, {
                data,
                timestamp: Date.now(),
                accessCount: 1,
            });

            // Evict if cache too large
            this.evictLRU();

            return data;
        } finally {
            this.inflightRequests.delete(key);
        }
    }

    /**
     * Proactive prefetching of adjacent data
     */
    async prefetchAdjacent(
        symbol: string,
        timeframe: string,
        currentRange: { start: number; end: number }
    ): Promise<void> {
        const rangeWidth = currentRange.end - currentRange.start;
        const now = Math.floor(Date.now() / 1000);

        // Prefetch previous chunk (silently fail if no data)
        const prevPromise = this.fetch(
            symbol,
            timeframe,
            Math.floor(currentRange.start - rangeWidth),
            Math.floor(currentRange.start)
        ).catch(err => {
            console.log(`[DataManager] Prefetch previous chunk failed (OK, might be no data): ${err.message}`);
            return [];
        });

        // Prefetch next chunk SEULEMENT si pas dans le futur (silently fail if no data)
        const nextStart = Math.ceil(currentRange.end);
        const nextEnd = Math.ceil(currentRange.end + rangeWidth);

        let nextPromise: Promise<any[]>;
        if (nextStart > now) {
            // Ne pas précharger le futur
            console.log(`[DataManager] Skipping future prefetch for ${symbol} ${timeframe}`);
            nextPromise = Promise.resolve([]);
        } else {
            nextPromise = this.fetch(
                symbol,
                timeframe,
                nextStart,
                Math.min(nextEnd, now)  // Cap au présent
            ).catch(err => {
                console.log(`[DataManager] Prefetch next chunk failed (OK, might be no data): ${err.message}`);
                return [];
            });
        }

        await Promise.all([prevPromise, nextPromise]);
        console.log(`[DataManager] Prefetch completed for ${symbol} ${timeframe}`);
    }

    /**
     * Clear cache for a specific pair (used when switching pairs)
     */
    clearPair(symbol: string): void {
        const keysToDelete: string[] = [];
        for (const key of this.cache.keys()) {
            if (key.startsWith(`${symbol}:`)) {
                keysToDelete.push(key);
            }
        }
        keysToDelete.forEach(key => this.cache.delete(key));
        console.log(`[DataManager] Cleared cache for ${symbol} (${keysToDelete.length} entries)`);
    }

    /**
     * Invalidate cache for a specific symbol+timeframe (after data changes)
     */
    invalidate(symbol: string, timeframe: string): void {
        const keysToDelete: string[] = [];
        for (const key of this.cache.keys()) {
            if (key.startsWith(`${symbol}:${timeframe}:`)) {
                keysToDelete.push(key);
            }
        }
        keysToDelete.forEach(key => this.cache.delete(key));
        console.log(`[DataManager] Invalidated cache for ${symbol}/${timeframe} (${keysToDelete.length} entries)`);
    }

    /**
     * Get cache statistics
     */
    getStats() {
        const entries = Array.from(this.cache.values());
        const totalSize = entries.reduce((sum, e) => sum + e.data.length, 0);
        const avgAccessCount = entries.reduce((sum, e) => sum + e.accessCount, 0) / entries.length;

        return {
            cacheSize: this.cache.size,
            inflightRequests: this.inflightRequests.size,
            totalCandles: totalSize,
            avgAccessCount: avgAccessCount || 0,
        };
    }

    // --- PRIVATE METHODS ---

    private async fetchFromAPI(
        symbol: string,
        timeframe: string,
        start: number | null,
        end: number | null
    ): Promise<Candle[]> {
        let url = `${this.apiBase}/candles?symbol=${symbol}&timeframe=${timeframe}&limit=5000`;

        if (start !== null) {
            url += `&start=${start}`;
        }

        if (end !== null) {
            url += `&end=${end}`;
        }

        console.log(`[DataManager] Fetching ${symbol} ${timeframe} (${start ? new Date(start * 1000).toISOString().substring(0, 16) : 'auto'} → ${end ? new Date(end * 1000).toISOString().substring(0, 16) : 'auto'})`);

        const response = await fetch(url);

        if (!response.ok) {
            throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }

        const candles = await response.json();

        if (!Array.isArray(candles)) {
            throw new Error('Invalid data format received');
        }

        if (candles.length === 0) {
            // Pas une erreur - peut être légitime (pas de données dans cette range)
            console.log(`[DataManager] No candles in range for ${symbol} ${timeframe}`);
            return [];
        }

        console.log(`[DataManager] Fetched ${candles.length} candles`);
        return candles;
    }

    private makeKey(symbol: string, timeframe: string, start: number | null, end: number | null): string {
        return `${symbol}:${timeframe}:${start || 'null'}:${end || 'null'}`;
    }

    /**
     * Evict least recently used entries when cache is full
     */
    private evictLRU(): void {
        if (this.cache.size <= this.maxCacheSize) return;

        // Find entry with lowest access count
        let minAccessCount = Infinity;
        let oldestKey: string | null = null;
        let oldestTimestamp = Infinity;

        for (const [key, entry] of this.cache.entries()) {
            if (entry.accessCount < minAccessCount || (entry.accessCount === minAccessCount && entry.timestamp < oldestTimestamp)) {
                minAccessCount = entry.accessCount;
                oldestKey = key;
                oldestTimestamp = entry.timestamp;
            }
        }

        if (oldestKey) {
            this.cache.delete(oldestKey);
            console.log(`[DataManager] Evicted LRU entry: ${oldestKey} (access count: ${minAccessCount})`);
        }
    }
}

// Singleton instance
export const dataManager = new DataManager();
