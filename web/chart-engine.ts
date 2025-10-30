import {ChartState, ThemeColors, ChartLayout, ChartCallbacks, ChartOptions} from './types';
import {chartConfig} from './config';
import * as PIXI from 'pixi.js';

/**
 * ChartEngine - Moteur de rendu de chandelier haute performance avec WebGL (PixiJS)
 * Architecture: MVC pattern pour séparation des responsabilités
 */

export class ChartEngine {
    container: HTMLElement;
    app: PIXI.Application;
    bgLayer: PIXI.Container;
    mainLayer: PIXI.Container;
    overlayLayer: PIXI.Container;
    overlayCanvas: HTMLCanvasElement;
    overlayCtx: CanvasRenderingContext2D;
    mainCtx: any; // Compatibility stub
    timeframes: string[];
    state: ChartState;
    callbacks: ChartCallbacks;
    theme: ThemeColors;
    layout: ChartLayout;
    rsiData: Map<string, any[]>;
    rsiVisibility: Map<string, boolean>;
    rsiHistoricalData: Map<string, any[]>; // Cache des données historiques pour RSI temps-réel
    legendContainer: HTMLDivElement;
    overlayParams: any;
    realtimeCandles: Map<string, any>; // TF → candle en cours
    realtimeWs: WebSocket | null; // Connexion WebSocket temps réel
    realtimeSubscribed: Set<string>; // Streams déjà souscrits (symbol:tf)
    realtimeUpdating: boolean; // Flag pour éviter appels concurrents

    // Graphics réutilisables pour performance (batching)
    wicksGraphics: PIXI.Graphics;
    bodiesUpFilledGraphics: PIXI.Graphics;
    bodiesUpHollowGraphics: PIXI.Graphics;
    bodiesDownGraphics: PIXI.Graphics;
    bordersGraphics: PIXI.Graphics;
    realtimeMarkersGraphics: PIXI.Graphics;
    volumeGraphics: PIXI.Graphics;
    rsiGraphics: PIXI.Graphics;
    indicatorBgGraphics: PIXI.Graphics;

    // Optimisation rendu avec requestAnimationFrame
    renderScheduled: boolean;
    overlayRenderScheduled: boolean;
    rafId: number | null;

    private constructor(container: HTMLElement, app: PIXI.Application, options: ChartOptions = {}) {
        this.container = container;
        this.app = app;

        // Ajouter le canvas PixiJS au conteneur
        const pixiCanvas = this.app.canvas;
        pixiCanvas.style.position = 'absolute';
        pixiCanvas.style.left = '0';
        pixiCanvas.style.top = '0';
        this.container.appendChild(pixiCanvas);

        // Créer les couches (layers) pour le rendu PixiJS
        this.bgLayer = new PIXI.Container();
        this.mainLayer = new PIXI.Container();
        this.overlayLayer = new PIXI.Container();

        this.app.stage.addChild(this.bgLayer);
        this.app.stage.addChild(this.mainLayer);
        this.app.stage.addChild(this.overlayLayer);

        // Créer objets Graphics réutilisables (batching pour performance)
        // Ordre d'ajout = ordre de rendu (z-index)
        this.volumeGraphics = new PIXI.Graphics();
        this.indicatorBgGraphics = new PIXI.Graphics();
        this.wicksGraphics = new PIXI.Graphics();
        this.bodiesDownGraphics = new PIXI.Graphics();
        this.bodiesUpFilledGraphics = new PIXI.Graphics();
        this.bodiesUpHollowGraphics = new PIXI.Graphics();
        this.bordersGraphics = new PIXI.Graphics();
        this.realtimeMarkersGraphics = new PIXI.Graphics();
        this.rsiGraphics = new PIXI.Graphics();

        this.mainLayer.addChild(this.volumeGraphics);
        this.mainLayer.addChild(this.indicatorBgGraphics);
        this.mainLayer.addChild(this.wicksGraphics);
        this.mainLayer.addChild(this.bodiesDownGraphics);
        this.mainLayer.addChild(this.bodiesUpFilledGraphics);
        this.mainLayer.addChild(this.bodiesUpHollowGraphics);
        this.mainLayer.addChild(this.bordersGraphics);
        this.mainLayer.addChild(this.realtimeMarkersGraphics);
        this.mainLayer.addChild(this.rsiGraphics);

        // Canvas 2D overlay pour UI elements (axes, labels, crosshair)
        this.overlayCanvas = document.createElement('canvas');
        this.overlayCanvas.style.position = 'absolute';
        this.overlayCanvas.style.left = '0';
        this.overlayCanvas.style.top = '0';
        this.overlayCanvas.style.pointerEvents = 'all';
        this.overlayCtx = this.overlayCanvas.getContext('2d')!;
        this.container.appendChild(this.overlayCanvas);

        // Stub pour compatibilité
        this.mainCtx = {
            clearRect: () => {
            },
            fillStyle: '',
            strokeStyle: '',
            fillRect: () => {
            },
            strokeRect: () => {
            },
            beginPath: () => {
            },
            moveTo: () => {
            },
            lineTo: () => {
            },
            stroke: () => {
            },
            fill: () => {
            },
            save: () => {
            },
            restore: () => {
            },
            setLineDash: () => {
            },
            measureText: (text) => ({width: text.length * 7}),
            lineWidth: 1,
            globalAlpha: 1,
            font: '',
            textAlign: '',
            textBaseline: '',
            fillText: () => {
            }
        };

        this.setupCanvasSizes();

        // Configuration timeframes (sera définie par setTimeframes())
        this.timeframes = [];

        // Indicateurs multi-timeframes
        this.rsiData = new Map();
        this.rsiVisibility = new Map();
        this.rsiHistoricalData = new Map();

        // Polling temps réel
        this.realtimeCandles = new Map();
        this.realtimeWs = null;
        this.realtimeSubscribed = new Set();
        this.realtimeUpdating = false;

        // Optimisation rendu
        this.renderScheduled = false;
        this.overlayRenderScheduled = false;
        this.rafId = null;

        // Conteneur pour légendes interactives
        this.legendContainer = document.createElement('div');
        this.legendContainer.style.cssText = 'position: absolute; bottom: 45px; left: 80px; z-index: 10; pointer-events: auto;';
        this.container.appendChild(this.legendContainer);

        // État du chart
        this.state = {
            data: [],
            currentTimeframe: '1d',
            symbol: null,
            isLoading: false,

            // Vue (en timestamps secondes)
            viewStart: 0,
            viewEnd: 0,

            // Contraintes de zoom
            minBars: 80,
            maxBars: 200,

            // Prix
            priceMin: 0,
            priceMax: 0,

            // Souris
            mouseX: -1,
            mouseY: -1,
            isDragging: false,
            dragStartX: 0,
            dragStartViewStart: 0,
            dragStartViewEnd: 0,

            // Crosshair
            showCrosshair: false,
            crosshairCandle: null,

            // Zoom throttling
            isProcessingZoom: false,
            lastZoomTime: 0
        };

        // Callbacks
        this.callbacks = {
            onLoadData: options.onLoadData || (async () => []),
            onTimeframeChange: options.onTimeframeChange || (async () => {
            }),
            onError: options.onError || console.error
        };

        // Style (sera mis à jour depuis config)
        this.theme = {} as ThemeColors;
        this.updateTheme();

        // Layout
        this.layout = {
            marginLeft: 70,
            marginRight: 60,
            marginTop: 15,
            marginBottom: 40
        };

        // Configurer les événements souris/tactile
        this.setupEvents();

        // Rendu initial
        this.renderBackground();
    }

    static async create(container: HTMLElement, options: ChartOptions = {}): Promise<ChartEngine> {
        // Initialiser PixiJS Application (async dans v8)
        const app = new PIXI.Application();
        await app.init({
            width: container.clientWidth,
            height: container.clientHeight,
            backgroundColor: 0xffffff,
            antialias: true,
            resolution: window.devicePixelRatio || 1,
            autoDensity: true
        });

        return new ChartEngine(container, app, options);
    }

    parseTimeframeToSeconds(tf) {
        const match = tf.match(/^(\d+)([mhd])$/);
        if (!match) return 86400; // Default 1d

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

    calculateRSI(candles, period = 14) {
        if (candles.length < period + 1) return [];

        const rsi = [];
        let gains = 0;
        let losses = 0;

        // Première période
        for (let i = 1; i <= period; i++) {
            const change = candles[i].close - candles[i - 1].close;
            if (change > 0) gains += change;
            else losses += Math.abs(change);
        }

        let avgGain = gains / period;
        let avgLoss = losses / period;
        let rs = avgGain / avgLoss;
        rsi.push({time: candles[period].time, value: 100 - (100 / (1 + rs))});

        // Suite
        for (let i = period + 1; i < candles.length; i++) {
            const change = candles[i].close - candles[i - 1].close;
            const gain = change > 0 ? change : 0;
            const loss = change < 0 ? Math.abs(change) : 0;

            avgGain = (avgGain * (period - 1) + gain) / period;
            avgLoss = (avgLoss * (period - 1) + loss) / period;
            rs = avgGain / avgLoss;
            rsi.push({time: candles[i].time, value: 100 - (100 / (1 + rs))});
        }

        return rsi;
    }

    setTimeframes(timeframes) {
        this.timeframes = timeframes.sort((a, b) =>
            this.parseTimeframeToSeconds(a) - this.parseTimeframeToSeconds(b)
        );
        console.log(`⚙️ Timeframes configured: ${this.timeframes.join(', ')}`);
    }

    updateTheme() {
        const themeColors = chartConfig.getThemeColors();
        this.theme = {
            ...themeColors,
            upColor: chartConfig.get('colors.upColor'),
            downColor: chartConfig.get('colors.downColor'),
            upBorderColor: chartConfig.get('colors.upBorderColor'),
            downBorderColor: chartConfig.get('colors.downBorderColor'),
            tooltipBg: themeColors.bg === '#ffffff'
                ? 'rgba(255, 255, 255, 0.95)'
                : 'rgba(30, 30, 30, 0.95)',
            tooltipBorder: themeColors.grid
        };
    }

    setupCanvasSizes() {
        const rect = this.container.getBoundingClientRect();
        const dpr = window.devicePixelRatio || 1;

        // Reset canvas size (this also resets the context)
        this.overlayCanvas.width = rect.width * dpr;
        this.overlayCanvas.height = rect.height * dpr;
        this.overlayCanvas.style.width = rect.width + 'px';
        this.overlayCanvas.style.height = rect.height + 'px';

        // Reapply scale after reset
        this.overlayCtx.setTransform(dpr, 0, 0, dpr, 0, 0);
    }

    resizeCanvas() {
        const rect = this.container.getBoundingClientRect();
        this.app.renderer.resize(rect.width, rect.height);
        this.setupCanvasSizes();
    }

    setupEvents() {
        // Attacher les événements à overlayCanvas (qui est au-dessus de PixiJS)
        const canvas = this.overlayCanvas;

        // Wheel zoom
        canvas.addEventListener('wheel', (e) => this.handleWheel(e));

        // Mouse tracking
        canvas.addEventListener('mousemove', (e) => this.handleMouseMove(e));
        canvas.addEventListener('mouseleave', () => this.handleMouseLeave());

        // Drag pan
        canvas.addEventListener('mousedown', (e) => this.handleMouseDown(e));
        canvas.addEventListener('mouseup', () => this.handleMouseUp());

        // Resize
        window.addEventListener('resize', () => this.handleResize());
    }

    handleWheel(e) {
        e.preventDefault();
        if (this.state.isLoading || this.state.data.length === 0) return;

        // Throttle: ignorer si on est déjà en train de traiter un zoom
        const now = Date.now();
        if (this.state.isProcessingZoom) {
            console.log('⏭️  Skipping zoom (already processing)');
            return;
        }

        // Throttle: au minimum 50ms entre chaque zoom (20 fps max)
        if (now - this.state.lastZoomTime < 50) {
            console.log('⏭️  Skipping zoom (too fast)');
            return;
        }

        this.state.isProcessingZoom = true;
        this.state.lastZoomTime = now;

        try {
            const rect = this.overlayCanvas.getBoundingClientRect();
            const mouseX = e.clientX - rect.left;

            // Calculer position dans la zone du chart (sans marges)
            const chartWidth = rect.width - this.layout.marginLeft - this.layout.marginRight;
            const mouseXInChart = mouseX - this.layout.marginLeft;

            // Clamp dans la zone du chart
            const clampedX = Math.max(0, Math.min(chartWidth, mouseXInChart));

            // Timestamp pivot sous la souris (dans la zone du chart)
            const viewWidth = this.state.viewEnd - this.state.viewStart;
            const pivotRatio = clampedX / chartWidth;
            const pivotTime = this.state.viewStart + pivotRatio * viewWidth;

            const pivotDateBefore = new Date(pivotTime * 1000).toISOString().substring(0, 16);
            console.log(`🎯 BEFORE zoom: Pivot at x=${clampedX.toFixed(0)}px, ratio=${pivotRatio.toFixed(3)}, time=${pivotDateBefore}`);
            console.log(`   View: ${new Date(this.state.viewStart * 1000).toISOString().substring(0, 16)} → ${new Date(this.state.viewEnd * 1000).toISOString().substring(0, 16)} (${Math.round(viewWidth / this.parseTimeframeToSeconds(this.state.currentTimeframe))} bars)`);

            // Facteur de zoom
            const zoomFactor = e.deltaY > 0 ? 1.15 : 0.87;
            let newWidth = viewWidth * zoomFactor;

            // Vérifier contrainte AVANT de modifier la vue
            const tfSeconds = this.parseTimeframeToSeconds(this.state.currentTimeframe);
            const minWidth = 50 * tfSeconds; // 50 bars minimum

            // Si on essaie de zoomer en dessous du minimum ET qu'on est sur la TF la plus basse
            const currentIndex = this.timeframes.indexOf(this.state.currentTimeframe);
            if (newWidth < minWidth && currentIndex === 0 && zoomFactor < 1) {
                // Bloquer le zoom IN, ne rien faire
                console.log(`⛔ Can't zoom more on ${this.state.currentTimeframe} (minimum ${Math.round(viewWidth / tfSeconds)} bars)`);
                return;
            }

            // Recalculer bornes en gardant le pivot à la même position relative
            this.state.viewStart = pivotTime - newWidth * pivotRatio;
            this.state.viewEnd = pivotTime + newWidth * (1 - pivotRatio);

            console.log(`   AFTER zoom: View: ${new Date(this.state.viewStart * 1000).toISOString().substring(0, 16)} → ${new Date(this.state.viewEnd * 1000).toISOString().substring(0, 16)} (${Math.round(newWidth / this.parseTimeframeToSeconds(this.state.currentTimeframe))} bars)`);

            // Vérifier que le pivot est toujours au même endroit
            const newViewWidth = this.state.viewEnd - this.state.viewStart;
            const verifyPivotTime = this.state.viewStart + pivotRatio * newViewWidth;
            const pivotDateAfter = new Date(verifyPivotTime * 1000).toISOString().substring(0, 16);
            console.log(`   ✓ Verify pivot: ${pivotDateAfter} (should be ${pivotDateBefore})`);

            // Vérifier si changement de TF nécessaire, en passant le pivot pour le préserver
            const didChange = this.checkAndSwitchTimeframe(pivotTime, pivotRatio);

            if (!didChange) {
                // Pas de changement de TF
                // Vérifier qu'on n'est pas en dessous du minimum
                const currentWidth = this.state.viewEnd - this.state.viewStart;
                if (currentWidth < minWidth) {
                    // Clamp à minWidth en gardant le pivot
                    this.state.viewStart = pivotTime - minWidth * pivotRatio;
                    this.state.viewEnd = pivotTime + minWidth * (1 - pivotRatio);
                    console.log(`   ⚠️ Clamped to minWidth, new view: ${new Date(this.state.viewStart * 1000).toISOString().substring(0, 16)} → ${new Date(this.state.viewEnd * 1000).toISOString().substring(0, 16)}`);
                }

                // Vérifier si on doit recharger plus de données
                this.checkAndReloadData();

                // Planifier render via RAF
                this.scheduleRender();
            }
            // Si changement de TF, le callback va déclencher loadData qui va render
        } finally {
            // Toujours relâcher le flag
            this.state.isProcessingZoom = false;
        }
    }

    handleMouseMove(e) {
        const rect = this.overlayCanvas.getBoundingClientRect();
        this.state.mouseX = e.clientX - rect.left;
        this.state.mouseY = e.clientY - rect.top;

        if (this.state.isDragging) {
            this.handleDrag(e);
        } else {
            this.state.showCrosshair = true;
            this.updateCrosshair();
            // Utiliser scheduleOverlayRender au lieu d'appel direct
            this.scheduleOverlayRender();
        }
    }

    handleMouseLeave() {
        this.state.showCrosshair = false;
        this.state.mouseX = -1;
        this.state.mouseY = -1;

        // Nettoyer l'overlay et redessiner seulement les éléments statiques
        const w = this.app.screen.width;
        const h = this.app.screen.height;
        this.overlayCtx.clearRect(0, 0, w, h);
        this.renderStaticOverlay();
    }

    handleMouseDown(e) {
        if (this.state.isLoading || this.state.data.length === 0) return;

        this.state.isDragging = true;
        this.state.dragStartX = e.clientX;
        this.state.dragStartViewStart = this.state.viewStart;
        this.state.dragStartViewEnd = this.state.viewEnd;
        this.overlayCanvas.style.cursor = 'grabbing';
    }

    handleMouseUp() {
        this.state.isDragging = false;
        this.overlayCanvas.style.cursor = 'crosshair';
        this.checkAndReloadData();
    }

    handleDrag(e) {
        const dx = e.clientX - this.state.dragStartX;
        const rect = this.overlayCanvas.getBoundingClientRect();
        const viewWidth = this.state.dragStartViewEnd - this.state.dragStartViewStart;
        const timeShift = -dx * viewWidth / rect.width;

        this.state.viewStart = this.state.dragStartViewStart + timeShift;
        this.state.viewEnd = this.state.dragStartViewEnd + timeShift;

        // Planifier render via RAF pour éviter trop de rendus pendant le drag
        this.scheduleRender();
    }

    handleResize() {
        this.resizeCanvas();
        this.renderBackground();
        this.render();
    }

    checkAndSwitchTimeframe(pivotTime = null, pivotRatio = null) {
        const viewWidth = this.state.viewEnd - this.state.viewStart;
        const tfSeconds = this.parseTimeframeToSeconds(this.state.currentTimeframe);
        const visibleBars = viewWidth / tfSeconds;

        const currentIndex = this.timeframes.indexOf(this.state.currentTimeframe);

        // Zoom in: < minBars (changement plus agressif)
        if (visibleBars < this.state.minBars && currentIndex > 0) {
            const newTF = this.timeframes[currentIndex - 1];
            const savedRange = {
                start: this.state.viewStart,
                end: this.state.viewEnd,
                oldTFSeconds: tfSeconds,  // Sauvegarder l'ancien TF pour calculer le nombre de bougies
                pivotTime: pivotTime,     // Sauvegarder le pivot pour le préserver
                pivotRatio: pivotRatio    // Position relative du pivot dans la vue
            };
            console.log(`🔽 Zoom IN: ${this.state.currentTimeframe} → ${newTF} (${Math.round(visibleBars)} bars < ${this.state.minBars})`);
            console.log(`   💾 Saving current view with pivot at ${pivotTime ? new Date(pivotTime * 1000).toISOString().substring(0, 16) : 'N/A'} (ratio: ${pivotRatio ? pivotRatio.toFixed(3) : 'N/A'})`);
            this.callbacks.onTimeframeChange(newTF, savedRange);
            return true;
        }

        // Zoom out: > maxBars (changement plus agressif)
        if (visibleBars > this.state.maxBars && currentIndex < this.timeframes.length - 1) {
            const newTF = this.timeframes[currentIndex + 1];
            const savedRange = {
                start: this.state.viewStart,
                end: this.state.viewEnd,
                oldTFSeconds: tfSeconds,  // Sauvegarder l'ancien TF pour calculer le nombre de bougies
                pivotTime: pivotTime,     // Sauvegarder le pivot pour le préserver
                pivotRatio: pivotRatio    // Position relative du pivot dans la vue
            };
            console.log(`🔼 Zoom OUT: ${this.state.currentTimeframe} → ${newTF} (${Math.round(visibleBars)} bars > ${this.state.maxBars})`);
            console.log(`   💾 Saving current view with pivot at ${pivotTime ? new Date(pivotTime * 1000).toISOString().substring(0, 16) : 'N/A'} (ratio: ${pivotRatio ? pivotRatio.toFixed(3) : 'N/A'})`);
            this.callbacks.onTimeframeChange(newTF, savedRange);
            return true;
        }

        return false;
    }

    checkAndReloadData() {
        if (this.state.isLoading || this.state.data.length === 0) return;

        // Trouver les limites des données chargées
        const dataStart = this.state.data[0].time;
        const dataEnd = this.state.data[this.state.data.length - 1].time;

        // Calculer la largeur de vue
        const viewWidth = this.state.viewEnd - this.state.viewStart;
        const threshold = viewWidth * 0.5; // Recharger si on est à 50% du bord

        // Vérifier si on s'approche des bords
        const needsReload =
            this.state.viewStart < (dataStart + threshold) ||
            this.state.viewEnd > (dataEnd - threshold);

        if (needsReload) {
            console.log(`🔄 Reloading data - view approaching data boundaries`);
            console.log(`   Data range: ${new Date(dataStart * 1000).toISOString().substring(0, 16)} → ${new Date(dataEnd * 1000).toISOString().substring(0, 16)}`);
            console.log(`   View range: ${new Date(this.state.viewStart * 1000).toISOString().substring(0, 16)} → ${new Date(this.state.viewEnd * 1000).toISOString().substring(0, 16)}`);

            // Recharger avec la vue actuelle (loadData ajoutera automatiquement les marges)
            this.loadData(this.state.symbol, this.state.currentTimeframe);
        }
    }

    updateCrosshair() {
        if (!this.state.showCrosshair || this.state.data.length === 0) return;

        const rect = this.overlayCanvas.getBoundingClientRect();
        const chartWidth = rect.width - this.layout.marginLeft - this.layout.marginRight;

        // Trouver la bougie sous le curseur
        const relativeX = (this.state.mouseX - this.layout.marginLeft) / chartWidth;
        const timestampUnderCursor = this.state.viewStart + relativeX * (this.state.viewEnd - this.state.viewStart);

        // Chercher la bougie la plus proche
        let closest = null;
        let minDist = Infinity;

        for (const candle of this.state.data) {
            if (candle.time < this.state.viewStart || candle.time > this.state.viewEnd) continue;

            const dist = Math.abs(candle.time - timestampUnderCursor);
            if (dist < minDist) {
                minDist = dist;
                closest = candle;
            }
        }

        this.state.crosshairCandle = closest;
    }

    async loadData(symbol, timeframe, savedRange = null) {
        console.log(`📊 loadData() called: symbol=${symbol}, TF=${timeframe}, isLoading=${this.state.isLoading}`);

        // Éviter les appels concurrents
        if (this.state.isLoading) {
            console.warn('⚠️ Already loading data, ignoring loadData() call');
            return;
        }

        // Sauvegarder la plage temporelle exacte si on change de TF
        if (savedRange === null && this.state.data.length > 0 && this.state.viewStart !== 0) {
            savedRange = {
                start: this.state.viewStart,
                end: this.state.viewEnd
            };
            console.log(`💾 Saving exact range: ${new Date(savedRange.start * 1000).toISOString()} → ${new Date(savedRange.end * 1000).toISOString()}`);
        }

        this.state.symbol = symbol;
        this.state.currentTimeframe = timeframe;
        this.state.isLoading = true;

        // Afficher loading
        this.renderLoading();

        try {
            // Calculer la plage de dates à charger (élargie pour permettre le pan)
            let fetchStart = null;
            let fetchEnd = null;

            if (savedRange) {
                // Lors d'un changement de TF, charger une plage élargie autour de la vue sauvegardée
                const width = savedRange.end - savedRange.start;
                const margin = width * 2;  // 2x de marge de chaque côté
                fetchStart = Math.floor(savedRange.start - margin);
                fetchEnd = Math.ceil(savedRange.end + margin);
            } else if (this.state.viewStart !== 0 && this.state.viewEnd !== 0) {
                // Si on a déjà une vue, charger autour de celle-ci
                const width = this.state.viewEnd - this.state.viewStart;
                const margin = width * 2;
                fetchStart = Math.floor(this.state.viewStart - margin);
                fetchEnd = Math.ceil(this.state.viewEnd + margin);
            }
            // Sinon fetchStart et fetchEnd restent null -> l'API chargera les dernières données

            console.log(`📡 Fetching data range: ${fetchStart ? new Date(fetchStart * 1000).toISOString().substring(0, 16) : 'auto'} → ${fetchEnd ? new Date(fetchEnd * 1000).toISOString().substring(0, 16) : 'auto'}`);

            let data = await this.callbacks.onLoadData(symbol, timeframe, fetchStart, fetchEnd);

            if (!Array.isArray(data) || data.length === 0) {
                throw new Error('No data received');
            }

            this.state.data = data;
            console.log(`✅ Loaded ${data.length} candles for ${symbol} ${timeframe}`);

            // Détecter et combler les gaps si nécessaire
            const didFetch = await this.fillGapsIfNeeded(symbol, timeframe, data);
            if (didFetch) {
                data = await this.callbacks.onLoadData(symbol, timeframe, fetchStart, fetchEnd);
                this.state.data = data;
                console.log(`🔄 Reloaded ${data.length} candles après fetch`);
            }

            // Calculer prix min/max global
            const prices = data.flatMap(c => [c.high, c.low]);
            this.state.priceMin = Math.min(...prices);
            this.state.priceMax = Math.max(...prices);

            // Initialiser ou restaurer vue
            if (savedRange === null) {
                // Premier chargement
                this.fitToData();
            } else {
                // Restaurer la MÊME plage temporelle exacte
                this.restoreViewFromRange(savedRange);
            }

            await this.loadIndicatorData();

            this.renderBackground();
            this.render();

            // Démarrer/mettre à jour WebSocket (ne fait rien si config inchangée)
            this.startRealtimeUpdates();

        } catch (error) {
            this.callbacks.onError(error);
            this.renderError(error.message);
        } finally {
            this.state.isLoading = false;
        }
    }

    async fillGapsIfNeeded(symbol: string, timeframe: string, data: any[]): Promise<boolean> {
        if (data.length === 0) return false;

        const lastCandle = data[data.length - 1];
        const now = Math.floor(Date.now() / 1000);
        const gapSeconds = now - lastCandle.time;
        const tfSeconds = this.parseTimeframeToSeconds(timeframe);
        const missingCandles = gapSeconds / tfSeconds;

        // Seuil: > 2 bougies manquantes (laisse marge pour bougie en cours)
        if (missingCandles > 2) {
            console.log(`🔍 Gap détecté: ${Math.floor(missingCandles)} bougies manquantes, fetch...`);

            try {
                const response = await fetch(`/api/fetch?symbol=${symbol}&timeframe=${timeframe}`, {
                    method: 'POST'
                });

                if (response.ok) {
                    const result = await response.json();
                    if (result.inserted > 0) {
                        console.log(`✅ ${result.inserted} bougies ajoutées`);
                        return true;
                    }
                }
            } catch (error) {
                console.error('❌ Erreur fetch:', error);
            }
        }
        return false;
    }

    async startRealtimeUpdates() {
        if (!this.state.symbol) {
            console.warn('⚠️ Cannot start realtime updates: no symbol');
            return;
        }

        // Éviter les appels concurrents
        if (this.realtimeUpdating) {
            console.log('⏭️  Already updating realtime, skipping');
            return;
        }

        this.realtimeUpdating = true;

        try {
            // Nettoyer les anciennes souscriptions qui ne correspondent pas au symbole actuel
            const oldSubscriptions = Array.from(this.realtimeSubscribed);
            for (const streamKey of oldSubscriptions) {
                const [symbol, _] = streamKey.split(':');
                if (symbol !== this.state.symbol) {
                    this.realtimeSubscribed.delete(streamKey);
                    console.log(`🧹 Removed old subscription: ${streamKey}`);
                }
            }

            // Nettoyer les bougies temps-réel de l'ancienne paire
            this.realtimeCandles.clear();

            // Déterminer les TF à surveiller = celles affichées pour le RSI
            const currentIdx = this.timeframes.indexOf(this.state.currentTimeframe);
            const watchedTFs = new Set<string>();

            // TF inférieure (RSI)
            if (currentIdx > 0) {
                watchedTFs.add(this.timeframes[currentIdx - 1]);
            }

            // TF actuelle (affichage + RSI)
            watchedTFs.add(this.state.currentTimeframe);

            // TF supérieure (RSI)
            if (currentIdx < this.timeframes.length - 1) {
                watchedTFs.add(this.timeframes[currentIdx + 1]);
            }

            const tfArray = Array.from(watchedTFs).sort((a, b) =>
                this.parseTimeframeToSeconds(a) - this.parseTimeframeToSeconds(b)
            );

            // Identifier les nouveaux streams à souscrire
            const newStreams: string[] = [];
            for (const tf of tfArray) {
                const streamKey = `${this.state.symbol}:${tf}`;
                if (!this.realtimeSubscribed.has(streamKey)) {
                    newStreams.push(tf);
                    this.realtimeSubscribed.add(streamKey);
                }
            }

            // Si pas de WebSocket, en créer un
            if (!this.realtimeWs || this.realtimeWs.readyState !== WebSocket.OPEN) {
                this.connectWebSocket();
            }

            // Envoyer la souscription via WebSocket
            if (newStreams.length > 0 && this.realtimeWs && this.realtimeWs.readyState === WebSocket.OPEN) {
                console.log(`🔌 Subscribing to new streams for ${this.state.symbol}: ${newStreams.join(', ')}`);

                const subscribeMsg = {
                    action: 'subscribe',
                    symbol: this.state.symbol,
                    timeframes: tfArray
                };

                this.realtimeWs.send(JSON.stringify(subscribeMsg));
            }

        } finally {
            this.realtimeUpdating = false;
        }
    }

    connectWebSocket() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/ws/realtime`;

        console.log('🔌 Connecting to WebSocket:', wsUrl);

        this.realtimeWs = new WebSocket(wsUrl);

        this.realtimeWs.onopen = () => {
            console.log('✅ WebSocket connected');
        };

        this.realtimeWs.onmessage = (event) => {
            try {
                const msg = JSON.parse(event.data);

                if (msg.type === 'candle_update') {
                    // Ignorer les mises à jour qui ne correspondent pas au symbole actuel
                    if (msg.symbol !== this.state.symbol) {
                        console.log(`⏭️  Ignoring candle update for ${msg.symbol} (current: ${this.state.symbol})`);
                        return;
                    }
                    this.handleRealtimeCandle(msg.timeframe, msg.candle, msg.candle.is_closed, true);
                } else if (msg.type === 'subscribed') {
                    console.log(`✅ Subscribed to ${msg.symbol} [${msg.timeframes.join(', ')}]`);
                } else if (msg.type === 'error') {
                    console.error('❌ WebSocket error:', msg.message);
                }
            } catch (error) {
                console.error('Failed to parse WebSocket message:', error);
            }
        };

        this.realtimeWs.onerror = (error) => {
            console.error('❌ WebSocket error:', error);
        };

        this.realtimeWs.onclose = () => {
            console.log('🔌 WebSocket closed, will reconnect on next update');
            this.realtimeWs = null;
        };
    }

    stopRealtimeUpdates() {
        if (this.realtimeWs) {
            this.realtimeWs.close();
            this.realtimeWs = null;
        }
        this.realtimeCandles.clear();
        // NE PAS vider realtimeSubscribed - on garde les souscriptions actives
        console.log('🛑 Stopped realtime WebSocket');
    }

    handleRealtimeCandle(timeframe: string, candle: any, isComplete: boolean, shouldRender: boolean = true): boolean {
        // Mettre à jour la bougie en cours pour cette TF
        this.realtimeCandles.set(timeframe, candle);

        let updated = false;

        // Si c'est la TF actuelle affichée
        if (timeframe === this.state.currentTimeframe) {
            // Trouver l'index de la dernière bougie
            const lastIndex = this.state.data.length - 1;
            if (lastIndex >= 0) {
                const lastCandle = this.state.data[lastIndex];
                const tfSeconds = this.parseTimeframeToSeconds(timeframe);

                // Vérifier si c'est la même bougie (même timestamp)
                if (lastCandle.time === candle.time) {
                    // Mise à jour de la bougie existante
                    this.state.data[lastIndex] = candle;
                    updated = true;
                } else if (candle.time === lastCandle.time + tfSeconds) {
                    // Nouvelle bougie consécutive → l'ajouter
                    this.state.data.push(candle);
                    console.log(`✅ New candle added for ${timeframe} at ${new Date(candle.time * 1000).toISOString().substring(0, 16)}`);

                    // Si on affiche les dernières données, ajuster la vue pour suivre
                    const isAtEnd = this.state.viewEnd >= (lastCandle.time + tfSeconds * 0.5);
                    if (isAtEnd) {
                        // Décaler la vue pour suivre
                        const viewWidth = this.state.viewEnd - this.state.viewStart;
                        this.state.viewEnd = candle.time + tfSeconds;
                        this.state.viewStart = this.state.viewEnd - viewWidth;
                        console.log(`📍 Auto-scroll: view moved to ${new Date(this.state.viewStart * 1000).toISOString().substring(0, 16)} → ${new Date(this.state.viewEnd * 1000).toISOString().substring(0, 16)}`);
                    }
                    updated = true;
                } else if (candle.time > lastCandle.time + tfSeconds) {
                    // Gap détecté mais on ne reload PAS : les bougies intermédiaires sont simplement manquantes
                    // Le WebSocket n'envoie que la bougie actuelle, pas l'historique
                    console.log(`📍 Skipping gap (${Math.floor((candle.time - lastCandle.time - tfSeconds) / tfSeconds)} candles missing), adding current candle`);
                    this.state.data.push(candle);
                    updated = true;
                }

                if (updated) {
                    // Recalculer le RSI pour toutes les TF visibles (pas seulement currentTimeframe)
                    this.updateRealtimeRSIForAllTimeframes();

                    // Planifier render seulement si demandé
                    if (shouldRender) {
                        this.scheduleRender();
                    }
                }
            }
        } else {
            // Pour les autres TF (utilisées pour le RSI), on ne met PAS à jour en temps réel
            // car updateSingleTimeframeRSI() est trop lourd (fetch + recalcul complet)
            // Le RSI sera mis à jour au prochain changement de TF ou zoom/pan
            if (isComplete) {
                console.log(`📊 Complete candle on ${timeframe} (RSI update skipped for performance)`);
                updated = true;
            }
        }

        return updated;
    }

    async updateSingleTimeframeRSI(timeframe: string) {
        if (!chartConfig.get('indicators.enabled')) return;

        const period = chartConfig.get('indicators.rsi.period') || 14;

        try {
            // Charger avec marge
            const margin = (this.state.viewEnd - this.state.viewStart) * 2;
            const data = await this.callbacks.onLoadData(
                this.state.symbol,
                timeframe,
                Math.floor(this.state.viewStart - margin),
                Math.ceil(this.state.viewEnd + margin)
            );

            if (data && data.length > period) {
                const rsi = this.calculateRSI(data, period);
                const referenceTimestamps = this.state.data.map(c => c.time);
                const resampled = this.resampleIndicatorToGrid(rsi, referenceTimestamps);
                this.rsiData.set(timeframe, resampled);
                console.log(`📊 Updated RSI for ${timeframe} (${resampled.length} points)`);

                // Re-render pour afficher le nouveau RSI
                this.render();
            }
        } catch (e) {
            console.error(`❌ Failed to update RSI for ${timeframe}:`, e);
        }
    }

    updateRealtimeRSI() {
        if (!chartConfig.get('indicators.enabled')) return;

        const period = chartConfig.get('indicators.rsi.period') || 14;

        // Optimisation : limiter à period*2 derniers échantillons (28 pour period=14)
        // Suffisant pour RSI précis (14 warm-up + 14 calcul)
        if (this.state.data.length > period + 1) {
            const maxSamples = period * 2;

            // Prendre seulement les dernières bougies nécessaires
            const dataToUse = this.state.data.length > maxSamples
                ? this.state.data.slice(-maxSamples)
                : this.state.data;

            const rsi = this.calculateRSI(dataToUse, period);

            // Mettre à jour seulement les derniers points du RSI existant
            const existingRSI = this.rsiData.get(this.state.currentTimeframe) || [];
            const rsiPointsToReplace = Math.min(rsi.length, 10); // Remplacer max 10 derniers points

            if (existingRSI.length > rsiPointsToReplace) {
                // Garder le début, remplacer la fin
                const updatedRSI = existingRSI.slice(0, -rsiPointsToReplace).concat(rsi.slice(-rsiPointsToReplace));
                this.rsiData.set(this.state.currentTimeframe, updatedRSI);
            } else {
                // Pas assez de données existantes, tout remplacer
                this.rsiData.set(this.state.currentTimeframe, rsi);
            }
        }
    }

    updateRealtimeRSIForAllTimeframes() {
        if (!chartConfig.get('indicators.enabled')) return;

        const period = chartConfig.get('indicators.rsi.period') || 14;
        const currentTFSeconds = this.parseTimeframeToSeconds(this.state.currentTimeframe);

        // Itérer sur toutes les TF pour lesquelles on a du RSI
        for (const [tf, existingRSI] of this.rsiData.entries()) {
            try {
                const tfSeconds = this.parseTimeframeToSeconds(tf);

                // Récupérer la bougie temps-réel pour cette TF
                const realtimeCandle = this.realtimeCandles.get(tf);

                // Toujours utiliser la bougie temps-réel si disponible (considérée comme complète)
                if (!realtimeCandle) {
                    // Si on n'a pas de bougie temps-réel, on saute cette TF
                    continue;
                }

                // Construire les données pour le calcul : on prend les données de state.data
                // et on les utilise pour construire les échantillons pour cette TF
                // On va chercher dans le cache historique (rsiHistoricalData) si disponible
                if (!this.rsiHistoricalData) {
                    this.rsiHistoricalData = new Map();
                }

                // Si on n'a pas de données historiques pour cette TF, on lance un chargement asynchrone
                if (!this.rsiHistoricalData.has(tf)) {
                    // Lancer le chargement en arrière-plan (ne pas attendre)
                    this.loadHistoricalDataForRSI(tf, period);
                    continue;
                }

                // Récupérer les données historiques du cache
                let data = this.rsiHistoricalData.get(tf) || [];

                // Limiter aux 28 derniers échantillons
                const maxSamples = period * 2; // 28 pour period=14
                data = data.slice(-maxSamples);

                // Ajouter ou mettre à jour avec la bougie temps-réel
                if (data.length > 0) {
                    const lastHistorical = data[data.length - 1];
                    if (lastHistorical.time === realtimeCandle.time) {
                        // Remplacer la dernière bougie par la version temps-réel
                        data[data.length - 1] = realtimeCandle;
                    } else if (realtimeCandle.time > lastHistorical.time) {
                        // Ajouter la bougie temps-réel si elle est plus récente
                        data.push(realtimeCandle);
                        // Limiter à nouveau après ajout
                        data = data.slice(-maxSamples);
                        // Mettre à jour le cache
                        this.rsiHistoricalData.set(tf, data);
                    }
                }

                // Calculer le RSI sur ces données
                if (data.length > period + 1) {
                    const rsi = this.calculateRSI(data, period);

                    // Mettre à jour seulement les derniers points du RSI existant
                    const rsiPointsToReplace = Math.min(rsi.length, 10);

                    if (existingRSI.length > rsiPointsToReplace) {
                        // Garder le début, remplacer la fin
                        const updatedRSI = existingRSI.slice(0, -rsiPointsToReplace).concat(rsi.slice(-rsiPointsToReplace));
                        this.rsiData.set(tf, updatedRSI);
                    } else {
                        // Pas assez de données existantes, tout remplacer
                        this.rsiData.set(tf, rsi);
                    }
                }
            } catch (e) {
                console.error(`❌ Failed to update realtime RSI for ${tf}:`, e);
            }
        }
    }

    async loadHistoricalDataForRSI(tf: string, period: number) {
        try {
            const maxSamples = period * 2; // 28 pour period=14
            const tfSeconds = this.parseTimeframeToSeconds(tf);
            const margin = tfSeconds * maxSamples;

            const data = await this.callbacks.onLoadData(
                this.state.symbol,
                tf,
                Math.floor(Date.now() / 1000 - margin),
                Math.ceil(Date.now() / 1000)
            );

            if (data && data.length > 0) {
                // Stocker dans le cache
                if (!this.rsiHistoricalData) {
                    this.rsiHistoricalData = new Map();
                }
                this.rsiHistoricalData.set(tf, data.slice(-maxSamples));
                console.log(`📊 Loaded ${data.length} historical candles for ${tf} RSI calculation`);
            }
        } catch (e) {
            console.error(`❌ Failed to load historical data for ${tf}:`, e);
        }
    }

    async loadIndicatorData() {
        console.log(`📊 loadIndicatorData() called - enabled: ${chartConfig.get('indicators.enabled')}, symbol: ${this.state.symbol}, data: ${this.state.data.length} candles`);

        if (!chartConfig.get('indicators.enabled')) {
            this.rsiData.clear();
            return;
        }

        if (!this.state.symbol || this.state.data.length === 0) {
            console.warn('⚠️ Cannot load indicators: no symbol or no data');
            this.rsiData.clear();
            return;
        }

        // Clear anciennes données RSI
        this.rsiData.clear();

        // Initialiser le cache des données historiques pour le RSI temps-réel
        if (!this.rsiHistoricalData) {
            this.rsiHistoricalData = new Map();
        }

        // Auto: TF inférieure, actuelle, et supérieure (max 3)
        const currentIdx = this.timeframes.indexOf(this.state.currentTimeframe);
        const rsiTimeframes = [];

        if (currentIdx > 0) {
            // TF inférieure
            rsiTimeframes.push(this.timeframes[currentIdx - 1]);
        }

        // TF actuelle (toujours incluse)
        rsiTimeframes.push(this.state.currentTimeframe);

        if (currentIdx < this.timeframes.length - 1) {
            // TF supérieure
            rsiTimeframes.push(this.timeframes[currentIdx + 1]);
        }

        console.log(`📊 Current TF: ${this.state.currentTimeframe}, RSI TFs: ${rsiTimeframes.join(', ')}`);

        const period = chartConfig.get('indicators.rsi.period') || 14;

        // Timestamps de référence (candles actuels affichés)
        const referenceTimestamps = this.state.data.map(c => c.time);
        console.log(`📊 Reference timestamps: ${referenceTimestamps.length} candles`);

        for (const tf of rsiTimeframes) {
            try {
                // Charger avec marge large pour avoir assez de données
                const margin = (this.state.viewEnd - this.state.viewStart) * 2;
                console.log(`📊 Loading ${tf} data for RSI (${this.state.symbol})...`);
                let data = await this.callbacks.onLoadData(
                    this.state.symbol,
                    tf,
                    Math.floor(this.state.viewStart - margin),
                    Math.ceil(this.state.viewEnd + margin)
                );

                console.log(`📊 Loaded ${data?.length || 0} candles for ${tf}`);

                if (data && data.length > period) {
                    // Stocker les données historiques dans le cache pour les mises à jour temps-réel
                    const maxSamples = period * 2; // 28 pour period=14
                    this.rsiHistoricalData.set(tf, data.slice(-maxSamples));

                    const rsi = this.calculateRSI(data, period);
                    console.log(`📊 Calculated ${rsi.length} RSI points for ${tf}`);

                    // Rééchantillonner: pour chaque timestamp de candle affiché,
                    // prendre le dernier RSI calculé <= timestamp
                    const resampled = this.resampleIndicatorToGrid(rsi, referenceTimestamps);
                    console.log(`📊 Resampled to ${resampled.length} points for ${tf}`);
                    this.rsiData.set(tf, resampled);

                    // Initialiser visibilité à true par défaut
                    if (!this.rsiVisibility.has(tf)) {
                        this.rsiVisibility.set(tf, true);
                    }
                } else {
                    console.warn(`❌ Not enough data for RSI on ${tf}: ${data?.length || 0} candles (need > ${period})`);
                }
            } catch (e) {
                console.error(`❌ Failed to load RSI data for ${tf}:`, e);
            }
        }

        console.log(`📊 RSI data loaded for ${this.rsiData.size} timeframes`);
        this.updateRSILegend();
    }

    updateRSILegend() {
        this.legendContainer.innerHTML = '';
        if (this.rsiData.size === 0 || !chartConfig.get('indicators.enabled')) return;

        this.rsiData.forEach((data, tf) => {
            const color = this.getRSIColorForTimeframe(tf);

            const label = document.createElement('label');
            label.style.cssText = `display: inline-flex; align-items: center; margin: 0 10px 5px 0; padding: 4px 8px; background: rgba(0,0,0,0.7); border-radius: 4px; font-size: 11px; color: ${color}; cursor: pointer; font-family: monospace;`;

            const checkbox = document.createElement('input');
            checkbox.type = 'checkbox';
            checkbox.checked = this.rsiVisibility.get(tf) || false;
            checkbox.style.marginRight = '5px';
            checkbox.style.cursor = 'pointer';

            checkbox.addEventListener('change', () => {
                this.rsiVisibility.set(tf, checkbox.checked);
                this.scheduleRender();
            });

            label.appendChild(checkbox);
            label.appendChild(document.createTextNode(`RSI ${tf}`));
            this.legendContainer.appendChild(label);
        });
    }

    resampleIndicatorToGrid(indicatorData, targetTimestamps) {
        // Trier les données RSI par temps
        const sorted = [...indicatorData].sort((a, b) => a.time - b.time);

        return targetTimestamps.map(targetTime => {
            // Trouver le dernier point RSI calculé <= targetTime
            let lastValid = null;

            for (const point of sorted) {
                if (point.time <= targetTime) {
                    lastValid = point;
                } else {
                    break; // Dépassé, inutile de continuer
                }
            }

            if (!lastValid) return null;

            // Retourner avec le timestamp de référence (celui du candle affiché)
            return {time: targetTime, value: lastValid.value};
        }).filter(p => p !== null);
    }

    getRSIColorForTimeframe(tf) {
        // Gradient de chaleur: TF fine → jaune (précis), TF large → rouge (moins précis)
        const intensity = chartConfig.get('indicators.rsi.heatIntensity') || 1.0;

        // Utiliser l'ordre des TF chargées pour le RSI (pas toutes les TF)
        const loadedTFs = Array.from(this.rsiData.keys()).sort((a, b) =>
            this.parseTimeframeToSeconds(a) - this.parseTimeframeToSeconds(b)
        );

        const tfIndex = loadedTFs.indexOf(tf);
        if (tfIndex === -1) return '#FF8C00';

        // Normaliser: 0 = TF fine (jaune), 1 = TF large (rouge)
        const normalizedIndex = loadedTFs.length > 1
            ? tfIndex / (loadedTFs.length - 1)
            : 0.5;

        // Jaune (#FFD700) → Orange → Rouge (#FF0000)
        const r = 255;
        const g = Math.round(215 * (1 - normalizedIndex));
        const b = 0;

        return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
    }

    fitToData() {
        if (this.state.data.length === 0) return;

        const barsToShow = Math.min(100, this.state.data.length);
        const tfSeconds = this.parseTimeframeToSeconds(this.state.currentTimeframe);

        this.state.viewEnd = this.state.data[this.state.data.length - 1].time + tfSeconds;
        this.state.viewStart = this.state.viewEnd - barsToShow * tfSeconds;
    }

    restoreViewFromRange(savedRange) {
        // Garder EXACTEMENT la même fenêtre temporelle
        // Les bougies changent mais pas les coordonnées temporelles
        this.state.viewStart = savedRange.start;
        this.state.viewEnd = savedRange.end;

        const oldTFSeconds = savedRange.oldTFSeconds || this.parseTimeframeToSeconds(this.state.currentTimeframe);
        const newTFSeconds = this.parseTimeframeToSeconds(this.state.currentTimeframe);
        const savedWidth = savedRange.end - savedRange.start;

        const oldBarsCount = Math.round(savedWidth / oldTFSeconds);
        const newBarsCount = Math.round(savedWidth / newTFSeconds);

        console.log(`📊 Timeframe change: ${oldBarsCount} bars → ${newBarsCount} bars`);
        console.log(`   Fixed window: ${new Date(this.state.viewStart * 1000).toISOString().substring(0, 16)} → ${new Date(this.state.viewEnd * 1000).toISOString().substring(0, 16)}`);
    }

    renderBackground() {
        const w = this.app.screen.width;
        const h = this.app.screen.height;

        // Nettoyer le layer de fond
        this.bgLayer.removeChildren();

        // Fond
        const bgColor = this.theme.bg === '#ffffff' ? 0xffffff : parseInt(this.theme.bg.replace('#', ''), 16);
        const bg = new PIXI.Graphics();
        bg.beginFill(bgColor);
        bg.drawRect(0, 0, w, h);
        bg.endFill();
        this.bgLayer.addChild(bg);

        // Zone du chart
        const chartX = this.layout.marginLeft;
        const chartY = this.layout.marginTop;
        const chartW = w - this.layout.marginLeft - this.layout.marginRight;
        const chartH = h - this.layout.marginTop - this.layout.marginBottom;

        // Grille config
        const gridOpacity = chartConfig.get('grid.opacity');
        const gridColor = parseInt(this.theme.grid.replace('#', ''), 16);

        const grid = new PIXI.Graphics();
        grid.lineStyle(1, gridColor, gridOpacity);

        // Grille horizontale (prix)
        const numLines = chartConfig.get('grid.horizontal');
        for (let i = 0; i <= numLines; i++) {
            const y = chartY + (i / numLines) * chartH;
            grid.moveTo(chartX, y);
            grid.lineTo(chartX + chartW, y);
        }

        // Grille verticale (temps)
        const numTimeLines = chartConfig.get('grid.vertical');
        for (let i = 0; i <= numTimeLines; i++) {
            const x = chartX + (i / numTimeLines) * chartW;
            grid.moveTo(x, chartY);
            grid.lineTo(x, chartY + chartH);
        }

        this.bgLayer.addChild(grid);

        // Watermark
        if (chartConfig.get('watermark.enabled') && this.state.symbol) {
            this.renderWatermark(w, h);
        }
    }

    renderWatermark(w, h) {
        const opacity = chartConfig.get('watermark.opacity') / 100;
        const fontSize = chartConfig.get('watermark.fontSize');
        const text = `${this.state.symbol} ${this.state.currentTimeframe}`;

        const watermark = new PIXI.Text(text, {
            fontFamily: 'sans-serif',
            fontSize: fontSize,
            fontWeight: 'bold',
            fill: this.theme.textLight,
            align: 'center'
        });

        watermark.anchor.set(0.5);
        watermark.x = w / 2;
        watermark.y = h / 2;
        watermark.alpha = opacity;

        this.bgLayer.addChild(watermark);
    }

    /**
     * Planifie un rendu complet via requestAnimationFrame
     * Évite plusieurs rendus par frame
     */
    scheduleRender() {
        if (this.renderScheduled) return;
        this.renderScheduled = true;
        requestAnimationFrame(() => {
            this.renderScheduled = false;
            this.render();
        });
    }

    /**
     * Planifie un rendu d'overlay uniquement via requestAnimationFrame
     * Plus léger que render() complet
     */
    scheduleOverlayRender() {
        if (this.overlayRenderScheduled) return;
        this.overlayRenderScheduled = true;
        requestAnimationFrame(() => {
            this.overlayRenderScheduled = false;
            this.renderOverlayOnly();
        });
    }

    render() {
        if (this.state.data.length === 0) return;

        const w = this.app.screen.width;
        const h = this.app.screen.height;

        // Nettoyer les Graphics réutilisables (au lieu de removeChildren)
        this.wicksGraphics.clear();
        this.bodiesUpFilledGraphics.clear();
        this.bodiesUpHollowGraphics.clear();
        this.bodiesDownGraphics.clear();
        this.bordersGraphics.clear();
        this.realtimeMarkersGraphics.clear();
        this.volumeGraphics.clear();
        this.rsiGraphics.clear();
        this.indicatorBgGraphics.clear();

        // Nettoyer l'overlay Canvas2D
        this.overlayCtx.clearRect(0, 0, w, h);

        const chartX = this.layout.marginLeft;
        const chartY = this.layout.marginTop;
        const chartW = w - this.layout.marginLeft - this.layout.marginRight;

        // Ajuster chartH si indicateurs en mode séparé
        let chartH = h - this.layout.marginTop - this.layout.marginBottom;
        let indicatorH = 0;
        if (chartConfig.get('indicators.enabled') && this.rsiData.size > 0 && !chartConfig.get('indicators.rsi.overlay')) {
            const indicatorHeightPercent = chartConfig.get('indicators.heightPercent') / 100;
            indicatorH = h * indicatorHeightPercent;
            chartH = chartH - indicatorH - 5; // 5px de séparation
        }

        // Filtrer bougies visibles
        const visibleCandles = this.state.data.filter(c =>
            c.time >= this.state.viewStart && c.time <= this.state.viewEnd
        );

        if (visibleCandles.length === 0) {
            this.renderNoData();
            return;
        }

        // Calculer prix min/max de la vue
        const visiblePrices = visibleCandles.flatMap(c => [c.high, c.low]);
        const priceMin = Math.min(...visiblePrices);
        const priceMax = Math.max(...visiblePrices);
        const priceRange = priceMax - priceMin;
        const padding = priceRange * 0.05;

        // Helper: prix → y
        const priceToY = (price) => {
            const ratio = (price - (priceMin - padding)) / (priceRange + 2 * padding);
            return chartY + chartH * (1 - ratio);
        };

        // Helper: temps → x
        const timeToX = (time) => {
            const ratio = (time - this.state.viewStart) / (this.state.viewEnd - this.state.viewStart);
            return chartX + ratio * chartW;
        };

        // Dessiner labels prix
        this.renderPriceAxis(priceMin, priceMax, priceRange, priceToY, chartY, chartH);

        // Volume (si activé)
        let volumeHeight = 0;
        if (chartConfig.get('volume.enabled')) {
            volumeHeight = chartH * (chartConfig.get('volume.heightPercent') / 100);
            this.renderVolume(visibleCandles, chartX, chartY, chartW, chartH, volumeHeight, timeToX);
        }

        // Dessiner bougies avec PixiJS
        const tfSeconds = this.parseTimeframeToSeconds(this.state.currentTimeframe);
        const candleWidthSeconds = (this.state.viewEnd - this.state.viewStart) / chartW;
        const candleWidth = Math.max(1, Math.min(50, tfSeconds / candleWidthSeconds * 0.8));

        const borderWidth = chartConfig.get('candles.borderWidth');
        const wickWidth = chartConfig.get('candles.wickWidth');
        const hollowUp = chartConfig.get('candles.hollowUp');
        const minBodyHeight = chartConfig.get('candles.minBodyHeight');

        // Identifier la bougie temps réel (dernière bougie si elle vient du WS)
        const realtimeCandle = this.realtimeCandles.get(this.state.currentTimeframe);
        const realtimeCandleTime = realtimeCandle?.time || null;

        // Convertir couleurs une seule fois
        const upColor = parseInt(this.theme.upColor.replace('#', ''), 16);
        const downColor = parseInt(this.theme.downColor.replace('#', ''), 16);
        const upBorderColor = parseInt(this.theme.upBorderColor.replace('#', ''), 16);
        const downBorderColor = parseInt(this.theme.downBorderColor.replace('#', ''), 16);

        // Dessiner toutes les bougies en batch (beaucoup plus performant)
        visibleCandles.forEach(candle => {
            const x = timeToX(candle.time);
            const yOpen = priceToY(candle.open);
            const yClose = priceToY(candle.close);
            const yHigh = priceToY(candle.high);
            const yLow = priceToY(candle.low);

            const isUp = candle.close >= candle.open;
            const color = isUp ? upColor : downColor;
            const borderColor = isUp ? upBorderColor : downBorderColor;

            // Marqueur WebSocket (trait gris sous la bougie)
            const isRealtimeCandle = realtimeCandleTime !== null && candle.time === realtimeCandleTime;
            if (isRealtimeCandle) {
                const markerY = yLow + 8;
                this.realtimeMarkersGraphics.lineStyle(2, 0x808080, 0.6);
                this.realtimeMarkersGraphics.moveTo(x - candleWidth / 2, markerY);
                this.realtimeMarkersGraphics.lineTo(x + candleWidth / 2, markerY);
            }

            // Mèche (toutes dans le même Graphics)
            this.wicksGraphics.lineStyle(wickWidth, color, 1);
            this.wicksGraphics.moveTo(x, yHigh);
            this.wicksGraphics.lineTo(x, yLow);

            // Corps
            const bodyTop = Math.min(yOpen, yClose);
            const bodyHeight = Math.max(minBodyHeight, Math.abs(yClose - yOpen));

            // Remplissage (creux si haussier et hollowUp activé)
            if (isUp && hollowUp) {
                // Creux: juste bordure (dans bodiesUpHollowGraphics)
                if (borderWidth > 0) {
                    this.bodiesUpHollowGraphics.lineStyle(borderWidth, borderColor, 1);
                    this.bodiesUpHollowGraphics.drawRect(x - candleWidth / 2, bodyTop, candleWidth, bodyHeight);
                }
            } else if (isUp) {
                // Haussier plein
                this.bodiesUpFilledGraphics.beginFill(color);
                this.bodiesUpFilledGraphics.drawRect(x - candleWidth / 2, bodyTop, candleWidth, bodyHeight);
                this.bodiesUpFilledGraphics.endFill();

                if (borderWidth > 0) {
                    this.bordersGraphics.lineStyle(borderWidth, borderColor, 1);
                    this.bordersGraphics.drawRect(x - candleWidth / 2, bodyTop, candleWidth, bodyHeight);
                }
            } else {
                // Baissier (toujours plein)
                this.bodiesDownGraphics.beginFill(color);
                this.bodiesDownGraphics.drawRect(x - candleWidth / 2, bodyTop, candleWidth, bodyHeight);
                this.bodiesDownGraphics.endFill();

                if (borderWidth > 0) {
                    this.bordersGraphics.lineStyle(borderWidth, borderColor, 1);
                    this.bordersGraphics.drawRect(x - candleWidth / 2, bodyTop, candleWidth, bodyHeight);
                }
            }
        });

        // Appliquer les strokes après avoir dessiné toutes les lignes (batching)
        this.wicksGraphics.stroke();
        this.realtimeMarkersGraphics.stroke();
        this.bodiesUpHollowGraphics.stroke();
        this.bordersGraphics.stroke();

        // Indicateurs
        if (chartConfig.get('indicators.enabled') && this.rsiData.size > 0) {
            console.log(`📊 Rendering ${this.rsiData.size} RSI timeframes`);
            const overlay = chartConfig.get('indicators.rsi.overlay');
            if (overlay) {
                // Ajuster chartH pour exclure le volume
                const candleChartH = chartH - volumeHeight;
                this.renderIndicatorsOverlay(chartX, chartY, chartW, candleChartH, timeToX, priceMin, priceMax, priceRange);
            } else {
                this.renderIndicatorsSeparate(chartX, chartY, chartW, chartH, indicatorH, timeToX);
            }
        }

        // Calculer indicatorY pour le panneau RSI séparé
        const indicatorY = chartY + chartH + 5;

        // Sauvegarder les paramètres pour redessiner l'overlay
        this.overlayParams = {
            w, h,
            priceMin, priceMax, priceRange, priceToY,
            chartX, chartY, chartW, chartH,
            visibleCandles,
            volumeHeight,
            indicatorH,
            indicatorY,
            timeToX
        };

        // Dessiner éléments statiques sur overlay
        this.renderStaticOverlay();

        // Render crosshair dynamique si nécessaire
        if (this.state.showCrosshair) {
            this.renderDynamicOverlay();
        }
    }

    renderVolume(candles, chartX, chartY, chartW, chartH, volumeHeight, timeToX) {
        if (candles.length === 0) return;

        const maxVolume = Math.max(...candles.map(c => c.volume));

        // Convertir couleurs une seule fois
        const volUpColorStr = chartConfig.get('colors.volumeUpColor');
        const volDownColorStr = chartConfig.get('colors.volumeDownColor');

        const matchUp = volUpColorStr.match(/^#([0-9a-f]{6})([0-9a-f]{2})?$/i);
        const volUpColor = matchUp ? parseInt(matchUp[1], 16) : 0x26a69a;
        const volUpAlpha = matchUp && matchUp[2] ? parseInt(matchUp[2], 16) / 255 : 0.5;

        const matchDown = volDownColorStr.match(/^#([0-9a-f]{6})([0-9a-f]{2})?$/i);
        const volDownColor = matchDown ? parseInt(matchDown[1], 16) : 0xef5350;
        const volDownAlpha = matchDown && matchDown[2] ? parseInt(matchDown[2], 16) / 255 : 0.5;

        const barWidth = Math.max(1, chartW / candles.length * 0.8);

        // Dessiner toutes les barres de volume dans le même Graphics
        candles.forEach(candle => {
            const x = timeToX(candle.time);
            const height = (candle.volume / maxVolume) * volumeHeight * 0.95;
            const y = chartY + chartH - height;

            const isUp = candle.close >= candle.open;
            const color = isUp ? volUpColor : volDownColor;
            const alpha = isUp ? volUpAlpha : volDownAlpha;

            this.volumeGraphics.beginFill(color, alpha);
            this.volumeGraphics.drawRect(x - barWidth / 2, y, barWidth, height);
            this.volumeGraphics.endFill();
        });
    }

    renderIndicatorsOverlay(chartX, chartY, chartW, chartH, timeToX, priceMin, priceMax, priceRange) {
        if (this.rsiData.size === 0) {
            console.log('📊 No RSI data to render (overlay mode)');
            return;
        }
        console.log(`📊 Rendering RSI (overlay mode) for ${this.rsiData.size} timeframes`);

        // RSI: 0-100, on le superpose avec échelle à droite
        const rsiToY = (value) => {
            const ratio = value / 100;
            return chartY + chartH * (1 - ratio);
        };

        // Utiliser le Graphics réutilisable (déjà cleared au début de render())
        this.rsiData.forEach((data, tf) => {
            if (!this.rsiVisibility.get(tf)) return;
            const colorStr = this.getRSIColorForTimeframe(tf);
            const color = parseInt(colorStr.replace('#', ''), 16);

            this.rsiGraphics.lineStyle(1.5, color, 1);

            let first = true;
            data.forEach(point => {
                if (point.time >= this.state.viewStart && point.time <= this.state.viewEnd) {
                    const x = timeToX(point.time);
                    const y = rsiToY(point.value);
                    if (first) {
                        this.rsiGraphics.moveTo(x, y);
                        first = false;
                    } else {
                        this.rsiGraphics.lineTo(x, y);
                    }
                }
            });
            this.rsiGraphics.stroke();
        });

        // Note: Les échelles RSI sont dessinées dans renderStaticOverlay()
    }

    renderIndicatorsSeparate(chartX, chartY, chartW, chartH, indicatorH, timeToX) {
        if (this.rsiData.size === 0) {
            console.log('📊 No RSI data to render (separate mode)');
            return;
        }
        console.log(`📊 Rendering RSI (separate mode) for ${this.rsiData.size} timeframes`);

        // Positionner le panneau RSI juste en dessous de la zone des prix
        const indicatorY = chartY + chartH + 5;

        // Fond (utiliser indicatorBgGraphics)
        const bgColor = this.theme.bg === '#ffffff' ? 0xf9f9f9 : 0x252525;
        this.indicatorBgGraphics.beginFill(bgColor);
        this.indicatorBgGraphics.drawRect(chartX, indicatorY, chartW, indicatorH);
        this.indicatorBgGraphics.endFill();

        // Grille horizontale (lignes de référence RSI 30, 50, 70) - en pointillés
        // Note: Les lignes sont dessinées dans renderStaticOverlay avec le canvas pour supporter les pointillés

        // Courbes RSI (utiliser rsiGraphics)
        const rsiToY = (value) => indicatorY + indicatorH * (1 - value / 100);

        this.rsiData.forEach((data, tf) => {
            if (!this.rsiVisibility.get(tf)) return;
            const colorStr = this.getRSIColorForTimeframe(tf);
            const color = parseInt(colorStr.replace('#', ''), 16);

            this.rsiGraphics.lineStyle(1.5, color, 1);

            let first = true;
            data.forEach(point => {
                if (point.time >= this.state.viewStart && point.time <= this.state.viewEnd) {
                    const x = timeToX(point.time);
                    const y = rsiToY(point.value);
                    if (first) {
                        this.rsiGraphics.moveTo(x, y);
                        first = false;
                    } else {
                        this.rsiGraphics.lineTo(x, y);
                    }
                }
            });
            this.rsiGraphics.stroke();
        });

        // Note: Les échelles RSI sont dessinées dans renderStaticOverlay()
    }

    renderLastPriceLine(candles, chartX, chartW, priceToY) {
        const lastCandle = candles[candles.length - 1];
        if (!lastCandle) return;

        const y = priceToY(lastCandle.close);
        const isUp = lastCandle.close >= lastCandle.open;
        const color = isUp ? this.theme.upColor : this.theme.downColor;

        this.overlayCtx.save();
        this.overlayCtx.strokeStyle = color;
        this.overlayCtx.lineWidth = 1;

        const lineStyle = chartConfig.get('lastPrice.lineStyle');
        if (lineStyle === 'dashed') {
            this.overlayCtx.setLineDash([4, 4]);
        } else if (lineStyle === 'dotted') {
            this.overlayCtx.setLineDash([2, 2]);
        }

        this.overlayCtx.beginPath();
        this.overlayCtx.moveTo(chartX, y);
        this.overlayCtx.lineTo(chartX + chartW, y);
        this.overlayCtx.stroke();

        // Label
        if (chartConfig.get('lastPrice.labelBg')) {
            const text = lastCandle.close.toFixed(2);
            this.overlayCtx.font = '11px monospace';
            const textWidth = this.overlayCtx.measureText(text).width;

            this.overlayCtx.fillStyle = color;
            this.overlayCtx.fillRect(chartX + chartW + 5, y - 10, textWidth + 8, 20);

            this.overlayCtx.fillStyle = '#ffffff';
            this.overlayCtx.textAlign = 'left';
            this.overlayCtx.textBaseline = 'middle';
            this.overlayCtx.fillText(text, chartX + chartW + 9, y);
        }

        this.overlayCtx.restore();
    }

    formatPrice(price, priceStep) {
        // Déterminer le nombre de décimales basé sur le step
        let decimals;

        if (priceStep >= 1000) {
            decimals = 0;
        } else if (priceStep >= 100) {
            decimals = 0;
        } else if (priceStep >= 10) {
            decimals = 0;
        } else if (priceStep >= 1) {
            decimals = 1;
        } else if (priceStep >= 0.1) {
            decimals = 2;
        } else if (priceStep >= 0.01) {
            decimals = 3;
        } else if (priceStep >= 0.001) {
            decimals = 4;
        } else {
            decimals = 6;
        }

        return price.toFixed(decimals);
    }

    renderPriceAxis(priceMin, priceMax, priceRange, priceToY, chartY, chartH) {
        this.overlayCtx.save();
        this.overlayCtx.fillStyle = this.theme.text;
        this.overlayCtx.font = '11px monospace';
        this.overlayCtx.textAlign = 'right';
        this.overlayCtx.textBaseline = 'middle';

        const priceStep = this.calculatePriceStep(priceRange);
        const start = Math.floor(priceMin / priceStep) * priceStep;

        let count = 0;
        for (let price = start; price <= priceMax; price += priceStep) {
            const y = priceToY(price);
            if (y < chartY || y > chartY + chartH) continue;

            this.overlayCtx.fillText(this.formatPrice(price, priceStep), this.layout.marginLeft - 8, y);
            count++;
        }
        console.log(`📊 Price axis: rendered ${count} labels`);
        this.overlayCtx.restore();
    }

    renderTimeAxis(w, h) {
        this.overlayCtx.save();
        const chartX = this.layout.marginLeft;
        const chartW = w - this.layout.marginLeft - this.layout.marginRight;
        const viewDuration = this.state.viewEnd - this.state.viewStart;
        const viewDurationDays = viewDuration / 86400;

        // Déterminer format selon durée
        let formatFn;
        let estimatedLabelWidth;

        if (viewDurationDays < 1) {
            // Moins d'un jour: HH:MM
            formatFn = (ts) => new Date(ts * 1000).toISOString().substring(11, 16);
            estimatedLabelWidth = 40;
        } else if (viewDurationDays < 30) {
            // Moins d'un mois: MM-DD HH:MM
            formatFn = (ts) => {
                const date = new Date(ts * 1000);
                const iso = date.toISOString();
                return `${iso.substring(5, 10)} ${iso.substring(11, 16)}`;
            };
            estimatedLabelWidth = 80;
        } else if (viewDurationDays < 365) {
            // Moins d'un an: MM-DD
            formatFn = (ts) => new Date(ts * 1000).toISOString().substring(5, 10);
            estimatedLabelWidth = 45;
        } else {
            // Plus d'un an: YYYY-MM-DD
            formatFn = (ts) => new Date(ts * 1000).toISOString().substring(0, 10);
            estimatedLabelWidth = 70;
        }

        // Calculer nombre de labels possibles sans chevauchement
        const minSpacing = estimatedLabelWidth + 10; // Espace minimum entre labels
        const maxLabels = Math.floor(chartW / minSpacing);
        const numLabels = Math.max(3, Math.min(maxLabels, 12)); // Entre 3 et 12 labels

        this.overlayCtx.fillStyle = this.theme.text;
        this.overlayCtx.font = '11px sans-serif';
        this.overlayCtx.textAlign = 'center';
        this.overlayCtx.textBaseline = 'top';

        const y = h - this.layout.marginBottom + 10;

        // Générer les timestamps à intervalles réguliers
        for (let i = 0; i <= numLabels; i++) {
            const ratio = i / numLabels;
            const timestamp = this.state.viewStart + ratio * viewDuration;
            const x = chartX + ratio * chartW;

            // Dessiner tick vertical
            this.overlayCtx.strokeStyle = this.theme.grid;
            this.overlayCtx.globalAlpha = 0.3;
            this.overlayCtx.lineWidth = 1;
            this.overlayCtx.beginPath();
            this.overlayCtx.moveTo(x, y - 5);
            this.overlayCtx.lineTo(x, y);
            this.overlayCtx.stroke();
            this.overlayCtx.globalAlpha = 1;

            // Dessiner label
            this.overlayCtx.fillText(formatFn(timestamp), x, y + 2);
        }
        this.overlayCtx.restore();
    }

    renderStaticOverlay() {
        if (!this.overlayParams) return;

        const {
            w,
            h,
            priceMin,
            priceMax,
            priceRange,
            priceToY,
            chartX,
            chartY,
            chartW,
            chartH,
            visibleCandles,
            volumeHeight,
            indicatorH,
            indicatorY,
            timeToX
        } = this.overlayParams;

        // Dessiner axe prix
        this.renderPriceAxis(priceMin, priceMax, priceRange, priceToY, chartY, chartH);

        // Dessiner axe temps
        this.renderTimeAxis(w, h);

        // Ligne dernier prix
        if (chartConfig.get('lastPrice.enabled') && visibleCandles.length > 0) {
            this.renderLastPriceLine(visibleCandles, chartX, chartW, priceToY);
        }

        // Échelles RSI
        if (chartConfig.get('indicators.enabled') && this.rsiData.size > 0) {
            const overlay = chartConfig.get('indicators.rsi.overlay');
            if (overlay) {
                // Mode overlay: échelle à droite
                const candleChartH = chartH - volumeHeight;
                this.renderRSIScaleOverlay(chartX, chartY, chartW, candleChartH, priceMin, priceMax, priceRange);
            } else {
                // Mode séparé: échelle à gauche avec lignes de référence
                this.renderRSIScaleSeparate(chartX, indicatorY, indicatorH);
            }
        }
    }

    renderRSIScaleOverlay(chartX, chartY, chartW, chartH, priceMin, priceMax, priceRange) {
        const padding = priceRange * 0.05;
        const rsiToY = (value) => {
            const ratio = value / 100;
            return chartY + chartH * (1 - ratio);
        };

        this.overlayCtx.save();

        // Dessiner les lignes de référence 30, 50, 70 en pointillés discrets (mode overlay)
        this.overlayCtx.strokeStyle = '#666666'; // Gris moyen
        this.overlayCtx.lineWidth = 1; // Fin
        this.overlayCtx.globalAlpha = 0.25; // Plus transparent en overlay
        this.overlayCtx.setLineDash([4, 4]); // Pointillés

        [30, 50, 70].forEach(level => {
            const y = rsiToY(level);
            this.overlayCtx.beginPath();
            this.overlayCtx.moveTo(chartX, y);
            this.overlayCtx.lineTo(chartX + chartW, y);
            this.overlayCtx.stroke();
        });

        this.overlayCtx.setLineDash([]); // Reset
        this.overlayCtx.globalAlpha = 1.0;

        // Dessiner les labels
        this.overlayCtx.fillStyle = this.theme.textLight;
        this.overlayCtx.font = '10px monospace';
        this.overlayCtx.textAlign = 'left';
        [30, 50, 70].forEach(level => {
            const y = rsiToY(level);
            this.overlayCtx.fillText(`RSI ${level}`, chartX + chartW + 5, y + 3);
        });

        this.overlayCtx.restore();
    }

    renderRSIScaleSeparate(chartX, indicatorY, indicatorH) {
        const chartW = this.app.screen.width - this.layout.marginLeft - this.layout.marginRight;

        this.overlayCtx.save();

        // Dessiner les lignes de référence 30, 50, 70 en pointillés discrets
        this.overlayCtx.strokeStyle = '#666666'; // Gris moyen
        this.overlayCtx.lineWidth = 1; // Fin
        this.overlayCtx.globalAlpha = 0.3; // Transparent
        this.overlayCtx.setLineDash([4, 4]); // Pointillés

        [30, 50, 70].forEach(level => {
            const y = indicatorY + indicatorH * (1 - level / 100);
            this.overlayCtx.beginPath();
            this.overlayCtx.moveTo(chartX, y);
            this.overlayCtx.lineTo(chartX + chartW, y);
            this.overlayCtx.stroke();
        });

        this.overlayCtx.setLineDash([]); // Reset
        this.overlayCtx.globalAlpha = 1.0;

        // Dessiner les labels
        this.overlayCtx.fillStyle = this.theme.text;
        this.overlayCtx.font = '10px monospace';
        this.overlayCtx.textAlign = 'right';
        this.overlayCtx.textBaseline = 'middle';
        [0, 30, 50, 70, 100].forEach(level => {
            const y = indicatorY + indicatorH * (1 - level / 100);
            this.overlayCtx.fillText(level.toString(), chartX - 5, y);
        });

        this.overlayCtx.restore();
    }

    renderDynamicOverlay() {
        if (!this.state.showCrosshair) return;

        const w = this.app.screen.width;
        const h = this.app.screen.height;
        const chartX = this.layout.marginLeft;
        const chartY = this.layout.marginTop;
        const chartW = w - this.layout.marginLeft - this.layout.marginRight;
        const chartH = h - this.layout.marginTop - this.layout.marginBottom;

        // Crosshair config
        const crosshairStyle = chartConfig.get('crosshair.style');
        const crosshairWidth = chartConfig.get('crosshair.width');

        let dashArray = [];
        if (crosshairStyle === 'dashed') dashArray = [4, 4];
        else if (crosshairStyle === 'dotted') dashArray = [2, 2];

        // Crosshair lines
        this.overlayCtx.strokeStyle = this.theme.crosshair;
        this.overlayCtx.lineWidth = crosshairWidth;
        this.overlayCtx.setLineDash(dashArray);

        // Vertical
        this.overlayCtx.beginPath();
        this.overlayCtx.moveTo(this.state.mouseX, chartY);
        this.overlayCtx.lineTo(this.state.mouseX, chartY + chartH);
        this.overlayCtx.stroke();

        // Horizontal
        this.overlayCtx.beginPath();
        this.overlayCtx.moveTo(chartX, this.state.mouseY);
        this.overlayCtx.lineTo(chartX + chartW, this.state.mouseY);
        this.overlayCtx.stroke();

        this.overlayCtx.setLineDash([]);

        // Floating labels
        if (chartConfig.get('crosshair.floatingLabels')) {
            this.renderFloatingLabels(chartX, chartY, chartW, chartH);
        }

        // Tooltip si bougie trouvée
        if (this.state.crosshairCandle) {
            this.renderTooltip(this.state.crosshairCandle);
        }
    }

    renderOverlayOnly() {
        // Nettoyer l'overlay canvas
        const w = this.app.screen.width;
        const h = this.app.screen.height;
        this.overlayCtx.clearRect(0, 0, w, h);

        // Redessiner les éléments statiques
        this.renderStaticOverlay();

        // Redessiner le crosshair dynamique
        this.renderDynamicOverlay();
    }

    renderFloatingLabels(chartX, chartY, chartW, chartH) {
        // Label prix (Y)
        if (this.state.data.length > 0) {
            // Utiliser la plage de prix visible
            const visibleCandles = this.state.data.filter(c =>
                c.time >= this.state.viewStart && c.time <= this.state.viewEnd
            );

            if (visibleCandles.length > 0) {
                const visiblePrices = visibleCandles.flatMap(c => [c.high, c.low]);
                const priceMin = Math.min(...visiblePrices);
                const priceMax = Math.max(...visiblePrices);
                const priceRange = priceMax - priceMin;
                const padding = priceRange * 0.05;

                const mouseYRatio = (this.state.mouseY - chartY) / chartH;
                const price = (priceMax + padding) - mouseYRatio * (priceRange + 2 * padding);

                this.overlayCtx.fillStyle = this.theme.crosshair;
                this.overlayCtx.fillRect(chartX - 60, this.state.mouseY - 10, 55, 20);

                this.overlayCtx.fillStyle = '#ffffff';
                this.overlayCtx.font = '11px monospace';
                this.overlayCtx.textAlign = 'right';
                this.overlayCtx.textBaseline = 'middle';
                this.overlayCtx.fillText(price.toFixed(2), chartX - 8, this.state.mouseY);
            }
        }

        // Label temps (X)
        const mouseXRatio = (this.state.mouseX - chartX) / chartW;
        const timestamp = this.state.viewStart + mouseXRatio * (this.state.viewEnd - this.state.viewStart);
        const timeStr = new Date(timestamp * 1000).toISOString().substring(11, 16);

        this.overlayCtx.fillStyle = this.theme.crosshair;
        this.overlayCtx.fillRect(this.state.mouseX - 25, chartY + chartH + 5, 50, 20);

        this.overlayCtx.fillStyle = '#ffffff';
        this.overlayCtx.font = '11px sans-serif';
        this.overlayCtx.textAlign = 'center';
        this.overlayCtx.textBaseline = 'top';
        this.overlayCtx.fillText(timeStr, this.state.mouseX, chartY + chartH + 10);
    }

    renderTooltip(candle) {
        const padding = 8;
        const lineHeight = 16;

        // Formater la date avec timezone
        const date = new Date(candle.time * 1000);
        const utcStr = date.toISOString().substring(0, 19).replace('T', ' ') + ' UTC';

        // Conversion en Europe/Paris
        const parisStr = date.toLocaleString('fr-FR', {
            timeZone: 'Europe/Paris',
            year: 'numeric',
            month: '2-digit',
            day: '2-digit',
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit'
        }) + ' CET/CEST';

        const lines = [
            `Time: ${utcStr}`,
            `      ${parisStr}`,
            `Open:  ${candle.open.toFixed(2)}`,
            `High:  ${candle.high.toFixed(2)}`,
            `Low:   ${candle.low.toFixed(2)}`,
            `Close: ${candle.close.toFixed(2)}`,
            `Vol:   ${candle.volume.toFixed(2)}`
        ];

        const maxWidth = Math.max(...lines.map(l => this.overlayCtx.measureText(l).width));
        const tooltipW = maxWidth + padding * 2;
        const tooltipH = lines.length * lineHeight + padding * 2;

        let x = this.state.mouseX + 15;
        let y = this.state.mouseY + 15;

        const rect = this.container.getBoundingClientRect();
        if (x + tooltipW > rect.width) x = this.state.mouseX - tooltipW - 15;
        if (y + tooltipH > rect.height) y = this.state.mouseY - tooltipH - 15;

        // Fond
        this.overlayCtx.fillStyle = this.theme.tooltipBg;
        this.overlayCtx.strokeStyle = this.theme.tooltipBorder;
        this.overlayCtx.lineWidth = 1;
        this.overlayCtx.fillRect(x, y, tooltipW, tooltipH);
        this.overlayCtx.strokeRect(x, y, tooltipW, tooltipH);

        // Texte
        this.overlayCtx.fillStyle = this.theme.text;
        this.overlayCtx.font = '11px monospace';
        this.overlayCtx.textAlign = 'left';
        this.overlayCtx.textBaseline = 'top';

        lines.forEach((line, i) => {
            this.overlayCtx.fillText(line, x + padding, y + padding + i * lineHeight);
        });
    }

    renderLoading() {
        const rect = this.container.getBoundingClientRect();
        this.mainCtx.clearRect(0, 0, rect.width, rect.height);

        this.mainCtx.fillStyle = this.theme.textLight;
        this.mainCtx.font = '14px sans-serif';
        this.mainCtx.textAlign = 'center';
        this.mainCtx.textBaseline = 'middle';
        this.mainCtx.fillText('Loading...', rect.width / 2, rect.height / 2);
    }

    renderNoData() {
        const rect = this.container.getBoundingClientRect();

        this.mainCtx.fillStyle = this.theme.textLight;
        this.mainCtx.font = '14px sans-serif';
        this.mainCtx.textAlign = 'center';
        this.mainCtx.textBaseline = 'middle';

        const message = this.state.isLoading ? 'Loading new timeframe...' : 'Zoom out to see data';
        this.mainCtx.fillText(message, rect.width / 2, rect.height / 2);

        this.renderTimeAxis(rect.width, rect.height);
    }

    renderError(message) {
        const rect = this.container.getBoundingClientRect();
        this.mainCtx.clearRect(0, 0, rect.width, rect.height);

        this.mainCtx.fillStyle = this.theme.downColor;
        this.mainCtx.font = '14px sans-serif';
        this.mainCtx.textAlign = 'center';
        this.mainCtx.textBaseline = 'middle';
        this.mainCtx.fillText(`Error: ${message}`, rect.width / 2, rect.height / 2);
    }

    calculatePriceStep(range) {
        const steps = [0.01, 0.05, 0.1, 0.5, 1, 5, 10, 50, 100, 500, 1000, 5000, 10000];
        const targetLines = 8;
        for (const step of steps) {
            if (range / step <= targetLines) return step;
        }
        return steps[steps.length - 1];
    }

    destroy() {
        // Cleanup
        this.stopRealtimeUpdates();
        this.app.destroy(true, {children: true, texture: true});
        this.overlayCanvas.remove();
        this.legendContainer.remove();
    }
}
