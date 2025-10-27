#!/bin/bash
# Script pour ajouter les types TypeScript aux méthodes

sed -i '1i\import { Candle, ChartState, ThemeColors, ChartLayout, ChartCallbacks, ChartOptions, SavedRange } from '\''./types.js'\'';\nimport { chartConfig } from '\''./config.js'\'';' chart-engine.ts

# Ajouter export à la classe
sed -i 's/^class ChartEngine {/export class ChartEngine {/' chart-engine.ts

# Remplacer ! pour les getContext
sed -i "s/getContext('2d')/getContext('2d')!/g" chart-engine.ts

# Ajouter types aux méthodes principales
sed -i 's/setupCanvasLayers() {/setupCanvasLayers(): void {/' chart-engine.ts
sed -i 's/parseTimeframeToSeconds(tf) {/parseTimeframeToSeconds(tf: string): number {/' chart-engine.ts
sed -i 's/setTimeframes(timeframes) {/setTimeframes(timeframes: string[]): void {/' chart-engine.ts
sed -i 's/updateTheme() {/updateTheme(): void {/' chart-engine.ts
sed -i 's/setupCanvas() {/setupCanvas(): void {/' chart-engine.ts
sed -i 's/setupEvents() {/setupEvents(): void {/' chart-engine.ts
sed -i 's/handleResize() {/handleResize(): void {/' chart-engine.ts
sed -i 's/renderBackground() {/renderBackground(): void {/' chart-engine.ts
sed -i 's/render() {/render(): void {/' chart-engine.ts
sed -i 's/fitToData() {/fitToData(): void {/' chart-engine.ts
sed -i 's/renderOverlay() {/renderOverlay(): void {/' chart-engine.ts
sed -i 's/renderLoading() {/renderLoading(): void {/' chart-engine.ts
sed -i 's/destroy() {/destroy(): void {/' chart-engine.ts

# Ajouter type au constructor
sed -i 's/constructor(container, options = {})/constructor(container: HTMLElement, options: ChartOptions = {})/' chart-engine.ts

# Cast l'objet state
sed -i 's/lastZoomTime: 0$/lastZoomTime: 0\n        } as ChartState;/' chart-engine.ts | head -1
