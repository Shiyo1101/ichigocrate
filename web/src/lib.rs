//! IchigoJam-RS の WebAssembly フロントエンド。
//!
//! eframe/egui は載せず、`ichigojam-core` の VM を直接駆動して `<canvas>` の 2D
//! コンテキストへ 1bpp 画面を blit する軽量ランナー。JS 側 (React ラッパや素の
//! HTML) が `requestAnimationFrame` ごとに [`IchigoJamRunner::tick`] を、キーイベント
//! ごとに [`IchigoJamRunner::on_key`] を呼ぶ受動オブジェクトとして設計する。これにより
//! 1 ページに複数インスタンスを貼ってもグローバル状態を共有しない。
//!
//! モジュール構成:
//! - [`runner`]: VM を駆動する `IchigoJamRunner` 本体 (JS 公開面)。
//! - [`keymap`]: `KeyboardEvent.code` から HID / BTN コードへの変換。
//! - [`output`]: onPrint 用の VRAM 差分ヘルパ。
//! - [`storage`]: SAVE/LOAD/FILES の localStorage 実装。

mod keymap;
mod output;
mod runner;
mod storage;

pub use runner::IchigoJamRunner;
