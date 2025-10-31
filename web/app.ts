import {Candle, PairInfo} from './types';
import {ChartEngine} from './chart-engine';
import {chartConfig} from './config';
import {AppStore} from './store';
import {dataManager} from './data-manager';

/**
 * Application de trading avec ChartEngine - REFACTORED avec Centralized State
 * Architecture: AppStore (single source of truth) → ChartEngine (pure renderer)
 */

const API_BASE = '/api';

// Centralized store
const store = new AppStore(dataManager);

// Chart reference
let chart: ChartEngine | null = null;

// Initialisation
document.addEventListener('DOMContentLoaded', async () => {
    console.log('🚀 Initializing Chart Engine v2 (Refactored Architecture)...');
    initSettingsPanel();
    await initChart();
    await loadPairs();

    // Subscribe to store changes for UI updates
    store.subscribe(() => {
        updateTimeframeDisplay();
        updateLoadingIndicator();
        // Update candle count: nombre de bougies VISIBLES (pas en mémoire)
        updateCandleCount(store.getVisibleBarsCount());
    });
});

async function initChart(): Promise<void> {
    const container = document.getElementById('chart')!;

    chart = await ChartEngine.create(container, {
        onLoadData: async (symbol, timeframe, start, end) => {
            // DataManager handles all fetching through store
            return await dataManager.fetch(symbol, timeframe, start, end);
        },

        onTimeframeChange: async (newTimeframe, savedRange) => {
            // Delegate to store
            await store.setTimeframe(newTimeframe);
        },

        onError: (error) => {
            console.error('Chart error:', error);
            updateStatus(`Error: ${error.message}`, true);
        },

        onInvalidateCache: (symbol, timeframe) => {
            // Invalidate cache when data changes (after fetch)
            dataManager.invalidate(symbol, timeframe);
        }
    });

    // Register chart with store (bidirectional)
    store.setChart(chart);
    chart.setStore(store);

    console.log('✅ Chart engine initialized with store connection');
}

async function loadPairs(): Promise<void> {
    try {
        const response = await fetch(`${API_BASE}/pairs`);
        const pairs: PairInfo[] = await response.json();

        const selector = document.getElementById('pairSelector') as HTMLSelectElement;
        selector.innerHTML = '<option value="">Select a pair...</option>';

        // Store pairs data
        const pairsData: Record<string, string[]> = {};
        pairs.forEach(pair => {
            pairsData[pair.symbol] = pair.timeframes;
            const option = document.createElement('option');
            option.value = pair.symbol;
            option.textContent = `${pair.symbol} (${pair.timeframes.join(', ')})`;
            selector.appendChild(option);
        });

        // Event listener for pair change
        selector.addEventListener('change', async (e: any) => {
            const newPair = e.target.value;
            if (newPair) {
                const timeframes = pairsData[newPair] || [];
                console.log(`[App] Timeframes from API for ${newPair}: ${timeframes.join(', ')}`);
                await store.setPair(newPair, timeframes);
            }
        });

        // Load saved pair or first pair
        const savedPair = localStorage.getItem('selectedPair');
        let initialPair = savedPair && pairsData[savedPair] ? savedPair : (pairs.length > 0 ? pairs[0].symbol : null);

        if (initialPair) {
            selector.value = initialPair;
            const timeframes = pairsData[initialPair] || [];
            console.log(`[App] Initial timeframes from API for ${initialPair}: ${timeframes.join(', ')}`);
            await store.setPair(initialPair, timeframes);
            console.log(`✅ Loaded ${pairs.length} pairs (selected: ${initialPair})`);
        }
    } catch (error) {
        console.error('Error loading pairs:', error);
        updateStatus('Error loading pairs', true);
    }
}

// --- UI HELPERS ---

function updateLoadingIndicator(): void {
    const loadingEl = document.getElementById('loading');
    if (loadingEl) {
        loadingEl.style.display = store.loadingState === 'loading' ? 'flex' : 'none';
    }

    const statusEl = document.getElementById('status');
    if (statusEl) {
        switch (store.loadingState) {
            case 'loading':
                statusEl.textContent = 'Loading...';
                statusEl.style.color = '';
                break;
            case 'success':
                statusEl.textContent = 'Ready';
                statusEl.style.color = '';
                break;
            case 'error':
                statusEl.textContent = store.error ? `Error: ${store.error.message}` : 'Error';
                statusEl.style.color = '#ef5350';
                break;
            default:
                statusEl.textContent = 'Ready';
                statusEl.style.color = '';
        }
    }
}

function updateStatus(message: string, isError: boolean = false): void {
    const el = document.getElementById('status');
    if (el) {
        el.textContent = message;
        el.style.color = isError ? '#ef5350' : '';
    }
}

function updateCandleCount(count: number): void {
    const el = document.getElementById('candleCount');
    if (el) el.textContent = count.toLocaleString();
}

function updateTimeframeDisplay(): void {
    const badge = document.getElementById('currentTimeframe');
    if (badge) badge.textContent = store.currentTimeframe.toUpperCase();
}

// --- SETTINGS PANEL ---

function initSettingsPanel(): void {
    const panel = document.getElementById('settingsPanel');
    const toggle = document.getElementById('settingsToggle');
    const close = document.getElementById('closeSettings');
    const reset = document.getElementById('resetConfig');

    if (!panel || !toggle || !close || !reset) return;

    // Toggle panel
    toggle.addEventListener('click', () => {
        panel.classList.toggle('open');
        toggle.classList.toggle('hidden');
    });

    close.addEventListener('click', () => {
        panel.classList.remove('open');
        toggle.classList.remove('hidden');
    });

    // Close panel when clicking outside
    document.addEventListener('click', (e: any) => {
        if (panel.classList.contains('open')) {
            const target = e.target;
            if (!panel.contains(target) && target !== toggle) {
                panel.classList.remove('open');
                toggle.classList.remove('hidden');
            }
        }
    });

    // Load current values into UI
    loadSettingsToUI();

    // Bind all controls
    bindSetting('hollowUp', 'candles.hollowUp', 'checkbox');
    bindSetting('borderWidth', 'candles.borderWidth', 'range', 'borderWidthValue');
    bindSetting('wickWidth', 'candles.wickWidth', 'range', 'wickWidthValue');

    bindSetting('volumeEnabled', 'volume.enabled', 'checkbox');
    bindSetting('volumeHeight', 'volume.heightPercent', 'range', 'volumeHeightValue');

    bindSetting('watermarkEnabled', 'watermark.enabled', 'checkbox');
    bindSetting('watermarkOpacity', 'watermark.opacity', 'range', 'watermarkOpacityValue');

    bindSetting('gridHorizontal', 'grid.horizontal', 'range', 'gridHorizontalValue');
    bindSetting('gridOpacity', 'grid.opacity', 'range', 'gridOpacityValue', (v) => v / 100);

    bindSetting('floatingLabels', 'crosshair.floatingLabels', 'checkbox');
    bindSetting('crosshairStyle', 'crosshair.style', 'select');

    bindSetting('lastPriceEnabled', 'lastPrice.enabled', 'checkbox');

    bindSetting('indicatorsEnabled', 'indicators.enabled', 'checkbox');
    bindSetting('indicatorsHeight', 'indicators.heightPercent', 'range', 'indicatorsHeightValue');
    bindSetting('rsiOverlay', 'indicators.rsi.overlay', 'checkbox');

    bindSetting('theme', 'colors.theme', 'select', null, null, applyThemeChange);

    // Reset button
    reset.addEventListener('click', () => {
        chartConfig.reset();
        loadSettingsToUI();
        void refreshChart();
    });
}

function loadSettingsToUI(): void {
    (document.getElementById('hollowUp') as HTMLInputElement).checked = chartConfig.get('candles.hollowUp');
    (document.getElementById('borderWidth') as HTMLInputElement).value = chartConfig.get('candles.borderWidth');
    document.getElementById('borderWidthValue')!.textContent = chartConfig.get('candles.borderWidth');
    (document.getElementById('wickWidth') as HTMLInputElement).value = chartConfig.get('candles.wickWidth');
    document.getElementById('wickWidthValue')!.textContent = chartConfig.get('candles.wickWidth');

    (document.getElementById('volumeEnabled') as HTMLInputElement).checked = chartConfig.get('volume.enabled');
    (document.getElementById('volumeHeight') as HTMLInputElement).value = chartConfig.get('volume.heightPercent');
    document.getElementById('volumeHeightValue')!.textContent = chartConfig.get('volume.heightPercent');

    (document.getElementById('watermarkEnabled') as HTMLInputElement).checked = chartConfig.get('watermark.enabled');
    (document.getElementById('watermarkOpacity') as HTMLInputElement).value = chartConfig.get('watermark.opacity');
    document.getElementById('watermarkOpacityValue')!.textContent = chartConfig.get('watermark.opacity');

    (document.getElementById('gridHorizontal') as HTMLInputElement).value = chartConfig.get('grid.horizontal');
    document.getElementById('gridHorizontalValue')!.textContent = chartConfig.get('grid.horizontal');
    (document.getElementById('gridOpacity') as HTMLInputElement).value = String(chartConfig.get('grid.opacity') * 100);
    document.getElementById('gridOpacityValue')!.textContent = String(Math.round(chartConfig.get('grid.opacity') * 100));

    (document.getElementById('floatingLabels') as HTMLInputElement).checked = chartConfig.get('crosshair.floatingLabels');
    (document.getElementById('crosshairStyle') as HTMLInputElement).value = chartConfig.get('crosshair.style');

    (document.getElementById('lastPriceEnabled') as HTMLInputElement).checked = chartConfig.get('lastPrice.enabled');

    (document.getElementById('indicatorsEnabled') as HTMLInputElement).checked = chartConfig.get('indicators.enabled');
    (document.getElementById('indicatorsHeight') as HTMLInputElement).value = chartConfig.get('indicators.heightPercent');
    document.getElementById('indicatorsHeightValue')!.textContent = chartConfig.get('indicators.heightPercent');
    (document.getElementById('rsiOverlay') as HTMLInputElement).checked = chartConfig.get('indicators.rsi.overlay');

    (document.getElementById('theme') as HTMLInputElement).value = chartConfig.get('colors.theme');
}

function bindSetting(elementId: string, configPath: string, type: string, valueDisplayId: string | null = null, transform: ((v: number) => number) | null = null, callback: ((v: any) => void) | null = null): void {
    const element = document.getElementById(elementId);
    if (!element) return;

    element.addEventListener(type === 'checkbox' ? 'change' : 'input', (e: any) => {
        let value: any;

        if (type === 'checkbox') {
            value = e.target.checked;
        } else if (type === 'range') {
            value = parseFloat(e.target.value);
            if (valueDisplayId) {
                const displayEl = document.getElementById(valueDisplayId);
                if (displayEl) displayEl.textContent = e.target.value;
            }
            if (transform) value = transform(value);
        } else {
            value = e.target.value;
        }

        chartConfig.set(configPath, value);

        if (callback) {
            callback(value);
        } else {
            void refreshChart();
        }
    });
}

function applyThemeChange(theme: 'light' | 'dark'): void {
    chartConfig.applyTheme(theme);

    // Update body background
    const isDark = theme === 'dark';
    document.body.style.background = isDark
        ? 'linear-gradient(135deg, #2c3e50 0%, #34495e 100%)'
        : 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)';

    void refreshChart();
}

async function refreshChart(): Promise<void> {
    if (chart && store.currentPair) {
        chart.updateTheme();
        await chart.loadIndicatorData();
        chart.renderBackground();
        chart.render();
    }
}
