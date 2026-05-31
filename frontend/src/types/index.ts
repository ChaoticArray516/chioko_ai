/**
 * Type definitions for Ellen Skill Frontend
 */

export interface MultimodalSyncPacket {
  type: 'multimodal_sync';
  text: string;
  audioData: string;
  motionId: string;
  expressionId: string;
  sampleRate: number;
  duration: number;
  timestamp: number;
  hasAudio: boolean;
}

export interface StatusMessage {
  type: 'status';
  status: 'ready' | 'thinking' | 'speaking' | 'error';
  message?: string;
}

export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected';

export interface EllenSkillState {
  connectionStatus: ConnectionStatus;
  currentText: string;
  currentExpression: string;
  isSpeaking: boolean;
  audioInitialized: boolean;
}

export interface ExpressionParams {
  [key: string]: number;
}

export type EllenEmotion =
  | 'lazy' | 'maid' | 'predator' | 'hangry' | 'shy' | 'surprised' | 'happy';

export type MotionId = 'idle' | 'idle2';

export type MessageHandler = (packet: MultimodalSyncPacket) => void;
export type StatusHandler = (status: StatusMessage) => void;
