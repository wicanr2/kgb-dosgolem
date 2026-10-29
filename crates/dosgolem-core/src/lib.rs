// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 自有 RRSAL 實模式 DOS 核心。種子取自 dosgolem-rust-xaika `src/native`
//! （commit dd3f8b8），該程式移植自 Go dosgolem f79eab6；不含 rust-dos 實作。
//! 來源與雜湊見 `external/xaika-native/source.json`。
pub mod cpu;
mod font;
pub mod machine;
pub mod memory;
pub mod mz;
