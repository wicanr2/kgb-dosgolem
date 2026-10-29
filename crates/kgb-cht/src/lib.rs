// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! KGB 適配層：接手文字 routine（docs/spec/004-kgb-text-takeover.md）、譯文表與 3 倍覆繪。
//! 遊戲專屬的偏移與簽章只放在這裡，通用核心不認識 KGB。
pub mod catalog;
pub mod lifecycle;
pub mod overlay;
pub mod takeover;
