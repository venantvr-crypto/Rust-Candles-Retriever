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
    initSettingsPanel();
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

                // Restaurer timeframe sauvegardé ou prendre défaut
                const savedTF = loadTimeframe();
                app.currentTimeframe = app.availableTimeframes.includes(savedTF)
                    ? savedTF
                    : (app.availableTimeframes.includes('1d')
                        ? '1d'
                        : app.availableTimeframes[app.availableTimeframes.length - 1]);

                populateTimeframeDropdown();
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

            // Restaurer timeframe sauvegardé ou prendre défaut
            const savedTF = loadTimeframe();
            app.currentTimeframe = app.availableTimeframes.includes(savedTF)
                ? savedTF
                : (app.availableTimeframes.includes('1d')
                    ? '1d'
                    : app.availableTimeframes[app.availableTimeframes.length - 1]);

            populateTimeframeDropdown();
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
    const dropdown = document.getElementById('timeframeSelector');
    if (dropdown) dropdown.value = app.currentTimeframe;
}

function populateTimeframeDropdown() {
    const dropdown = document.getElementById('timeframeSelector');
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

    // Event listener pour changement manuel (retirer ancien listener si existe)
    dropdown.onchange = async (e) => {
        if (app.isLoading) {
            console.log('⏸️  Ignoring TF change, loading in progress');
            return;
        }

        const newTF = e.target.value;
        console.log(`🔄 Manual TF change: ${app.currentTimeframe} → ${newTF}`);

        if (newTF !== app.currentTimeframe) {
            // Sauvegarder la plage actuelle avant changement
            const savedRange = app.chart && app.chart.state.data.length > 0 ? {
                start: app.chart.state.viewStart,
                end: app.chart.state.viewEnd
            } : null;

            app.currentTimeframe = newTF;
            app.chart.state.currentTimeframe = newTF;
            saveTimeframe();
            await loadCandles(savedRange);
        }
    };
}

function saveTimeframe() {
    try {
        localStorage.setItem('selectedTimeframe', app.currentTimeframe);
    } catch (e) {
        console.warn('Failed to save timeframe to localStorage:', e);
    }
}

function loadTimeframe() {
    try {
        return localStorage.getItem('selectedTimeframe') || '1d';
    } catch (e) {
        console.warn('Failed to load timeframe from localStorage:', e);
        return '1d';
    }
}

function initNavigationButtons() {
    const navLeft = document.getElementById('navLeft');
    const navRight = document.getElementById('navRight');

    if (!navLeft || !navRight) {
        console.error('❌ Navigation buttons not found in DOM');
        return;
    }

    console.log('✅ Navigation buttons initialized');

    navLeft.addEventListener('click', () => {
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

        console.log(`← Panning left by ${panAmount}s`);

        app.chart.state.viewStart -= panAmount;
        app.chart.state.viewEnd -= panAmount;

        // Utiliser la méthode render directement
        if (app.chart.scheduleRender) {
            app.chart.scheduleRender();
        } else {
            app.chart.render();
        }
    });

    navRight.addEventListener('click', () => {
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

        console.log(`→ Panning right by ${panAmount}s`);

        app.chart.state.viewStart += panAmount;
        app.chart.state.viewEnd += panAmount;

        // Utiliser la méthode render directement
        if (app.chart.scheduleRender) {
            app.chart.scheduleRender();
        } else {
            app.chart.render();
        }
    });
}

// Settings Panel Management
function initSettingsPanel() {
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

    bindSetting('theme', 'colors.theme', 'select', null, null, applyThemeChange);

    // Reset button
    reset.addEventListener('click', () => {
        chartConfig.reset();
        loadSettingsToUI();
        refreshChart();
    });
}

function loadSettingsToUI() {
    document.getElementById('hollowUp').checked = chartConfig.get('candles.hollowUp');
    document.getElementById('borderWidth').value = chartConfig.get('candles.borderWidth');
    document.getElementById('borderWidthValue').textContent = chartConfig.get('candles.borderWidth');
    document.getElementById('wickWidth').value = chartConfig.get('candles.wickWidth');
    document.getElementById('wickWidthValue').textContent = chartConfig.get('candles.wickWidth');

    document.getElementById('volumeEnabled').checked = chartConfig.get('volume.enabled');
    document.getElementById('volumeHeight').value = chartConfig.get('volume.heightPercent');
    document.getElementById('volumeHeightValue').textContent = chartConfig.get('volume.heightPercent');

    document.getElementById('watermarkEnabled').checked = chartConfig.get('watermark.enabled');
    document.getElementById('watermarkOpacity').value = chartConfig.get('watermark.opacity');
    document.getElementById('watermarkOpacityValue').textContent = chartConfig.get('watermark.opacity');

    document.getElementById('gridHorizontal').value = chartConfig.get('grid.horizontal');
    document.getElementById('gridHorizontalValue').textContent = chartConfig.get('grid.horizontal');
    document.getElementById('gridOpacity').value = chartConfig.get('grid.opacity') * 100;
    document.getElementById('gridOpacityValue').textContent = Math.round(chartConfig.get('grid.opacity') * 100);

    document.getElementById('floatingLabels').checked = chartConfig.get('crosshair.floatingLabels');
    document.getElementById('crosshairStyle').value = chartConfig.get('crosshair.style');

    document.getElementById('lastPriceEnabled').checked = chartConfig.get('lastPrice.enabled');

    document.getElementById('theme').value = chartConfig.get('colors.theme');
}

function bindSetting(elementId, configPath, type, valueDisplayId = null, transform = null, callback = null) {
    const element = document.getElementById(elementId);

    element.addEventListener(type === 'checkbox' ? 'change' : 'input', (e) => {
        let value;

        if (type === 'checkbox') {
            value = e.target.checked;
        } else if (type === 'range') {
            value = parseFloat(e.target.value);
            if (transform) value = transform(value);
            if (valueDisplayId) {
                const displayValue = transform ? e.target.value : value;
                document.getElementById(valueDisplayId).textContent = displayValue;
            }
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

function applyThemeChange(theme) {
    chartConfig.applyTheme(theme);

    // Update body background
    const isDark = theme === 'dark';
    document.body.style.background = isDark
        ? 'linear-gradient(135deg, #2c3e50 0%, #34495e 100%)'
        : 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)';

    refreshChart();
}

function refreshChart() {
    if (app.chart && app.currentPair) {
        app.chart.updateTheme();
        app.chart.renderBackground();
        app.chart.render();
    }
}
