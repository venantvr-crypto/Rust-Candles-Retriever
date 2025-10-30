import {Candle} from './types';
import {ChartEngine} from './chart-engine';
import {DataManager} from './data-manager';

/**
 * AppStore - Centralized State Management
 * Single source of truth for the entire application state
 */
export class AppStore {
    // --- PUBLIC STATE ---
    public currentPair: string | null = null;
    public currentTimeframe: string = '1d';
    public visibleRange: { start: number; end: number } = {start: 0, end: 0};
    public loadingState: 'idle' | 'loading' | 'success' | 'error' = 'idle';
    public error: Error | null = null;

    // --- BUSINESS RULES ---
    public minBars: number = 50;
    public maxBars: number = 150;

    // --- PRIVATE DEPENDENCIES ---
    private chart: ChartEngine | null = null;
    private dataManager: DataManager;
    private availableTimeframes: string[] = [];
    private observers: Array<() => void> = [];

    constructor(dataManager: DataManager) {
        this.dataManager = dataManager;
    }

    // --- INITIALIZATION ---

    setChart(chart: ChartEngine): void {
        this.chart = chart;
    }

    // --- PUBLIC API ---

    async setPair(pair: string, timeframes: string[]): Promise<void> {
        if (pair === this.currentPair) return;

        const oldPair = this.currentPair;
        this.currentPair = pair;
        this.availableTimeframes = this.sortTimeframes(timeframes);

        console.log(`[Store] Timeframes for ${pair} (sorted): ${this.availableTimeframes.join(', ')}`);

        // Save to localStorage
        localStorage.setItem('selectedPair', pair);

        // Restore preferred TF or use default
        const savedTF = localStorage.getItem('selectedTimeframe');
        this.currentTimeframe =
            (savedTF && this.availableTimeframes.includes(savedTF)) ? savedTF :
                (this.availableTimeframes.includes('1d') ? '1d' : this.availableTimeframes[this.availableTimeframes.length - 1]);

        // Update chart timeframes
        if (this.chart) {
            this.chart.setTimeframes(this.availableTimeframes);
        }

        console.log(`[Store] Changed pair: ${oldPair} → ${pair}, TF: ${this.currentTimeframe}`);

        // Notify observers
        this.notifyObservers();

        // Full reload
        await this.loadDataForCurrentView(true);
    }

    setAvailableTimeframes(timeframes: string[]): void {
        this.availableTimeframes = this.sortTimeframes(timeframes);
    }

    async setTimeframe(newTimeframe: string): Promise<void> {
        if (newTimeframe === this.currentTimeframe) return;

        const oldTF = this.currentTimeframe;
        this.currentTimeframe = newTimeframe;
        localStorage.setItem('selectedTimeframe', newTimeframe);

        console.log(`[Store] Changed timeframe: ${oldTF} → ${newTimeframe}`);

        // Notify observers
        this.notifyObservers();

        // Full reload
        await this.loadDataForCurrentView(true);
    }

    setLoadingState(state: 'idle' | 'loading' | 'success' | 'error', error?: Error): void {
        this.loadingState = state;
        this.error = error || null;
        this.notifyObservers();
    }

    resetVisibleRange(): void {
        this.visibleRange = {start: 0, end: 0};
    }

    async updateZoom(zoomFactor: number, pivotTime: number, pivotRatio: number): Promise<void> {
        if (this.loadingState === 'loading' || !this.currentPair || !this.chart) return;

        console.log(`[Store] updateZoom: factor=${zoomFactor.toFixed(2)}, pivot=${new Date(pivotTime * 1000).toISOString().substring(0, 16)}, ratio=${pivotRatio.toFixed(3)}`);

        // Utiliser le pivotRatio passé par chart-engine (déjà calculé avec les bonnes valeurs)
        // Ne PAS le recalculer ici car visibleRange peut être désynchronisé!
        const oldWidth = this.visibleRange.end - this.visibleRange.start;
        const newWidth = oldWidth * zoomFactor;

        const newViewStart = pivotTime - newWidth * pivotRatio;
        const newViewEnd = pivotTime + newWidth * (1 - pivotRatio);

        this.visibleRange = {start: newViewStart, end: newViewEnd};

        // Check if TF change needed
        const tfSeconds = this.parseTimeframeToSeconds(this.currentTimeframe);
        if (tfSeconds === 0) return;

        const newVisibleBars = newWidth / tfSeconds;

        let didChangeTimeframe = false;

        // Zoom IN (< minBars)
        if (newVisibleBars < this.minBars) {
            const smallerTF = this.getSmallerTimeframe();
            if (smallerTF) {
                console.log(`[Store] 🔍 Zoom IN: ${Math.round(newVisibleBars)} bars < ${this.minBars}. Switch ${this.currentTimeframe} → ${smallerTF}`);
                await this.setTimeframeInternal(smallerTF, pivotTime, pivotRatio);
                didChangeTimeframe = true;
            }
        }
        // Zoom OUT (> maxBars)
        else if (newVisibleBars > this.maxBars) {
            const largerTF = this.getLargerTimeframe();
            if (largerTF) {
                console.log(`[Store] 🔎 Zoom OUT: ${Math.round(newVisibleBars)} bars > ${this.maxBars}. Switch ${this.currentTimeframe} → ${largerTF}`);
                await this.setTimeframeInternal(largerTF, pivotTime, pivotRatio);
                didChangeTimeframe = true;
            }
        }

        // Load data if TF didn't change (need to update chart view)
        if (!didChangeTimeframe) {
            // Update chart view directly without reloading
            this.chart.state.viewStart = this.visibleRange.start;
            this.chart.state.viewEnd = this.visibleRange.end;

            console.log(`📐 Updated chart view: ${new Date(this.chart.state.viewStart * 1000).toISOString().substring(0, 16)} → ${new Date(this.chart.state.viewEnd * 1000).toISOString().substring(0, 16)}`);

            // Check if we need more data
            await this.chart.checkAndReloadData();

            // Render
            this.chart.render();

            // Resynchroniser visibleRange avec le state du chart après render
            this.visibleRange.start = this.chart.state.viewStart;
            this.visibleRange.end = this.chart.state.viewEnd;
        }
    }

    async updatePan(timeShift: number): Promise<void> {
        if (this.loadingState === 'loading') return;

        this.visibleRange = {
            start: this.visibleRange.start + timeShift,
            end: this.visibleRange.end + timeShift,
        };

        await this.loadDataForCurrentView(false);

        if (this.chart) {
            this.chart.render();
        }
    }

    // --- OBSERVERS ---

    subscribe(callback: () => void): () => void {
        this.observers.push(callback);
        return () => {
            this.observers = this.observers.filter(cb => cb !== callback);
        };
    }

    private notifyObservers(): void {
        this.observers.forEach(callback => callback());
    }

    // --- INTERNAL LOGIC ---

    private async setTimeframeInternal(newTimeframe: string, pivotTime?: number, pivotRatio?: number): Promise<void> {
        if (newTimeframe === this.currentTimeframe || !this.currentPair || !this.chart) return;

        const oldTF = this.currentTimeframe;
        this.currentTimeframe = newTimeframe;
        localStorage.setItem('selectedTimeframe', newTimeframe);

        console.log(`[Store] TF change: ${oldTF} → ${newTimeframe}, pivot=${pivotTime ? new Date(pivotTime * 1000).toISOString().substring(0, 16) : 'N/A'}, ratio=${pivotRatio?.toFixed(3)}`);

        this.notifyObservers();

        // Prepare savedRange with pivot info
        const savedRange = {
            start: this.visibleRange.start,
            end: this.visibleRange.end,
            oldTFSeconds: this.parseTimeframeToSeconds(oldTF),
            pivotTime: pivotTime || null,
            pivotRatio: pivotRatio || null,
            isSilentReload: true  // Garder l'affichage actuel pendant le changement de TF
        };

        // Use ChartEngine's loadData which preserves pivot
        await this.chart.loadData(this.currentPair, this.currentTimeframe, savedRange);

        // Update store's visible range from chart (chart may have adjusted it)
        this.visibleRange.start = this.chart.state.viewStart;
        this.visibleRange.end = this.chart.state.viewEnd;
    }

    private async loadDataForCurrentView(isFullReset: boolean = false): Promise<void> {
        if (!this.currentPair || !this.chart) return;

        this.loadingState = 'loading';
        this.notifyObservers();

        let rangeToLoad = this.visibleRange;

        // Full reset: load recent data
        if (isFullReset || rangeToLoad.start === 0) {
            const now = Math.floor(Date.now() / 1000);
            const tfSeconds = this.parseTimeframeToSeconds(this.currentTimeframe);
            const defaultWidth = tfSeconds * 100;
            rangeToLoad = {start: now - defaultWidth, end: now + (tfSeconds * 10)};
        }

        try {
            // Update visible range if reset
            if (isFullReset || this.visibleRange.start === 0) {
                this.visibleRange = rangeToLoad;
            }

            // Prepare savedRange for chart.loadData()
            const savedRange = {
                start: this.visibleRange.start,
                end: this.visibleRange.end,
                oldTFSeconds: null,
                pivotTime: null,
                pivotRatio: null
            };

            // Use ChartEngine's loadData which handles everything (indicators, render, etc.)
            await this.chart.loadData(this.currentPair, this.currentTimeframe, savedRange);

            // Trigger prefetching after load
            if (rangeToLoad.start !== 0 && rangeToLoad.end !== 0) {
                this.dataManager.prefetchAdjacent(
                    this.currentPair,
                    this.currentTimeframe,
                    {start: rangeToLoad.start, end: rangeToLoad.end}
                ).catch(e => console.warn('Prefetch failed:', e));
            }

            this.loadingState = 'success';

        } catch (e) {
            this.loadingState = 'error';
            this.error = e as Error;
            console.error('[Store] Load error:', e);
        }

        this.notifyObservers();
    }

    // --- HELPERS ---

    private getSmallerTimeframe(): string | null {
        const currentIndex = this.availableTimeframes.indexOf(this.currentTimeframe);
        const result = currentIndex > 0 ? this.availableTimeframes[currentIndex - 1] : null;
        console.log(`[Store] getSmallerTimeframe: current=${this.currentTimeframe} (idx=${currentIndex}), smaller=${result}, all=[${this.availableTimeframes.join(', ')}]`);
        return result;
    }

    private getLargerTimeframe(): string | null {
        const currentIndex = this.availableTimeframes.indexOf(this.currentTimeframe);
        const result = (currentIndex !== -1 && currentIndex < this.availableTimeframes.length - 1) ? this.availableTimeframes[currentIndex + 1] : null;
        console.log(`[Store] getLargerTimeframe: current=${this.currentTimeframe} (idx=${currentIndex}), larger=${result}, all=[${this.availableTimeframes.join(', ')}]`);
        return result;
    }

    private parseTimeframeToSeconds(tf: string): number {
        const match = tf.match(/^(\d+)([mhd])$/);
        if (!match) return 86400;

        const value = parseInt(match[1]);
        const unit = match[2];

        switch (unit) {
            case 'm':
                return value * 60;
            case 'h':
                return value * 3600;
            case 'd':
                return value * 86400;
            default:
                return 86400;
        }
    }

    private sortTimeframes(timeframes: string[]): string[] {
        return [...timeframes].sort((a, b) =>
            this.parseTimeframeToSeconds(a) - this.parseTimeframeToSeconds(b)
        );
    }
}
