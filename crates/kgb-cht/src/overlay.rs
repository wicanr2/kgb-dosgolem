// SPDX-License-Identifier: LicenseRef-RRSAL-1.0
//! 3 倍覆繪：原版 320×200 畫面放大後，把被接手的文字以中文畫上去。
//! 排版沿用原版規則（docs/spec/004 §2）：區塊依行數上移 y（每少一行下移 6）、逐行置中；
//! 單行從 x 起畫。字色取原版顏色索引的 DAC 值，外加黑色描邊。
use crate::takeover::{Kind, TextEvent};
use fontdue::{Font, FontSettings};
use std::collections::HashMap;

pub const SCALE: usize = 3;
pub const W: usize = 320 * SCALE;
pub const H: usize = 200 * SCALE;

pub struct Glyphs {
    font: Font,
    cache: HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>,
}

impl Glyphs {
    /// collection_index：Noto Sans CJK 的 TC 為 3。
    pub fn load(bytes: Vec<u8>, collection_index: u32) -> Result<Self, String> {
        let font = Font::from_bytes(
            bytes,
            FontSettings {
                collection_index,
                scale: 24.0,
                ..FontSettings::default()
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            font,
            cache: HashMap::new(),
        })
    }
    pub fn has(&self, c: char) -> bool {
        c == ' ' || self.font.has_glyph(c)
    }
    fn glyph(&mut self, c: char, px: u32) -> &(fontdue::Metrics, Vec<u8>) {
        let font = &self.font;
        self.cache
            .entry((c, px))
            .or_insert_with(|| font.rasterize(c, px as f32))
    }
    fn advance(&mut self, c: char, px: u32) -> i32 {
        self.glyph(c, px).0.advance_width.round() as i32
    }
    fn ascent(&self, px: u32) -> i32 {
        self.font
            .horizontal_line_metrics(px as f32)
            .map_or(px as i32 * 7 / 8, |m| m.ascent.round() as i32)
    }
}

/// 一個顯示中的覆繪項目。
#[derive(Clone, Debug)]
pub struct Item {
    pub event: TextEvent,
    pub zh: String,
}

/// 依寬度斷行：中文逐字可斷，ASCII 連續字母視為一個詞。`\n` 強制換行。
pub fn wrap(g: &mut Glyphs, text: &str, px: u32, max: i32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut tokens: Vec<String> = Vec::new();
        for c in para.chars() {
            let word = c.is_ascii_alphanumeric();
            match tokens.last_mut() {
                Some(t) if word && t.chars().last().is_some_and(|p| p.is_ascii_alphanumeric()) => {
                    t.push(c)
                }
                _ => tokens.push(c.to_string()),
            }
        }
        let (mut line, mut w) = (String::new(), 0);
        for t in tokens {
            let tw: i32 = t.chars().map(|c| g.advance(c, px)).sum();
            if w + tw > max && !line.is_empty() {
                lines.push(std::mem::take(&mut line).trim_end().to_string());
                w = 0;
                if t == " " {
                    continue;
                }
            }
            line.push_str(&t);
            w += tw;
        }
        lines.push(line.trim_end().to_string());
    }
    lines
}

fn blend(dst: &mut [u8], at: usize, rgb: [u8; 3], a: u8) {
    if a == 0 {
        return;
    }
    for k in 0..3 {
        let d = u16::from(dst[at + k]);
        dst[at + k] = ((d * u16::from(255 - a) + u16::from(rgb[k]) * u16::from(a)) / 255) as u8;
    }
}

fn draw_text(g: &mut Glyphs, out: &mut [u8], text: &str, x: i32, top: i32, px: u32, rgb: [u8; 3]) {
    let base = top + g.ascent(px);
    let outline = SCALE as i32 / 2 + 1;
    for pass in 0..2 {
        let mut pen = x;
        for c in text.chars() {
            let (m, bitmap) = g.glyph(c, px).clone();
            let gx = pen + m.xmin;
            let gy = base - m.ymin - m.height as i32;
            let offsets: Vec<(i32, i32)> = if pass == 0 {
                (-outline..=outline)
                    .flat_map(|dy| (-outline..=outline).map(move |dx| (dx, dy)))
                    .collect()
            } else {
                vec![(0, 0)]
            };
            let color = if pass == 0 { [0, 0, 0] } else { rgb };
            for (ox, oy) in offsets {
                for row in 0..m.height {
                    for col in 0..m.width {
                        let a = bitmap[row * m.width + col];
                        let (px_, py) = (gx + col as i32 + ox, gy + row as i32 + oy);
                        if a == 0 || px_ < 0 || py < 0 || px_ >= W as i32 || py >= H as i32 {
                            continue;
                        }
                        blend(out, (py as usize * W + px_ as usize) * 3, color, a);
                    }
                }
            }
            pen += m.advance_width.round() as i32;
        }
    }
}

/// 字型大小：原版行高 × 倍率的 0.9。
pub fn font_px(line_height: u16) -> u32 {
    (u32::from(line_height.max(8)) * SCALE as u32 * 9 / 10).max(12)
}

/// 放大原版畫面並畫上覆繪項目。dac 為 256 色 8 位元 RGB。
pub fn compose(rgb320: &[u8], items: &[Item], dac: &[[u8; 3]; 256], g: &mut Glyphs) -> Vec<u8> {
    let mut out = vec![0; W * H * 3];
    for y in 0..H {
        for x in 0..W {
            let s = ((y / SCALE) * 320 + x / SCALE) * 3;
            out[(y * W + x) * 3..(y * W + x) * 3 + 3].copy_from_slice(&rgb320[s..s + 3]);
        }
    }
    for it in items {
        let e = &it.event;
        let px = font_px(e.line_height);
        let lh = i32::from(e.line_height.max(8)) * SCALE as i32;
        // 字形像素＝[440e]＋(值 & 0Fh)，主字色值為 1（驅動 51e0:12d1，docs/spec/004 §2）。
        let rgb = dac[usize::from(e.color.wrapping_add(1))];
        match e.kind {
            Kind::Block => {
                let width = i32::from(if e.width == 0 { 320 } else { e.width }) * SCALE as i32;
                let lines = wrap(g, &it.zh, px, width - 4 * SCALE as i32);
                let n = lines.len() as i32;
                let top = (i32::from(e.y) + (i32::from(e.max_lines) - n) * 6) * SCALE as i32;
                for (i, line) in lines.iter().enumerate() {
                    let lw: i32 = line.chars().map(|c| g.advance(c, px)).sum();
                    let x = i32::from(e.x) * SCALE as i32 + (width - lw) / 2;
                    draw_text(g, &mut out, line, x, top + i as i32 * lh, px, rgb);
                }
            }
            Kind::Line => {
                let x = i32::from(e.x) * SCALE as i32;
                let top = i32::from(e.y) * SCALE as i32;
                draw_text(g, &mut out, &it.zh.replace('\n', " "), x, top, px, rgb);
            }
        }
    }
    out
}
