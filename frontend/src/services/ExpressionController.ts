export type EllenEmotion = 'lazy' | 'maid' | 'predator' | 'hangry' | 'shy' | 'surprised' | 'happy';

type NativeExpression = 'angry' | 'blush' | 'stunned' | 'sweat' | 'sunglasses' | 'mouth_left' | 'mouth_right' | null;

interface FacialParams {
  ParamEyeLOpen?: number;
  ParamEyeROpen?: number;
  ParamBrowLY?: number;
  ParamBrowRY?: number;
  ParamBrowLAngle?: number;
  ParamBrowRAngle?: number;
  ParamMouthForm?: number;
  ParamMouthOpenY?: number;
  ParamAngleZ?: number;
  ParamAngleX?: number;
  ParamAngleY?: number;
}

interface EmotionConfig {
  nativeExpression: NativeExpression;
  facialParams: FacialParams;
  duration: number;
  description: string;
}

const EMOTION_CONFIGS: Record<EllenEmotion, EmotionConfig> = {
  lazy: {
    nativeExpression: null,
    facialParams: { ParamEyeLOpen: 0.45, ParamEyeROpen: 0.45, ParamBrowLY: 0.0, ParamBrowRY: 0.0, ParamMouthForm: -0.15, ParamAngleY: -3, ParamAngleZ: -2 },
    duration: 800,
    description: 'Lazy (half-closed eyes, slight frown)',
  },
  maid: {
    nativeExpression: null,
    facialParams: { ParamEyeLOpen: 0.85, ParamEyeROpen: 0.85, ParamBrowLY: 0.1, ParamBrowRY: 0.1, ParamMouthForm: 0.5, ParamAngleY: 2, ParamAngleZ: 0 },
    duration: 600,
    description: 'Professional maid (smile, attentive)',
  },
  predator: {
    nativeExpression: 'angry',
    facialParams: { ParamEyeLOpen: 0.7, ParamEyeROpen: 0.7, ParamBrowLY: -0.7, ParamBrowRY: -0.7, ParamBrowLAngle: -0.4, ParamBrowRAngle: -0.4, ParamMouthForm: -0.4, ParamMouthOpenY: 0.15, ParamAngleX: 5 },
    duration: 400,
    description: 'Predator mode (angry + furrowed brows + narrowed eyes)',
  },
  hangry: {
    nativeExpression: 'sweat',
    facialParams: { ParamEyeLOpen: 0.65, ParamEyeROpen: 0.65, ParamBrowLAngle: -0.3, ParamBrowRAngle: -0.3, ParamMouthOpenY: 0.35, ParamMouthForm: -0.2, ParamAngleZ: 4 },
    duration: 500,
    description: 'Hangry (sweat + open mouth + tilted head)',
  },
  shy: {
    nativeExpression: 'blush',
    facialParams: { ParamEyeLOpen: 0.55, ParamEyeROpen: 0.55, ParamBrowLY: 0.2, ParamBrowRY: 0.2, ParamMouthForm: 0.1, ParamAngleZ: -10, ParamAngleY: -5 },
    duration: 700,
    description: 'Shy (blush + lowered head + tilt)',
  },
  surprised: {
    nativeExpression: 'stunned',
    facialParams: { ParamEyeLOpen: 1.0, ParamEyeROpen: 1.0, ParamBrowLY: 0.7, ParamBrowRY: 0.7, ParamMouthOpenY: 0.5, ParamMouthForm: 0.0, ParamAngleY: 3 },
    duration: 300,
    description: 'Surprised (stunned + wide eyes + open mouth)',
  },
  happy: {
    nativeExpression: 'blush',
    facialParams: { ParamEyeLOpen: 0.9, ParamEyeROpen: 0.9, ParamBrowLY: 0.35, ParamBrowRY: 0.35, ParamMouthForm: 0.85, ParamMouthOpenY: 0.2, ParamAngleY: 5, ParamAngleZ: 3 },
    duration: 600,
    description: 'Happy (blush + big smile + raised head)',
  },
};

export interface ExpressionApplyCallbacks {
  applyNativeExpression: (name: string | null) => void;
  setParameter: (id: string, value: number) => void;
}

export class ExpressionController {
  private currentEmotion: EllenEmotion = 'lazy';
  private targetParams: FacialParams = {};
  private currentParams: FacialParams = {};
  private transitionStartTime = 0;
  private transitionDuration = 0;
  private isTransitioning = false;
  private callbacks: ExpressionApplyCallbacks | null = null;

  registerCallbacks(callbacks: ExpressionApplyCallbacks): void {
    this.callbacks = callbacks;
    this.applyEmotion(this.currentEmotion, 0);
  }

  applyExpression(emotionId: string, duration?: number): void {
    const valid: EllenEmotion[] = ['lazy', 'maid', 'predator', 'hangry', 'shy', 'surprised', 'happy'];
    const emotion = valid.includes(emotionId as EllenEmotion) ? emotionId as EllenEmotion : 'lazy';
    this.applyEmotion(emotion, duration);
  }

  setMouthOpen(value: number): void {
    this.targetParams.ParamMouthOpenY = Math.max(0, Math.min(1, value));
    if (!this.isTransitioning) {
      this.currentParams.ParamMouthOpenY = this.targetParams.ParamMouthOpenY;
      this.callbacks?.setParameter('ParamMouthOpenY', this.currentParams.ParamMouthOpenY ?? 0);
    }
  }

  update(): boolean {
    if (!this.isTransitioning) return false;
    const elapsed = Date.now() - this.transitionStartTime;
    const progress = Math.min(1, elapsed / this.transitionDuration);
    const eased = 1 - Math.pow(1 - progress, 3);
    const allKeys = new Set([
      ...Object.keys(this.currentParams),
      ...Object.keys(this.targetParams),
    ]) as Set<keyof FacialParams>;
    allKeys.forEach((key) => {
      const current = (this.currentParams[key] as number) ?? 0;
      const target = (this.targetParams[key] as number) ?? 0;
      (this.currentParams as Record<string, number>)[key] = current + (target - current) * eased;
    });
    this.applyCurrentParams();
    if (progress >= 1) this.isTransitioning = false;
    return true;
  }

  getCurrentEmotion(): EllenEmotion { return this.currentEmotion; }
  resetToDefault(duration = 800): void { this.applyExpression('lazy', duration); }

  private applyEmotion(emotion: EllenEmotion, duration?: number): void {
    const config = EMOTION_CONFIGS[emotion];
    const transitionMs = duration ?? config.duration;
    this.targetParams = { ...config.facialParams };
    this.transitionStartTime = Date.now();
    this.transitionDuration = transitionMs;
    this.isTransitioning = transitionMs > 0;
    this.currentEmotion = emotion;
    if (this.callbacks) this.callbacks.applyNativeExpression(config.nativeExpression);
    if (transitionMs === 0) {
      this.currentParams = { ...this.targetParams };
      this.isTransitioning = false;
      this.applyCurrentParams();
    }
  }

  private applyCurrentParams(): void {
    if (!this.callbacks) return;
    Object.entries(this.currentParams).forEach(([id, value]) => {
      this.callbacks!.setParameter(id, value as number);
    });
  }
}
