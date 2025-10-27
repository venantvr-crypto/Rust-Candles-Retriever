/**
 * Configuration du chart - Paramètres visuels et comportement
 */

const DEFAULT_CONFIG = {
    // Style des bougies
    candles: {
        hollowUp: true,              // Bougies haussières creuses
        borderWidth: 1,              // Épaisseur bordure (0 = pas de bordure)
        wickWidth: 1,                // Épaisseur mèches (1-3px)
        minBodyHeight: 1,            // Hauteur minimum corps (doji)
    },

    // Couleurs
    colors: {
        theme: 'light',              // 'light' ou 'dark'
        upColor: '#26a69a',
        downColor: '#ef5350',
        upBorderColor: '#26a69a',
        downBorderColor: '#ef5350',
        volumeUpColor: '#26a69a80',  // Avec alpha
        volumeDownColor: '#ef535080',
    },

    // Volume
    volume: {
        enabled: true,
        heightPercent: 20,           // % de hauteur du chart
    },

    // Grille
    grid: {
        horizontal: 8,               // Nombre de lignes
        vertical: 6,
        style: 'solid',              // 'solid', 'dashed', 'dotted'
        opacity: 0.5,
    },

    // Watermark
    watermark: {
        enabled: true,
        opacity: 5,                  // En % (0-30)
        fontSize: 80,
    },

    // Crosshair
    crosshair: {
        style: 'dashed',             // 'solid', 'dashed', 'dotted'
        width: 1,
        floatingLabels: true,        // Labels flottants sur axes
    },

    // Échelle de prix
    priceScale: {
        autoFit: true,               // Auto-ajustement vertical
        logarithmic: false,          // Échelle log
        padding: 5,                  // % padding haut/bas
    },

    // Dernier prix
    lastPrice: {
        enabled: true,
        lineStyle: 'dashed',
        labelBg: true,
    },
};

class ChartConfig {
    constructor() {
        this.config = this.loadFromStorage() || {...DEFAULT_CONFIG};
    }

    loadFromStorage() {
        try {
            const stored = localStorage.getItem('chartConfig');
            if (stored) {
                const parsed = JSON.parse(stored);
                // Merge avec defaults pour nouveaux paramètres
                return this.deepMerge(DEFAULT_CONFIG, parsed);
            }
        } catch (e) {
            console.warn('Failed to load config from localStorage:', e);
        }
        return null;
    }

    saveToStorage() {
        try {
            localStorage.setItem('chartConfig', JSON.stringify(this.config));
            console.log('✅ Config saved to localStorage');
        } catch (e) {
            console.error('Failed to save config:', e);
        }
    }

    deepMerge(target, source) {
        const result = {...target};
        for (const key in source) {
            if (source[key] && typeof source[key] === 'object' && !Array.isArray(source[key])) {
                result[key] = this.deepMerge(target[key] || {}, source[key]);
            } else {
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
            if (value === undefined) return undefined;
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
        this.config = {...DEFAULT_CONFIG};
        this.saveToStorage();
    }

    applyTheme(theme) {
        if (theme === 'dark') {
            this.config.colors = {
                ...this.config.colors,
                theme: 'dark',
            };
        } else {
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

// Instance globale
const chartConfig = new ChartConfig();
