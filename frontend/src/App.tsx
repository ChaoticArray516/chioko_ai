import { useState, useCallback, useRef, useEffect } from 'react';
import { useEllenSkill } from './hooks/useEllenSkill';
import { Live2DCanvas } from './components/Live2DCanvas';
import './App.css';

function App() {
  const {
    connectionStatus, currentText, currentExpression, currentMotion,
    isSpeaking, audioInitialized, ttsAvailable, initializeAudio, sendMessage, reconnect,
  } = useEllenSkill();

  const [inputText, setInputText] = useState('');
  const [showStartButton, setShowStartButton] = useState(true);
  const [chatOpen, setChatOpen] = useState(false);
  const [messages, setMessages] = useState<Array<{role: 'user' | 'ellen', content: string, expression?: string}>>([
    { role: 'ellen', content: 'はぁ…ご主人様、何か用？できるだけ早く済ませてほしいな。', expression: 'lazy' },
  ]);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => { messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' }); }, [messages, currentText]);

  useEffect(() => {
    if (currentText && !isSpeaking) {
      setMessages(prev => {
        const filtered = prev.filter(m => !(m.role === 'ellen' && m.content === '...'));
        return [...filtered, { role: 'ellen', content: currentText, expression: currentExpression }];
      });
    }
  }, [currentText, isSpeaking, currentExpression]);

  const handleStart = useCallback(() => {
    const success = initializeAudio();
    if (success) setShowStartButton(false);
  }, [initializeAudio]);

  const handleSend = useCallback(() => {
    if (!inputText.trim()) return;
    const message = inputText.trim();
    setMessages(prev => [...prev, { role: 'user', content: message }]);
    setInputText('');
    const success = sendMessage(message);
    if (success) setMessages(prev => [...prev, { role: 'ellen', content: '...' }]);
  }, [inputText, sendMessage]);

  const handleModelHit = useCallback((hitAreas: string[]) => {
    if (hitAreas.length > 0) sendMessage('[System: User clicked on you, please respond with shy or surprised expression]');
  }, [sendMessage]);

  const handleKeyPress = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend(); }
  }, [handleSend]);

  const getStatusColor = () => {
    switch (connectionStatus) {
      case 'connected': return '#4ade80';
      case 'connecting': return '#fbbf24';
      case 'disconnected': return '#ef4444';
      default: return '#9ca3af';
    }
  };

  const getStatusText = () => {
    switch (connectionStatus) {
      case 'connected': return 'System Online';
      case 'connecting': return 'Establishing Link...';
      case 'disconnected': return 'Link Severed';
      default: return 'Unknown';
    }
  };

  return (
    <div className="app-container">
      <div className="live2d-section">
        <div className="live2d-wrapper">
          <Live2DCanvas modelPath="/Shark/shark.model3.json"
            motionId={currentMotion || (isSpeaking ? 'idle2' : 'idle')}
            expressionId={currentExpression} onHit={handleModelHit} autoFit={true} />
          {showStartButton && (
            <div className="start-overlay">
              <button className="start-button" onClick={handleStart}>
                <span className="start-icon">▶</span>
                <span>Click to Start</span>
                <span className="start-subtitle">音声を有効化</span>
              </button>
            </div>
          )}
        </div>
        <div className="status-indicator-float">
          <div className="status-dot" style={{ backgroundColor: getStatusColor() }} />
          <span>{getStatusText()}</span>
        </div>
        <div className="expression-badge-float">
          <span className="expression-label">Expression:</span>
          <span className="expression-value">{currentExpression}</span>
          {isSpeaking && <span className="speaking-dot" />}
        </div>
        <button className="mobile-toggle" onClick={() => setChatOpen(!chatOpen)} aria-label="Toggle chat">💬</button>
      </div>

      <div className={`chat-panel ${chatOpen ? 'open' : ''}`}>
        <div className="chat-header">
          <div className="chat-avatar">エ</div>
          <div className="chat-info">
            <div className="chat-name">エレン・ジョー</div>
            <div className="chat-status">
              <div className="status-dot-small" style={{ backgroundColor: getStatusColor() }} />
              <span>{getStatusText()}</span>
            </div>
          </div>
          <div className="rank-badge"><span className="rank-letter">S</span><span className="rank-text">RANK</span></div>
        </div>

        <div className="chat-messages">
          {messages.map((msg, index) => (
            <div key={index} className={`message ${msg.role}`}>
              <div className={`message-avatar ${msg.role}`}>{msg.role === 'user' ? 'You' : 'エ'}</div>
              <div className="message-content">
                {msg.role === 'ellen' && msg.expression && <div className="message-emotion-tag">{msg.expression}</div>}
                <div className="message-text">{msg.content}</div>
              </div>
            </div>
          ))}
          {isSpeaking && (
            <div className="message ellen typing">
              <div className="message-avatar ellen">エ</div>
              <div className="message-content">
                <div className="typing-indicator"><span></span><span></span><span></span></div>
              </div>
            </div>
          )}
          <div ref={messagesEndRef} />
        </div>

        <div className="chat-input-container">
          <div className="input-group">
            <input type="text" value={inputText} onChange={(e) => setInputText(e.target.value)} onKeyDown={handleKeyPress}
              placeholder="Type your message..." disabled={connectionStatus !== 'connected'} className="chat-input" />
            <button onClick={handleSend} disabled={!inputText.trim() || connectionStatus !== 'connected'}
              className="send-btn" aria-label="Send message">➤</button>
          </div>
          <div className="chat-toolbar">
            <button onClick={reconnect} disabled={connectionStatus === 'connected'} className="toolbar-btn">🔄 Reconnect</button>
            <span className="audio-status">🔊 {audioInitialized ? 'Audio On' : 'Audio Off'} | TTS: {ttsAvailable ? '✓ GPT-SoVITS Active' : '✗ No Voice (TTS offline)'}</span>
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
