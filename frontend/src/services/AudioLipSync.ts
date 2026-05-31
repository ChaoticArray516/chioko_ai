export type LipSyncCallback = (value: number) => void;

export class AudioLipSync {
  private audioContext: AudioContext | null = null;
  private analyser: AnalyserNode | null = null;
  private source: AudioBufferSourceNode | null = null;
  private gainNode: GainNode | null = null;
  private isPlaying = false;
  private lipSyncCallback: LipSyncCallback | null = null;
  private animationFrameId: number | null = null;

  initialize(): boolean {
    if (this.audioContext) return true;
    try {
      const AudioContextClass = window.AudioContext || (window as any).webkitAudioContext;
      if (!AudioContextClass) return false;
      this.audioContext = new AudioContextClass();
      this.analyser = this.audioContext.createAnalyser();
      this.analyser.fftSize = 256;
      this.analyser.smoothingTimeConstant = 0.8;
      this.gainNode = this.audioContext.createGain();
      this.gainNode.gain.value = 1.0;
      this.analyser.connect(this.gainNode);
      this.gainNode.connect(this.audioContext.destination);
      return true;
    } catch {
      return false;
    }
  }

  isInitialized(): boolean {
    return this.audioContext !== null && this.audioContext.state !== 'closed';
  }

  async playAudio(base64Audio: string, _sampleRate: number, lipSyncCallback: LipSyncCallback): Promise<void> {
    if (!this.audioContext) throw new Error('AudioContext not initialized');
    if (this.audioContext.state === 'suspended') await this.audioContext.resume();
    this.stop();
    this.lipSyncCallback = lipSyncCallback;
    const binaryString = atob(base64Audio);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) bytes[i] = binaryString.charCodeAt(i);
    const audioBuffer = await this.audioContext.decodeAudioData(bytes.buffer);
    this.source = this.audioContext.createBufferSource();
    this.source.buffer = audioBuffer;
    this.source.connect(this.analyser!);
    this.source.start(0);
    this.isPlaying = true;
    this.startLipSyncLoop();
    return new Promise((resolve) => {
      this.source!.onended = () => {
        this.isPlaying = false;
        this.stopLipSyncLoop();
        this.lipSyncCallback?.(0);
        resolve();
      };
    });
  }

  stop(): void {
    if (this.source) {
      try { this.source.stop(); } catch { /* already stopped */ }
      this.source.disconnect();
      this.source = null;
    }
    this.isPlaying = false;
    this.stopLipSyncLoop();
    this.lipSyncCallback?.(0);
  }

  setVolume(volume: number): void {
    if (this.gainNode) this.gainNode.gain.value = Math.max(0, Math.min(1, volume));
  }

  dispose(): void {
    this.stop();
    if (this.analyser) { this.analyser.disconnect(); this.analyser = null; }
    if (this.gainNode) { this.gainNode.disconnect(); this.gainNode = null; }
    if (this.audioContext) { this.audioContext.close(); this.audioContext = null; }
  }

  private startLipSyncLoop(): void {
    if (!this.analyser || !this.lipSyncCallback) return;
    const bufferLength = this.analyser.frequencyBinCount;
    const dataArray = new Uint8Array(bufferLength);
    const updateLipSync = () => {
      if (!this.isPlaying || !this.analyser) return;
      this.analyser.getByteFrequencyData(dataArray);
      let sum = 0; let count = 0;
      const startBin = Math.floor(bufferLength * 0.05);
      const endBin = Math.floor(bufferLength * 0.5);
      for (let i = startBin; i < endBin; i++) { sum += dataArray[i]; count++; }
      const average = sum / count;
      const mouthOpenness = Math.pow(average / 255, 0.7);
      this.lipSyncCallback!(mouthOpenness);
      this.animationFrameId = requestAnimationFrame(updateLipSync);
    };
    updateLipSync();
  }

  private stopLipSyncLoop(): void {
    if (this.animationFrameId) { cancelAnimationFrame(this.animationFrameId); this.animationFrameId = null; }
  }
}
