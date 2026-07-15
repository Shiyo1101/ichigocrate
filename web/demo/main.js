// IchigoCrate wasm デモのエントリ。ビルド済み pkg/ のグルーを読み込み、
// canvas へ 1 インスタンスを貼って rAF とキーイベントを配線する。
import init, { IchigoCrateRunner } from "../pkg/ichigocrate_web.js";

await init();

const canvas = document.getElementById("screen");
// storagePrefix でこのデモ用にスロットを分離 (persist=true: localStorage)。
const runner = new IchigoCrateRunner(canvas, "demo", true);
// デモ用: コンソールから命令ハンドル (type/exec/run/getScreenText 等) を試せる。
window.runner = runner;
window.IchigoCrateRunner = IchigoCrateRunner;
canvas.focus();

// onPrint: 画面出力ストリームを右のログへ追記する。
const log = document.getElementById("log");
runner.onPrint((chunk) => {
  log.textContent += chunk;
  log.scrollTop = log.scrollHeight;
});

// rAF ごとに 1 フレーム進める。performance.now() を時間基準に渡す。
// 実機 LED の代わりに、点灯中は画面枠を赤くする (native 版と同じ表現)。
function frame(t) {
  runner.tick(t);
  canvas.style.borderColor = runner.is_led() ? "#e62828" : "#333";
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

// ブラウザ既定動作 (スクロール/リロード/フォーカス移動) と衝突するキーは
// canvas にフォーカスがある間だけ抑止する。
const PREVENT = new Set([
  "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown",
  "Space", "Tab", "Enter", "Backspace",
  "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10",
]);

canvas.addEventListener("keydown", (e) => {
  // Ctrl/Cmd 併用 (コピー等) は OS/ブラウザに委ねる。
  if (e.ctrlKey || e.metaKey) return;
  runner.on_key(e.code, e.shiftKey, e.altKey, true);
  if (PREVENT.has(e.code)) e.preventDefault();
});
canvas.addEventListener("keyup", (e) => {
  runner.on_key(e.code, e.shiftKey, e.altKey, false);
});
