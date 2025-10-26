/**
 * Application de trading avec ChartEngine
 */

const API_BASE = '/api';

// État global
const app = {
    chart: null,
    currentPair: null,
    currentTimeframe: '1d',
    isLoading: false,
    availableTimeframes: []
};

// Initialisation
document.addEventListener('DOMContentLoaded', async () => {
    console.log('🚀 Initializing Chart Engine v2...');
    initChart();
    await loadPairs();
});

function initChart() {
    const container = document.getElementById('chart');

    app.chart = new ChartEngine(container, {
        onLoadData: async (symbol, timeframe, start = null, end = null) => {
            return await fetchCandles(symbol, timeframe, start, end);
        },

        onTimeframeChange: async (newTimeframe, savedRange) => {
            if (app.isLoading) {
                console.log('⚠️ Already loading, ignoring TF change');
                return;
            }

            app.currentTimeframe = newTimeframe;
            updateTimeframeDisplay();
            await loadCandles(savedRange);
        },

        onError: (error) => {
            console.error('Chart error:', error);
            updateStatus(`Error: ${error.message}`, true);
        }
    });

    console.log('✅ Chart engine initialized');
}

async function loadPairs() {
    try {
        const response = await fetch(`${API_BASE}/pairs`);
        const pairs = await response.json();

        const selector = document.getElementById('pairSelector');
        selector.innerHTML = '<option value="">Select a pair...</option>';

        // Store pairs data for later use
        const pairsData = {};
        pairs.forEach(pair => {
            pairsData[pair.symbol] = pair.timeframes;
            const option = document.createElement('option');
            option.value = pair.symbol;
            option.textContent = `${pair.symbol} (${pair.timeframes.join(', ')})`;
            selector.appendChild(option);
        });

        // Event listener
        selector.addEventListener('change', async (e) => {
            app.currentPair = e.target.value;
            if (app.currentPair) {
                // Récupérer les timeframes disponibles pour cette paire
                app.availableTimeframes = pairsData[app.currentPair] || [];
                app.chart.setTimeframes(app.availableTimeframes);

                // Trouver le meilleur timeframe par défaut (1d si dispo, sinon le dernier)
                app.currentTimeframe = app.availableTimeframes.includes('1d')
                    ? '1d'
                    : app.availableTimeframes[app.availableTimeframes.length - 1];

                updateTimeframeDisplay();
                await loadCandles();
            }
        });

        // Auto-select first
        if (pairs.length > 0) {
            selector.value = pairs[0].symbol;
            app.currentPair = pairs[0].symbol;
            app.availableTimeframes = pairsData[app.currentPair] || [];
            app.chart.setTimeframes(app.availableTimeframes);

            app.currentTimeframe = app.availableTimeframes.includes('1d')
                ? '1d'
                : app.availableTimeframes[app.availableTimeframes.length - 1];

            await loadCandles();
        }

        console.log(`✅ Loaded ${pairs.length} pairs`);
    } catch (error) {
        console.error('Error loading pairs:', error);
        updateStatus('Error loading pairs', true);
    }
}

async function loadCandles(savedRange = null) {
    if (!app.currentPair) return;

    if (app.isLoading) {
        console.log('⚠️ Already loading, skipping...');
        return;
    }

    app.isLoading = true;
    app.chart.state.isLoading = true;
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
        app.chart.state.isLoading = false;
        showLoading(false);
    }
}

async function fetchCandles(symbol, timeframe, start = null, end = null) {
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
function showLoading(show) {
    const el = document.getElementById('loading');
    if (el) el.style.display = show ? 'flex' : 'none';
}

function updateStatus(message, isError = false) {
    const el = document.getElementById('status');
    if (el) {
        el.textContent = message;
        el.style.color = isError ? '#ef5350' : '';
    }
}

function updateCandleCount(count) {
    const el = document.getElementById('candleCount');
    if (el) el.textContent = count.toLocaleString();
}

function updateTimeframeDisplay() {
    const el = document.getElementById('currentTimeframe');
    if (el) el.textContent = app.currentTimeframe;
}
