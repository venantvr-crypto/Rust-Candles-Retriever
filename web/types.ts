/**
 * Types globaux pour l'application de trading
 */

export interface Candle {
    time: number;
    open: number;
    high: number;
    low: number;
    close: number;
    volume: number;
}

export interface ChartState {
    data: Candle[];
    currentTimeframe: string;
    symbol: string | null;
    viewStart: number;
    viewEnd: number;
    minBars: number;
    maxBars: number;
    priceMin: number;
    priceMax: number;
    showCrosshair: boolean;
    mouseX: number;
    mouseY: number;
    crosshairCandle: Candle | null;
    isDragging: boolean;
    dragStartX: number;
    dragStartViewStart: number;
    dragStartViewEnd: number;
    isLoading: boolean;
    isProcessingZoom: boolean;
    lastZoomTime: number;
}

export interface ThemeColors {
    bg: string;
    grid: string;
    text: string;
    textLight: string;
    crosshair: string;
    upColor: string;
    downColor: string;
    upBorderColor: string;
    downBorderColor: string;
    tooltipBg: string;
    tooltipBorder: string;
}

export interface ChartLayout {
    marginLeft: number;
    marginRight: number;
    marginTop: number;
    marginBottom: number;
}

export interface ChartCallbacks {
    onLoadData: (symbol: string, timeframe: string, start?: number, end?: number) => Promise<Candle[]>;
    onTimeframeChange: (newTimeframe: string, savedRange: SavedRange) => Promise<void>;
    onError: (error: Error) => void;
    onInvalidateCache: (symbol: string, timeframe: string) => void;
}

export interface ChartOptions {
    onLoadData?: (symbol: string, timeframe: string, start?: number, end?: number) => Promise<Candle[]>;
    onTimeframeChange?: (newTimeframe: string, savedRange: SavedRange) => Promise<void>;
    onError?: (error: Error) => void;
    onInvalidateCache?: (symbol: string, timeframe: string) => void;
}

export interface SavedRange {
    start: number;
    end: number;
    oldTFSeconds?: number;
    pivotTime?: number;
    pivotRatio?: number;
}

export interface IndicatorData {
    time: number;
    value: number;
}

export interface IndicatorConfig {
    type: 'rsi';
    period: number;
    overlay: boolean;
    heatIntensity: number;
}

export interface Config {
    candles: {
        hollowUp: boolean;
        borderWidth: number;
        wickWidth: number;
        minBodyHeight: number;
    };
    colors: {
        theme: 'light' | 'dark';
        upColor: string;
        downColor: string;
        upBorderColor: string;
        downBorderColor: string;
        volumeUpColor: string;
        volumeDownColor: string;
    };
    volume: {
        enabled: boolean;
        heightPercent: number;
    };
    grid: {
        horizontal: number;
        vertical: number;
        style: 'solid' | 'dashed' | 'dotted';
        opacity: number;
    };
    watermark: {
        enabled: boolean;
        opacity: number;
        fontSize: number;
    };
    crosshair: {
        style: 'solid' | 'dashed' | 'dotted';
        width: number;
        floatingLabels: boolean;
    };
    priceScale: {
        autoFit: boolean;
        logarithmic: boolean;
        padding: number;
    };
    lastPrice: {
        enabled: boolean;
        lineStyle: 'solid' | 'dashed' | 'dotted';
        labelBg: boolean;
    };
    indicators: {
        enabled: boolean;
        heightPercent: number;
        rsi: IndicatorConfig;
    };
}

export interface PairInfo {
    symbol: string;
    timeframes: string[];
}

export interface AppState {
    chart: any; // ChartEngine sera typé après conversion
    currentPair: string | null;
    currentTimeframe: string;
    isLoading: boolean;
    availableTimeframes: string[];
}
