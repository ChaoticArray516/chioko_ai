import { useEffect, useRef, useCallback, useState } from 'react';
import { Application, Ticker } from 'pixi.js';
import { Live2DModel } from 'pixi-live2d-display/cubism4';
import { ExpressionController } from '../services/ExpressionController';

declare global {
  interface Window {
    Live2DCubismCore: unknown;
    __LIVE2D_DEBUG__: { app: unknown; model: unknown; internalModel: unknown };
  }
}

interface Live2DInternalModel {
  coreModel?: { setParameterValueById?: (id: string, value: number) => void };
}

interface Live2DCanvasProps {
  modelPath: string;
  motionId?: string;
  expressionId?: string;
  onHit?: (hitAreas: string[]) => void;
  width?: number;
  height?: number;
  autoFit?: boolean;
}

interface MotionAnimation {
  breathAmplitude: number;
  bodySwayX: number;
  bodySwayZ: number;
  headSwayX: number;
  headSwayY: number;
  speedMultiplier: number;
}

const MOTION_ANIMATIONS: Record<string, MotionAnimation> = {
  idle: { breathAmplitude: 0.3, bodySwayX: 1.5, bodySwayZ: 0.8, headSwayX: 3.0, headSwayY: 1.5, speedMultiplier: 0.6 },
  idle2: { breathAmplitude: 0.5, bodySwayX: 3.0, bodySwayZ: 1.5, headSwayX: 6.0, headSwayY: 3.0, speedMultiplier: 1.0 },
  lazy_stretch: { breathAmplitude: 0.4, bodySwayX: 4.0, bodySwayZ: 2.0, headSwayX: 5.0, headSwayY: 2.5, speedMultiplier: 0.4 },
  alert: { breathAmplitude: 0.6, bodySwayX: 2.0, bodySwayZ: 1.0, headSwayX: 4.0, headSwayY: 2.0, speedMultiplier: 1.8 },
  shy_fidget: { breathAmplitude: 0.35, bodySwayX: 1.0, bodySwayZ: 0.5, headSwayX: 2.0, headSwayY: 1.0, speedMultiplier: 1.2 },
  hangry_sway: { breathAmplitude: 0.45, bodySwayX: 3.5, bodySwayZ: 2.5, headSwayX: 4.5, headSwayY: 3.0, speedMultiplier: 1.4 },
};

export function Live2DCanvas({
  modelPath, motionId = 'idle', expressionId = 'lazy', onHit, width = 800, height = 600, autoFit = false,
}: Live2DCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const appRef = useRef<Application | null>(null);
  const modelRef = useRef<Live2DModel | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [containerSize, setContainerSize] = useState({ width, height });
  const animationTickerRef = useRef<((delta: number) => void) | null>(null);
  const expressionControllerRef = useRef<ExpressionController | null>(null);

  const calcAutoFitScale = useCallback((containerWidth: number, containerHeight: number) => {
    const baseWidth = 1200; const baseHeight = 1600;
    const scale = Math.min((containerWidth * 0.4) / baseWidth, (containerHeight * 0.4) / baseHeight);
    return Math.max(0.08, Math.min(0.15, scale));
  }, []);

  useEffect(() => {
    if (!autoFit || !containerRef.current) return;
    const resizeObserver = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) {
        const { width: w, height: h } = entry.contentRect;
        setContainerSize({ width: w, height: h });
        if (appRef.current && modelRef.current) {
          appRef.current.renderer.resize(w, h);
          const model = modelRef.current;
          model.x = w / 2;
          model.y = h * 0.6;
          model.scale.set(calcAutoFitScale(w, h));
        }
      }
    });
    resizeObserver.observe(containerRef.current);
    return () => { resizeObserver.disconnect(); };
  }, [autoFit, calcAutoFitScale]);

  const initLive2D = useCallback(async () => {
    if (!canvasRef.current || appRef.current) return;
    let cw = width; let ch = height;
    if (autoFit && containerRef.current) {
      const rect = containerRef.current.getBoundingClientRect();
      cw = Math.floor(rect.width); ch = Math.floor(rect.height);
      if (cw === 0 || ch === 0) return;
    }
    try {
      setLoading(true);
      if (!window.Live2DCubismCore) throw new Error('Live2D Cubism Core not loaded');
      const app = new Application({ view: canvasRef.current, width: cw, height: ch, backgroundAlpha: 0, antialias: true, resolution: 1, autoDensity: false });
      appRef.current = app;
      app.start();
      try { Live2DModel.registerTicker(Ticker); } catch { /* already registered */ }
      const model = await Live2DModel.from(modelPath);
      modelRef.current = model;
      model.alpha = 0;
      app.stage.addChild(model);
      const scale = autoFit ? calcAutoFitScale(cw, ch) : 0.12;
      model.x = cw / 2;
      model.y = ch * 0.6;
      model.scale.set(scale);
      model.anchor.set(0.5, 0.5);

      let fadeAlpha = 0;
      const fadeTicker = (delta: number) => {
        fadeAlpha += delta * 0.1;
        if (fadeAlpha >= 1) { fadeAlpha = 1; model.alpha = 1; app.ticker.remove(fadeTicker); }
        else { model.alpha = fadeAlpha; }
      };
      app.ticker.add(fadeTicker);

      setLoading(false);
      window.__LIVE2D_DEBUG__ = { app, model, internalModel: model.internalModel };

      if (expressionControllerRef.current) {
        expressionControllerRef.current.registerCallbacks({
          applyNativeExpression: (name: string | null) => {
            if (!modelRef.current) return;
            modelRef.current.expression(name === null ? undefined as unknown as string : name);
          },
          setParameter: (id: string, value: number) => {
            if (!modelRef.current) return;
            (modelRef.current.internalModel as unknown as Live2DInternalModel).coreModel?.setParameterValueById?.(id, value);
          },
        });
      }

      const canvasEl = canvasRef.current;
      canvasEl.addEventListener('pointermove', (e: PointerEvent) => {
        if (!modelRef.current || !canvasRef.current) return;
        const rect = canvasRef.current.getBoundingClientRect();
        modelRef.current.focus(e.clientX - rect.left, e.clientY - rect.top);
      });
      canvasEl.addEventListener('pointerdown', (e: PointerEvent) => {
        if (!modelRef.current || !canvasRef.current) return;
        const rect = canvasRef.current.getBoundingClientRect();
        modelRef.current.tap(e.clientX - rect.left, e.clientY - rect.top);
      });
      model.on('hit', (hitAreas: string[]) => { onHit?.(hitAreas); });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
      setLoading(false);
    }
  }, [modelPath, width, height, expressionId, autoFit, calcAutoFitScale]);

  useEffect(() => { expressionControllerRef.current = new ExpressionController(); return () => { expressionControllerRef.current = null; }; }, []);

  useEffect(() => {
    if (!autoFit) return;
    if (containerSize.width > 0 && containerSize.height > 0 && !appRef.current) initLive2D();
  }, [autoFit, containerSize, initLive2D]);

  useEffect(() => {
    expressionControllerRef.current?.applyExpression(expressionId);
  }, [expressionId]);

  useEffect(() => {
    if (!modelRef.current || !appRef.current) return;
    const app = appRef.current;
    if (animationTickerRef.current) { app.ticker.remove(animationTickerRef.current); animationTickerRef.current = null; }
    const config = MOTION_ANIMATIONS[motionId] ?? MOTION_ANIMATIONS['idle'];
    const startTime = performance.now();
    const ticker = () => {
      const currentModel = modelRef.current;
      if (!currentModel?.internalModel) return;
      const coreModel = (currentModel.internalModel as unknown as Live2DInternalModel).coreModel;
      if (!coreModel?.setParameterValueById) return;
      const t = (performance.now() - startTime) / 1000;
      const s = config.speedMultiplier;
      coreModel.setParameterValueById('ParamBreath', (Math.sin(t * s * Math.PI * 0.5) + 1) / 2 * config.breathAmplitude);
      coreModel.setParameterValueById('ParamBodyAngleX', Math.sin(t * s * Math.PI * 0.33 + 0.5) * config.bodySwayX);
      coreModel.setParameterValueById('ParamBodyAngleZ', Math.sin(t * s * Math.PI * 0.25 + 1.0) * config.bodySwayZ);
      coreModel.setParameterValueById('ParamAngleX', Math.sin(t * s * Math.PI * 0.4 + 0.8) * config.headSwayX);
      coreModel.setParameterValueById('ParamAngleY', Math.sin(t * s * Math.PI * 0.286 + 1.5) * config.headSwayY);
      coreModel.setParameterValueById('ParamEyeBallX', Math.sin(t * s * Math.PI * 0.4 + 0.8) * 0.3);
      coreModel.setParameterValueById('ParamEyeBallY', Math.sin(t * s * Math.PI * 0.286 + 1.5) * 0.2);
      expressionControllerRef.current?.update();
    };
    animationTickerRef.current = ticker;
    app.ticker.add(ticker);
    return () => {
      if (animationTickerRef.current && appRef.current) { appRef.current.ticker.remove(animationTickerRef.current); animationTickerRef.current = null; }
    };
  }, [motionId]);

  useEffect(() => {
    if (autoFit) return;
    const timer = setTimeout(() => { initLive2D(); }, 100);
    return () => { clearTimeout(timer); appRef.current?.destroy(true); };
  }, [initLive2D, autoFit]);

  return (
    <div ref={containerRef} style={{ position: 'relative', width: autoFit ? '100%' : width, height: autoFit ? '100%' : height }}>
      <canvas ref={canvasRef} width={autoFit ? containerSize.width : width} height={autoFit ? containerSize.height : height}
        style={{ width: autoFit ? '100%' : width, height: autoFit ? '100%' : height, borderRadius: '8px', display: 'block' }} />
      {loading && (
        <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center',
          backgroundColor: 'rgba(0,0,0,0.5)', borderRadius: '8px', color: 'white', fontSize: '16px' }}>
          Loading Live2D Model...
        </div>
      )}
      {error && (
        <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center',
          backgroundColor: 'rgba(200,0,0,0.8)', borderRadius: '8px', color: 'white', fontSize: '14px', padding: '20px', textAlign: 'center' }}>
          <div><div style={{ fontWeight: 'bold', marginBottom: '8px' }}>Error Loading Model</div><div>{error}</div></div>
        </div>
      )}
    </div>
  );
}

export default Live2DCanvas;
