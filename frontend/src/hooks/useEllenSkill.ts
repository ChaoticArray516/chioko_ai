import { useState, useEffect, useRef, useCallback } from 'react';
import { WebSocketClient } from '../services/WebSocketClient';
import { AudioLipSync } from '../services/AudioLipSync';
import { ExpressionController } from '../services/ExpressionController';
import { MultimodalSyncPacket, StatusMessage, ConnectionStatus, EllenSkillState } from '../types';

const WS_URL = 'ws://127.0.0.1:8081';
const MAX_RECONNECT_ATTEMPTS = 5;
const RECONNECT_DELAY = 1000;

export function useEllenSkill() {
  const [state, setState] = useState<EllenSkillState & { ttsAvailable: boolean; currentMotion: string }>({
    connectionStatus: 'disconnected',
    currentText: '',
    currentExpression: 'lazy',
    currentMotion: 'idle',
    isSpeaking: false,
    audioInitialized: false,
    ttsAvailable: false,
  });

  const wsClientRef = useRef<WebSocketClient | null>(null);
  const audioRef = useRef<AudioLipSync | null>(null);
  const expressionRef = useRef<ExpressionController | null>(null);

  useEffect(() => {
    expressionRef.current = new ExpressionController();
    const wsClient = new WebSocketClient(WS_URL, MAX_RECONNECT_ATTEMPTS, RECONNECT_DELAY);
    wsClientRef.current = wsClient;
    const audio = new AudioLipSync();
    audioRef.current = audio;

    wsClient.onConnectionStatusChange((status: ConnectionStatus) => {
      setState((prev) => ({ ...prev, connectionStatus: status }));
    });

    wsClient.onMessage((packet: MultimodalSyncPacket) => {
      handleMultimodalPacket(packet);
    });

    wsClient.onStatus((statusMsg: StatusMessage) => {
      handleStatusMessage(statusMsg);
    });

    wsClient.connect();

    return () => { wsClient.disconnect(); audio.dispose(); };
  }, []);

  const getMotionForEmotion = useCallback((emotion: string, speaking: boolean): string => {
    if (speaking) {
      switch (emotion) {
        case 'predator': case 'surprised': return 'alert';
        case 'shy': return 'shy_fidget';
        case 'hangry': return 'hangry_sway';
        case 'lazy': return 'lazy_stretch';
        default: return 'idle2';
      }
    }
    switch (emotion) {
      case 'predator': case 'surprised': return 'alert';
      case 'shy': return 'shy_fidget';
      case 'hangry': return 'hangry_sway';
      case 'lazy': return 'lazy_stretch';
      default: return 'idle';
    }
  }, []);

  const handleMultimodalPacket = useCallback((packet: MultimodalSyncPacket) => {
    expressionRef.current?.applyExpression(packet.expressionId);
    const newMotion = getMotionForEmotion(packet.expressionId, false);
    setState((prev) => ({
      ...prev,
      currentText: packet.text,
      currentExpression: packet.expressionId,
      currentMotion: newMotion,
      ttsAvailable: packet.hasAudio,
    }));
    if (packet.hasAudio && packet.audioData) {
      playAudio(packet.audioData, packet.sampleRate);
    }
  }, [getMotionForEmotion]);

  const handleStatusMessage = useCallback((statusMsg: StatusMessage) => {
    switch (statusMsg.status) {
      case 'thinking':
        setState((prev) => ({ ...prev, isSpeaking: false, currentMotion: getMotionForEmotion(prev.currentExpression, false) }));
        break;
      case 'speaking':
        setState((prev) => ({ ...prev, isSpeaking: true, currentMotion: getMotionForEmotion(prev.currentExpression, true) }));
        break;
      case 'ready':
        setState((prev) => ({ ...prev, isSpeaking: false, currentMotion: getMotionForEmotion(prev.currentExpression, false) }));
        break;
      case 'error':
        setState((prev) => ({ ...prev, isSpeaking: false, currentMotion: getMotionForEmotion(prev.currentExpression, false) }));
        break;
    }
  }, []);

  const playAudio = useCallback(async (audioData: string, sampleRate: number) => {
    const audio = audioRef.current;
    if (!audio) return;
    try {
      await audio.playAudio(audioData, sampleRate, (mouthOpenness) => {
        expressionRef.current?.setMouthOpen(mouthOpenness);
      });
    } catch { /* audio playback failed */ }
  }, []);

  const initializeAudio = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return false;
    const success = audio.initialize();
    if (success) setState((prev) => ({ ...prev, audioInitialized: true }));
    return success;
  }, []);

  const sendMessage = useCallback((message: string) => {
    const wsClient = wsClientRef.current;
    if (!wsClient) return false;
    return wsClient.send({ type: 'message', content: message, timestamp: Date.now() });
  }, []);

  const reconnect = useCallback(() => { wsClientRef.current?.disconnect(); wsClientRef.current?.connect(); }, []);

  return {
    connectionStatus: state.connectionStatus,
    currentText: state.currentText,
    currentExpression: state.currentExpression,
    currentMotion: state.currentMotion,
    isSpeaking: state.isSpeaking,
    audioInitialized: state.audioInitialized,
    ttsAvailable: state.ttsAvailable,
    initializeAudio,
    sendMessage,
    reconnect,
  };
}
