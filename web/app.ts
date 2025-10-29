import {Candle, PairInfo, AppState} from './types';
import {ChartEngine} from './chart-engine';
import {chartConfig} from './config';

/**
 * Application de trading avec ChartEngine
 */

const API_BASE = '/api';

// État global
const app: AppState = {
    chart: null,
    currentPair: null,
    currentTimeframe: '1d',
    isLoading: false,
    availableTimeframes: []
};

// Initialisation
document.addEventListener('DOMContentLoaded', async () => {
    console.log('🚀 Initializing Chart Engine v2...');
    initSettingsPanel();
    await initChart();
    await loadPairs();
});

async function initChart(): Promise<void> {
    const container = document.getElementById('chart')!;

    app.chart = await ChartEngine.create(container, {
        onLoadData: async (symbol, timeframe, start, end) => {
            return await fetchCandles(symbol, timeframe, start, end);
        },

        onTimeframeChange: async (newTimeframe, savedRange) => {
            if (app.isLoading) {
                console.log('⚠️ Already loading, ignoring TF change');
                return;
            }

            app.currentTimeframe = newTimeframe;
            updateTimeframeDisplay();
            saveTimeframe();
            await loadCandles(savedRange);
        },

        onError: (error) => {
            console.error('Chart error:', error);
            updateStatus(`Error: ${error.message}`, true);
        }
    });

    // Initialize navigation buttons
    initNavigationButtons();

    console.log('✅ Chart engine initialized');
}

async function loadPairs(): Promise<void> {
    try {
        const response = await fetch(`${API_BASE}/pairs`);
        const pairs: PairInfo[] = await response.json();

        const selector = document.getElementById('pairSelector') as HTMLSelectElement;
        selector.innerHTML = '<option value="">Select a pair...</option>';

        // Store pairs data for later use
        const pairsData: Record<string, string[]> = {};
        pairs.forEach(pair => {
            pairsData[pair.symbol] = pair.timeframes;
            const option = document.createElement('option');
            option.value = pair.symbol;
            option.textContent = `${pair.symbol} (${pair.timeframes.join(', ')})`;
            selector.appendChild(option);
        });

        // Event listener
        selector.addEventListener('change', async (e: any) => {
            app.currentPair = e.target.value;
            if (app.currentPair) {
                // Sauvegarder dans localStorage
                localStorage.setItem('selectedPair', app.currentPair);

                // Récupérer les timeframes disponibles pour cette paire
                app.availableTimeframes = pairsData[app.currentPair] || [];
                app.chart.setTimeframes(app.availableTimeframes);

                // Restaurer timeframe sauvegardé ou prendre défaut
                const savedTF = loadTimeframe();
                app.currentTimeframe = app.availableTimeframes.includes(savedTF)
                    ? savedTF
                    : (app.availableTimeframes.includes('1d')
                        ? '1d'
                        : app.availableTimeframes[app.availableTimeframes.length - 1]);

                // Réinitialiser la vue pour charger les derniers points
                app.chart.state.viewStart = 0;
                app.chart.state.viewEnd = 0;

                populateTimeframeDropdown();
                updateTimeframeDisplay();
                await loadCandles();
            }
        });

        // Charger la paire sauvegardée depuis localStorage
        const savedPair = localStorage.getItem('selectedPair');
        let initialPair = savedPair && pairsData[savedPair] ? savedPair : (pairs.length > 0 ? pairs[0].symbol : null);

        // Auto-select
        if (initialPair) {
            selector.value = initialPair;
            app.currentPair = initialPair;
            app.availableTimeframes = pairsData[app.currentPair] || [];
            app.chart.setTimeframes(app.availableTimeframes);

            // Restaurer timeframe sauvegardé ou prendre défaut
            const savedTF = loadTimeframe();
            app.currentTimeframe = app.availableTimeframes.includes(savedTF)
                ? savedTF
                : (app.availableTimeframes.includes('1d')
                    ? '1d'
                    : app.availableTimeframes[app.availableTimeframes.length - 1]);

            populateTimeframeDropdown();
            await loadCandles();
            console.log(`✅ Loaded ${pairs.length} pairs (selected: ${initialPair})`);
        }
    } catch (error) {
        console.error('Error loading pairs:', error);
        updateStatus('Error loading pairs', true);
    }
}

async function loadCandles(savedRange: any = null): Promise<void> {
    if (!app.currentPair) return;

    if (app.isLoading) {
        console.log('⚠️ Already loading, skipping...');
        return;
    }

    app.isLoading = true;
    showLoading(true);
    updateStatus('Loading...');

    try {
        await app.chart.loadData(app.currentPair, app.currentTimeframe, savedRange);
        updateStatus('Ready');
    } catch (error) {
        console.error('Error loading candles:', error);
        updateStatus(`Error: ${error.message}`, true);
    } finally {
        app.isLoading = false;
        showLoading(false);
    }
}

async function fetchCandles(symbol: string, timeframe: string, start: number | null = null, end: number | null = null): Promise<Candle[]> {
    // Construire l'URL avec les paramètres de plage si fournis
    let url = `${API_BASE}/candles?symbol=${symbol}&timeframe=${timeframe}&limit=5000`;

    if (start !== null) {
        url += `&start=${start}`;
    }

    if (end !== null) {
        url += `&end=${end}`;
    }

    console.log(`📡 Fetching ${symbol} ${timeframe} (${start ? new Date(start * 1000).toISOString().substring(0, 16) : 'auto'} → ${end ? new Date(end * 1000).toISOString().substring(0, 16) : 'auto'})...`);

    const response = await fetch(url);

    if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }

    const candles = await response.json();

    if (!Array.isArray(candles) || candles.length === 0) {
        throw new Error('No data received');
    }

    updateCandleCount(candles.length);
    return candles;
}

// UI Helpers
function showLoading(show: boolean): void {
    const el = document.getElementById('loading');
    if (el) el.style.display = show ? 'flex' : 'none';
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
    const dropdown = document.getElementById('timeframeSelector') as HTMLSelectElement | null;
    if (dropdown) dropdown.value = app.currentTimeframe;
}

function populateTimeframeDropdown(): void {
    const dropdown = document.getElementById('timeframeSelector') as HTMLSelectElement | null;
    if (!dropdown) {
        console.error('❌ timeframeSelector not found in DOM');
        return;
    }

    console.log(`📋 Populating dropdown with timeframes: ${app.availableTimeframes.join(', ')}`);

    dropdown.innerHTML = '';
    app.availableTimeframes.forEach(tf => {
        const option = document.createElement('option');
        option.value = tf;
        option.textContent = tf.toUpperCase();
        dropdown.appendChild(option);
    });

    dropdown.value = app.currentTimeframe;
    console.log(`✅ Dropdown populated, current TF: ${app.currentTimeframe}`);

    // Event listener pour changement manuel
    dropdown.onchange = async (e: Event) => {
        if (app.isLoading) {
            console.log('⏸️  Ignoring TF change, loading in progress');
            return;
        }

        const newTF = (e.target as HTMLSelectElement).value;
        console.log(`🔄 Manual TF change: ${app.currentTimeframe} → ${newTF}`);

        if (newTF !== app.currentTimeframe) {
            app.currentTimeframe = newTF;
            if (app.chart) {
                app.chart.state.currentTimeframe = newTF;
                // Réinitialiser la vue pour charger les derniers points
                app.chart.state.viewStart = 0;
                app.chart.state.viewEnd = 0;
            }
            saveTimeframe();
            // Passer null pour charger les derniers points
            await loadCandles(null);
        }
    };
}

function saveTimeframe(): void {
    try {
        localStorage.setItem('selectedTimeframe', app.currentTimeframe);
    } catch (e) {
        console.warn('Failed to save timeframe to localStorage:', e);
    }
}

function loadTimeframe(): string {
    try {
        return localStorage.getItem('selectedTimeframe') || '1d';
    } catch (e) {
        console.warn('Failed to load timeframe from localStorage:', e);
        return '1d';
    }
}

function initNavigationButtons(): void {
    const navLeft = document.getElementById('navLeft') as HTMLButtonElement | null;
    const navRight = document.getElementById('navRight') as HTMLButtonElement | null;

    if (!navLeft || !navRight) {
        console.error('❌ Navigation buttons not found in DOM');
        return;
    }

    console.log('✅ Navigation buttons initialized');

    navLeft.addEventListener('click', async () => {
        console.log('◀ Left navigation clicked');

        if (app.isLoading) {
            console.log('⏸️  Skipping nav: loading');
            return;
        }

        if (!app.chart) {
            console.log('⏸️  Skipping nav: no chart');
            return;
        }

        if (app.chart.state.data.length === 0) {
            console.log('⏸️  Skipping nav: no data');
            return;
        }

        const viewWidth = app.chart.state.viewEnd - app.chart.state.viewStart;
        const panAmount = viewWidth * 0.3; // Pan 30% de la vue

        const newStart = app.chart.state.viewStart - panAmount;
        const newEnd = app.chart.state.viewEnd - panAmount;

        console.log(`← Panning left by ${panAmount}s`);

        // Vérifier si on sort de la plage de données disponibles
        const earliestData = app.chart.state.data[0]?.time || 0;
        const margin = viewWidth * 0.5; // Marge de 50% pour pré-charger

        if (newStart < earliestData + margin) {
            console.log('📥 Loading more historical data...');
            // Charger plus de données historiques
            await loadCandles({start: Math.floor(newStart - viewWidth), end: Math.ceil(newEnd)});
        } else {
            // Assez de données en cache, juste pan
            app.chart.state.viewStart = newStart;
            app.chart.state.viewEnd = newEnd;
            app.chart.render();
        }
    });

    navRight.addEventListener('click', async () => {
        console.log('▶ Right navigation clicked');

        if (app.isLoading) {
            console.log('⏸️  Skipping nav: loading');
            return;
        }

        if (!app.chart) {
            console.log('⏸️  Skipping nav: no chart');
            return;
        }

        if (app.chart.state.data.length === 0) {
            console.log('⏸️  Skipping nav: no data');
            return;
        }

        const viewWidth = app.chart.state.viewEnd - app.chart.state.viewStart;
        const panAmount = viewWidth * 0.3; // Pan 30% de la vue

        const newStart = app.chart.state.viewStart + panAmount;
        const newEnd = app.chart.state.viewEnd + panAmount;

        console.log(`→ Panning right by ${panAmount}s`);

        // Vérifier si on sort de la plage de données disponibles
        const latestData = app.chart.state.data[app.chart.state.data.length - 1]?.time || 0;
        const margin = viewWidth * 0.5; // Marge de 50% pour pré-charger

        if (newEnd > latestData - margin) {
            console.log('📥 Loading more recent data...');
            // Charger plus de données récentes
            await loadCandles({start: Math.floor(newStart), end: Math.ceil(newEnd + viewWidth)});
        } else {
            // Assez de données en cache, juste pan
            app.chart.state.viewStart = newStart;
            app.chart.state.viewEnd = newEnd;
            app.chart.render();
        }
    });
}

// Settings Panel Management
function initSettingsPanel(): void {
    const panel = document.getElementById('settingsPanel');
    const toggle = document.getElementById('settingsToggle');
    const close = document.getElementById('closeSettings');
    const reset = document.getElementById('resetConfig');

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
        refreshChart();
    });
}

function loadSettingsToUI(): void {
    (document.getElementById('hollowUp') as HTMLInputElement).checked = chartConfig.get('candles.hollowUp');
    (document.getElementById('borderWidth') as HTMLInputElement).value = chartConfig.get('candles.borderWidth');
    document.getElementById('borderWidthValue').textContent = chartConfig.get('candles.borderWidth');
    (document.getElementById('wickWidth') as HTMLInputElement).value = chartConfig.get('candles.wickWidth');
    document.getElementById('wickWidthValue').textContent = chartConfig.get('candles.wickWidth');

    (document.getElementById('volumeEnabled') as HTMLInputElement).checked = chartConfig.get('volume.enabled');
    (document.getElementById('volumeHeight') as HTMLInputElement).value = chartConfig.get('volume.heightPercent');
    document.getElementById('volumeHeightValue').textContent = chartConfig.get('volume.heightPercent');

    (document.getElementById('watermarkEnabled') as HTMLInputElement).checked = chartConfig.get('watermark.enabled');
    (document.getElementById('watermarkOpacity') as HTMLInputElement).value = chartConfig.get('watermark.opacity');
    document.getElementById('watermarkOpacityValue').textContent = chartConfig.get('watermark.opacity');

    (document.getElementById('gridHorizontal') as HTMLInputElement).value = chartConfig.get('grid.horizontal');
    document.getElementById('gridHorizontalValue').textContent = chartConfig.get('grid.horizontal');
    (document.getElementById('gridOpacity') as HTMLInputElement).value = String(chartConfig.get('grid.opacity') * 100);
    document.getElementById('gridOpacityValue')!.textContent = String(Math.round(chartConfig.get('grid.opacity') * 100));

    (document.getElementById('floatingLabels') as HTMLInputElement).checked = chartConfig.get('crosshair.floatingLabels');
    (document.getElementById('crosshairStyle') as HTMLInputElement).value = chartConfig.get('crosshair.style');

    (document.getElementById('lastPriceEnabled') as HTMLInputElement).checked = chartConfig.get('lastPrice.enabled');

    (document.getElementById('indicatorsEnabled') as HTMLInputElement).checked = chartConfig.get('indicators.enabled');
    (document.getElementById('indicatorsHeight') as HTMLInputElement).value = chartConfig.get('indicators.heightPercent');
    document.getElementById('indicatorsHeightValue').textContent = chartConfig.get('indicators.heightPercent');
    (document.getElementById('rsiOverlay') as HTMLInputElement).checked = chartConfig.get('indicators.rsi.overlay');

    (document.getElementById('theme') as HTMLInputElement).value = chartConfig.get('colors.theme');
}

function bindSetting(elementId: string, configPath: string, type: string, valueDisplayId: string | null = null, transform: ((v: number) => number) | null = null, callback: ((v: any) => void) | null = null): void {
    const element = document.getElementById(elementId)!;

    element.addEventListener(type === 'checkbox' ? 'change' : 'input', (e: any) => {
        let value: any;

        if (type === 'checkbox') {
            value = e.target.checked;
        } else if (type === 'range') {
            value = parseFloat(e.target.value);
            if (valueDisplayId) {
                document.getElementById(valueDisplayId).textContent = e.target.value;
            }
            if (transform) value = transform(value);
        } else {
            value = e.target.value;
        }

        chartConfig.set(configPath, value);

        if (callback) {
            callback(value);
        } else {
            refreshChart();
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

    refreshChart();
}

async function refreshChart(): Promise<void> {
    if (app.chart && app.currentPair) {
        app.chart.updateTheme();
        await app.chart.loadIndicatorData();
        app.chart.renderBackground();
        app.chart.render();
    }
}
