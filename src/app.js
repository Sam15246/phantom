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
    navigator.clipboard.writeText(text).catch(console.error);
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

// --- Event Listeners ---
document.addEventListener('DOMContentLoaded', async () => {
  const listen = window.__TAURI__.event.listen;

  // Hotkey events
  await listen('hotkey:toggle-night-mode', toggleNightMode);
  await listen('hotkey:toggle-click-through', toggleClickThrough);
  await listen('hotkey:opacity-up', () => adjustOpacity(0.05));
  await listen('hotkey:opacity-down', () => adjustOpacity(-0.05));
  await listen('hotkey:copy-answer', copyAnswer);

  // Recording events from backend
  await listen('recording:started', () => {
    isRecording = true;
    startTimer();
  });

  await listen('recording:stopped', (event) => {
    isRecording = false;
    stopTimer();
    console.log('Recording stopped, WAV size:', event.payload, 'bytes');
    setTimeout(resetStatus, 2000);
  });

  await listen('recording:error', (event) => {
    isRecording = false;
    stopTimer();
    console.error('Recording error:', event.payload);
    const statusEl = document.getElementById('status-indicator');
    statusEl.textContent = '● Error';
    statusEl.style.color = '#c25550';
    setTimeout(resetStatus, 3000);
  });

  console.log('Phantom overlay initialized');
});
