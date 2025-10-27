const DEFAULT_CONFIG = {
    candles: {
        hollowUp: true,
        borderWidth: 1,
        wickWidth: 1,
        minBodyHeight: 1,
    },
    colors: {
        theme: 'light',
        upColor: '#26a69a',
        downColor: '#ef5350',
        upBorderColor: '#26a69a',
        downBorderColor: '#ef5350',
        volumeUpColor: '#26a69a80',
        volumeDownColor: '#ef535080',
    },
    volume: {
        enabled: true,
        heightPercent: 20,
    },
    grid: {
        horizontal: 8,
        vertical: 6,
        style: 'solid',
        opacity: 0.5,
    },
    watermark: {
        enabled: true,
        opacity: 5,
        fontSize: 80,
    },
    crosshair: {
        style: 'dashed',
        width: 1,
        floatingLabels: true,
    },
    priceScale: {
        autoFit: true,
        logarithmic: false,
        padding: 5,
    },
    lastPrice: {
        enabled: true,
        lineStyle: 'dashed',
        labelBg: true,
    },
};
class ChartConfig {
    constructor() {
        this.config = this.loadFromStorage() || { ...DEFAULT_CONFIG };
    }
    loadFromStorage() {
        try {
            const stored = localStorage.getItem('chartConfig');
            if (stored) {
                const parsed = JSON.parse(stored);
                return this.deepMerge(DEFAULT_CONFIG, parsed);
            }
        }
        catch (e) {
            console.warn('Failed to load config from localStorage:', e);
        }
        return null;
    }
    saveToStorage() {
        try {
            localStorage.setItem('chartConfig', JSON.stringify(this.config));
            console.log('✅ Config saved to localStorage');
        }
        catch (e) {
            console.error('Failed to save config:', e);
        }
    }
    deepMerge(target, source) {
        const result = { ...target };
        for (const key in source) {
            if (source[key] && typeof source[key] === 'object' && !Array.isArray(source[key])) {
                result[key] = this.deepMerge(target[key] || {}, source[key]);
            }
            else {
                result[key] = source[key];
            }
        }
        return result;
    }
    get(path) {
        const keys = path.split('.');
        let value = this.config;
        for (const key of keys) {
            value = value[key];
            if (value === undefined)
                return undefined;
        }
        return value;
    }
    set(path, value) {
        const keys = path.split('.');
        let obj = this.config;
        for (let i = 0; i < keys.length - 1; i++) {
            obj = obj[keys[i]];
        }
        obj[keys[keys.length - 1]] = value;
        this.saveToStorage();
    }
    reset() {
        this.config = { ...DEFAULT_CONFIG };
        this.saveToStorage();
    }
    applyTheme(theme) {
        if (theme === 'dark') {
            this.config.colors = {
                ...this.config.colors,
                theme: 'dark',
            };
        }
        else {
            this.config.colors = {
                ...this.config.colors,
                theme: 'light',
            };
        }
        this.saveToStorage();
    }
    getThemeColors() {
        const isDark = this.config.colors.theme === 'dark';
        return {
            bg: isDark ? '#1e1e1e' : '#ffffff',
            grid: isDark ? '#2a2a2a' : '#f0f0f0',
            text: isDark ? '#d4d4d4' : '#333333',
            textLight: isDark ? '#808080' : '#999999',
            crosshair: isDark ? '#758696' : '#758696',
        };
    }
}
export const chartConfig = new ChartConfig();
