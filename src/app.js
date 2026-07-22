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

// --- Event Listeners ---
document.addEventListener('DOMContentLoaded', async () => {
  const listen = window.__TAURI__.event.listen;

  // Listen for hotkey events from Rust backend
  await listen('hotkey:toggle-night-mode', toggleNightMode);
  await listen('hotkey:toggle-click-through', toggleClickThrough);
  await listen('hotkey:opacity-up', () => adjustOpacity(0.05));
  await listen('hotkey:opacity-down', () => adjustOpacity(-0.05));
  await listen('hotkey:copy-answer', copyAnswer);

  console.log('Phantom overlay initialized');
});
