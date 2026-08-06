// Phantom overlay — UI logic
// Tauri 2 exposes APIs via window.__TAURI__ when withGlobalTauri is true

let isClickThrough = true;
let isNightMode = false;
let opacity = 0.92;

// --- Night Mode ---
function toggleNightMode() {
  isNightMode = !isNightMode;
  document.body.classList.toggle('night-mode', isNightMode);
}

// --- Click-Through Toggle ---
async function toggleClickThrough() {
  isClickThrough = !isClickThrough;
  try {
    await window.__TAURI__.core.invoke('set_click_through', { enabled: isClickThrough });
    document.getElementById('status-indicator').textContent = isClickThrough
      ? '● Ready'
      : '● Interactive';
  } catch (e) {
    console.error('Failed to toggle click-through:', e);
  }
}

// --- Opacity ---
function adjustOpacity(delta) {
  opacity = Math.max(0.1, Math.min(1.0, opacity + delta));
  document.getElementById('overlay').style.opacity = opacity;
}

// --- Copy Answer ---
function copyAnswer() {
  const answerBox = document.getElementById('answer-box');
  const text = answerBox.innerText;
  if (text && !text.includes('Press Ctrl+Shift')) {
    navigator.clipboard.writeText(text).then(() => {
      const statusEl = document.getElementById('status-indicator');
      const prevText = statusEl.textContent;
      statusEl.textContent = '● Copied!';
      statusEl.style.color = '#6a8a5a';
      setTimeout(() => {
        statusEl.textContent = prevText;
        statusEl.style.color = '';
      }, 1500);
    }).catch(console.error);
  }
}

// --- Copy Code Blocks Only ---
function copyCode() {
  const codeBlocks = document.querySelectorAll('#answer-box pre code');
  const code = Array.from(codeBlocks).map(el => el.textContent).join('\n\n');
  if (code) {
    navigator.clipboard.writeText(code).catch(console.error);
  }
}

// --- Recording State ---
let isRecording = false;
let recordingStartTime = null;
let timerInterval = null;

function formatTime(seconds) {
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${mins}:${secs.toString().padStart(2, '0')}`;
}

function startTimer() {
  recordingStartTime = Date.now();
  const timerEl = document.getElementById('recording-timer');
  const statusEl = document.getElementById('status-indicator');

  timerEl.style.display = 'inline';
  timerEl.textContent = '0:00';
  statusEl.textContent = '● Recording';
  statusEl.style.color = '#c25550';

  timerInterval = setInterval(() => {
    const elapsed = Math.floor((Date.now() - recordingStartTime) / 1000);
    timerEl.textContent = formatTime(elapsed);
  }, 1000);
}

function stopTimer() {
  if (timerInterval) {
    clearInterval(timerInterval);
    timerInterval = null;
  }

  const timerEl = document.getElementById('recording-timer');
  const statusEl = document.getElementById('status-indicator');

  timerEl.style.display = 'none';
  statusEl.textContent = '● Processing...';
  statusEl.style.color = '#c8b88a';
}

function resetStatus() {
  const statusEl = document.getElementById('status-indicator');
  statusEl.textContent = isClickThrough ? '● Ready' : '● Interactive';
  statusEl.style.color = '#c8b88a';
}

// --- Mermaid Init ---
if (window.mermaid) {
  window.mermaid.initialize({
    startOnLoad: false,
    theme: 'dark',
    themeVariables: {
      primaryColor: '#3a3830',
      primaryBorderColor: '#c8b88a',
      primaryTextColor: '#b0ada4',
      lineColor: '#7a7870',
      secondaryColor: '#2a2920',
      tertiaryColor: '#1e1d1b',
    },
    flowchart: { htmlLabels: false },
  });
}
let mermaidCounter = 0;

// --- Answer Rendering ---
let currentAnswer = '';

function renderMarkdown(text) {
  let html;
  if (window.marked && window.marked.parse) {
    html = window.marked.parse(text);
  } else {
    html = text.replace(/\n/g, '<br>');
  }
  if (window.DOMPurify) {
    html = DOMPurify.sanitize(html);
  }
  return html;
}

function highlightCode() {
  if (window.hljs) {
    document.querySelectorAll('#answer-box pre code').forEach((block) => {
      if (!block.classList.contains('language-mermaid')) {
        window.hljs.highlightElement(block);
      }
    });
  }
}

async function renderMermaidBlocks() {
  if (!window.mermaid) return;
  const blocks = document.querySelectorAll('#answer-box pre code.language-mermaid');
  for (const block of blocks) {
    const pre = block.parentElement;
    if (pre.dataset.mermaidRendered) continue;
    pre.dataset.mermaidRendered = 'true';
    const code = block.textContent;
    try {
      const id = 'mermaid-' + (++mermaidCounter);
      const { svg } = await window.mermaid.render(id, code);
      const div = document.createElement('div');
      div.className = 'mermaid-diagram';
      div.innerHTML = svg; // Mermaid SVG is trusted (library-generated), no sanitization needed
      pre.replaceWith(div);
    } catch (e) {
      // If mermaid can't parse it, leave as code block
      console.warn('Mermaid render failed:', e);
    }
  }
}

function showAnswer(html) {
  const answerBox = document.getElementById('answer-box');
  answerBox.innerHTML = html;
  highlightCode();
  renderMermaidBlocks();
  document.getElementById('quick-actions').style.display = 'flex';
  document.getElementById('follow-up').style.display = 'block';
  // UX-1: show hint when follow-up is visible and click-through is active
  const hint = document.getElementById('follow-up-hint');
  if (hint) {
    hint.style.display = isClickThrough ? 'block' : 'none';
  }
  answerBox.scrollTop = answerBox.scrollHeight;
}

// --- Chunk Rendering Debounce (UX-7) ---
let chunkRenderTimer = null;

function appendChunk(chunk) {
  currentAnswer += chunk;
  if (!chunkRenderTimer) {
    chunkRenderTimer = setTimeout(() => {
      chunkRenderTimer = null;
      showAnswer(renderMarkdown(currentAnswer));
    }, 50);
  }
}

// --- Hotkey Help Overlay (FEAT-5) ---
function toggleHotkeyHelp() {
  const panel = document.getElementById('hotkey-help');
  if (!panel) return;
  panel.style.display = panel.style.display === 'none' ? 'block' : 'none';
}

// --- Event Listeners ---
document.addEventListener('DOMContentLoaded', async () => {
  const listen = window.__TAURI__.event.listen;

  // Hotkey events (backend-forwarded UI events for click-through and night mode)
  await listen('hotkey:toggle-night-mode-ui', toggleNightMode);
  await listen('hotkey:toggle-click-through-ui', (event) => {
    // Backend already toggled click-through; just update UI
    isClickThrough = event.payload;
    document.getElementById('status-indicator').textContent = isClickThrough
      ? '● Ready'
      : '● Interactive';
    // UX-1: show/hide follow-up hint based on click-through state
    const hint = document.getElementById('follow-up-hint');
    const followUp = document.getElementById('follow-up');
    if (hint) {
      hint.style.display = (isClickThrough && followUp.style.display !== 'none') ? 'block' : 'none';
    }
  });
  await listen('hotkey:opacity-up', () => adjustOpacity(0.05));
  await listen('hotkey:opacity-down', () => adjustOpacity(-0.05));
  await listen('hotkey:copy-answer', copyAnswer);
  await listen('hotkey:copy-code', copyCode);
  await listen('hotkey:scroll-down', () => {
    const box = document.getElementById('answer-box');
    box.scrollTop += 150;
  });
  await listen('hotkey:scroll-up', () => {
    const box = document.getElementById('answer-box');
    box.scrollTop -= 150;
  });

  // Model switching hotkeys (UX-4)
  await listen('hotkey:model-sol', () => {
    document.getElementById('mode-label').textContent = 'SOL (gpt-5.6-sol)';
    const statusEl = document.getElementById('status-indicator');
    const prevText = statusEl.textContent;
    statusEl.textContent = '● SOL';
    statusEl.style.color = '#c8b88a';
    setTimeout(() => {
      statusEl.textContent = prevText;
      statusEl.style.color = '';
    }, 1500);
  });
  await listen('hotkey:model-terra', () => {
    document.getElementById('mode-label').textContent = 'TERRA (gpt-5.6-terra)';
  });
  await listen('hotkey:model-luna', () => {
    document.getElementById('mode-label').textContent = 'LUNA (gpt-5.6-luna)';
  });
  // Mode lock cycle (Ctrl+Shift+4) and unlock (Ctrl+Shift+5)
  await listen('mode:locked', (event) => {
    const mode = event.payload;
    const modeLabel = document.getElementById('mode-label');
    const statusEl = document.getElementById('status-indicator');

    if (mode === 'auto') {
      modeLabel.textContent = 'Auto-detect';
      statusEl.textContent = '● Auto Mode';
      statusEl.style.color = '#6a8a5a';
    } else {
      modeLabel.textContent = mode.toUpperCase() + ' (locked)';
      statusEl.textContent = '● ' + mode.toUpperCase() + ' Locked';
      statusEl.style.color = '#c8b88a';
    }
    setTimeout(() => {
      statusEl.textContent = isClickThrough ? '● Ready' : '● Interactive';
      statusEl.style.color = '';
    }, 2000);
  });

  // Hotkey help overlay (FEAT-5)
  await listen('hotkey:show-help', toggleHotkeyHelp);

  // Log hotkey registration errors visibly
  await listen('hotkey:registration-error', (event) => {
    const answerBox = document.getElementById('answer-box');
    answerBox.innerHTML = '<p style="color:#c25550;font-size:12px;">Hotkey registration failed: ' + event.payload + '</p>';
  });

  // Recording events from backend
  await listen('recording:started', () => {
    isRecording = true;
    startTimer();
  });

  await listen('recording:warning', (event) => {
    const statusEl = document.getElementById('status-indicator');
    const prevText = statusEl.textContent;
    statusEl.textContent = '● ' + event.payload;
    statusEl.style.color = '#c8a832';
    setTimeout(() => {
      statusEl.textContent = prevText;
      statusEl.style.color = '#c25550';
    }, 3000);
  });

  await listen('recording:stopped', (event) => {
    isRecording = false;
    stopTimer();
    setTimeout(resetStatus, 2000);
  });

  await listen('recording:error', (event) => {
    isRecording = false;
    stopTimer();
    const statusEl = document.getElementById('status-indicator');
    statusEl.textContent = '● Error';
    statusEl.style.color = '#c25550';
    setTimeout(resetStatus, 3000);
  });

  // Pipeline events
  await listen('pipeline:started', () => {
    currentAnswer = '';
    const answerBox = document.getElementById('answer-box');
    answerBox.innerHTML = '<p class="pipeline-status">Processing audio...</p>';
    document.getElementById('quick-actions').style.display = 'none';
    document.getElementById('follow-up').style.display = 'none';
  });

  await listen('pipeline:status', (event) => {
    const answerBox = document.getElementById('answer-box');
    answerBox.innerHTML = '<p class="pipeline-status">' + DOMPurify.sanitize(event.payload) + '</p>';
  });

  await listen('pipeline:transcript', (event) => {
    // Show transcript preview in answer box until first answer chunk arrives
    const transcript = window.DOMPurify
      ? DOMPurify.sanitize(event.payload)
      : event.payload;
    const answerBox = document.getElementById('answer-box');
    answerBox.innerHTML = `<blockquote class="transcript-preview">📝 "${transcript}"</blockquote>`;
  });

  await listen('pipeline:extraction', (event) => {
    const data = event.payload;
    document.getElementById('mode-label').textContent =
      (data.mode || 'general').toUpperCase();
  });

  await listen('answer:mode', (event) => {
    document.getElementById('mode-label').textContent =
      (event.payload || 'general').toUpperCase();
  });

  await listen('answer:chunk', (event) => {
    appendChunk(event.payload);
  });

  await listen('answer:done', async (event) => {
    // Flush any pending debounced render immediately
    if (chunkRenderTimer) {
      clearTimeout(chunkRenderTimer);
      chunkRenderTimer = null;
    }
    currentAnswer = event.payload;
    showAnswer(renderMarkdown(currentAnswer));
    resetStatus();

    // TTS earpiece — speak the answer if enabled
    try {
      const audioB64 = await window.__TAURI__.core.invoke('speak_answer', { text: currentAnswer });
      const audio = new Audio('data:audio/mp3;base64,' + audioB64);
      audio.volume = 0.7;
      audio.play().catch(() => {});
    } catch (_) {
      // TTS disabled or failed — silent
    }
  });

  await listen('pipeline:error', (event) => {
    const answerBox = document.getElementById('answer-box');
    const msg = (window.DOMPurify ? DOMPurify.sanitize(event.payload) : event.payload);
    answerBox.innerHTML = '<p class="pipeline-status" style="color: #c25550;">Error: ' + msg + '</p>';
    resetStatus();
  });

  // Screenshot events
  await listen('screenshot:taken', (event) => {
    const count = event.payload;
    const countEl = document.getElementById('screenshot-count');
    countEl.textContent = 'SS: ' + count;
    countEl.style.display = 'inline';
  });

  await listen('screenshot:error', (event) => {
  });

  await listen('screenshot:cleared', () => {
    document.getElementById('screenshot-count').style.display = 'none';
  });

  await listen('session:cleared', () => {
    currentAnswer = '';
    document.getElementById('answer-box').innerHTML =
      '<p class="placeholder-text">Session cleared. Press Ctrl+Shift+F6 to start recording</p>';
    document.getElementById('quick-actions').style.display = 'none';
    document.getElementById('follow-up').style.display = 'none';
    document.getElementById('screenshot-count').style.display = 'none';
    document.getElementById('mode-label').textContent = 'General';
    const hint = document.getElementById('follow-up-hint');
    if (hint) hint.style.display = 'none';
    resetStatus();
  });

  // Copy buttons
  document.getElementById('btn-copy').addEventListener('click', copyAnswer);
  document.getElementById('btn-copy-code').addEventListener('click', copyCode);

  // Follow-up question
  const followUpInput = document.getElementById('follow-up-input');
  if (followUpInput) {
    followUpInput.addEventListener('keydown', async (e) => {
      if (e.key === 'Enter' && followUpInput.value.trim()) {
        const question = followUpInput.value.trim();
        followUpInput.value = '';
        currentAnswer = '';
        document.getElementById('answer-box').innerHTML =
          '<p class="pipeline-status">Thinking...</p>';

        try {
          await window.__TAURI__.core.invoke('ask_followup', { question });
        } catch (err) {
          const msg = (window.DOMPurify ? DOMPurify.sanitize(String(err)) : String(err));
          document.getElementById('answer-box').innerHTML =
            '<p class="pipeline-status" style="color: #c25550;">Error: ' + msg + '</p>';
        }
      }
    });
  }

});
