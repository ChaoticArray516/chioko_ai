import {
  MultimodalSyncPacket,
  StatusMessage,
  ConnectionStatus,
  MessageHandler,
  StatusHandler,
} from '../types';

interface WSConfig {
  url: string;
  maxReconnectAttempts: number;
  reconnectDelay: number;
}

export class WebSocketClient {
  private ws: WebSocket | null = null;
  private config: WSConfig;
  private reconnectAttempts = 0;
  private reconnectTimer: number | null = null;
  private status: ConnectionStatus = 'disconnected';
  private onMessageHandler: MessageHandler | null = null;
  private onStatusHandler: StatusHandler | null = null;
  private onConnectionChange: ((status: ConnectionStatus) => void) | null = null;

  constructor(url: string, maxReconnectAttempts = 5, reconnectDelay = 1000) {
    this.config = { url, maxReconnectAttempts, reconnectDelay };
  }

  onMessage(handler: MessageHandler): void { this.onMessageHandler = handler; }
  onStatus(handler: StatusHandler): void { this.onStatusHandler = handler; }
  onConnectionStatusChange(handler: (status: ConnectionStatus) => void): void {
    this.onConnectionChange = handler;
  }
  getStatus(): ConnectionStatus { return this.status; }

  connect(): void {
    if (this.ws?.readyState === WebSocket.OPEN) return;
    this.setStatus('connecting');
    try {
      this.ws = new WebSocket(this.config.url);
      this.ws.onopen = () => {
        this.reconnectAttempts = 0;
        this.setStatus('connected');
      };
      this.ws.onmessage = (event) => { this.handleMessage(event.data); };
      this.ws.onclose = () => {
        this.setStatus('disconnected');
        this.attemptReconnect();
      };
      this.ws.onerror = () => { this.setStatus('disconnected'); };
    } catch {
      this.setStatus('disconnected');
      this.attemptReconnect();
    }
  }

  disconnect(): void {
    if (this.reconnectTimer) { window.clearTimeout(this.reconnectTimer); this.reconnectTimer = null; }
    if (this.ws) { this.ws.onclose = null; this.ws.close(); this.ws = null; }
    this.reconnectAttempts = 0;
    this.setStatus('disconnected');
  }

  send(message: object): boolean {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
      return true;
    }
    return false;
  }

  private handleMessage(data: string): void {
    try {
      const parsed = JSON.parse(data);
      if (parsed.type === 'multimodal_sync') {
        this.onMessageHandler?.(parsed as MultimodalSyncPacket);
      } else if (parsed.type === 'status') {
        this.onStatusHandler?.(parsed as StatusMessage);
      }
    } catch { /* ignore parse errors */ }
  }

  private attemptReconnect(): void {
    if (this.reconnectAttempts >= this.config.maxReconnectAttempts) return;
    this.reconnectAttempts++;
    const delay = this.config.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);
    this.reconnectTimer = window.setTimeout(() => { this.connect(); }, delay);
  }

  private setStatus(status: ConnectionStatus): void {
    if (this.status !== status) { this.status = status; this.onConnectionChange?.(status); }
  }
}
