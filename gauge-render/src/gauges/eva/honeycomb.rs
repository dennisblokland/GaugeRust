//! `EvaHoneycomb`: four rows of hexagon cells that light from left to right.
//! Cells below zero are green (vacuum on a boost gauge), cells above
//! `WarnValue` red, the rest orange. A seven-segment readout and the kanji
//! title sit above, a legend of the three zones below.

use crate::canvas::{Canvas, Sprite};
use crate::gauges::eva::{
	digits, footer, format_value, hexagon, jp_or_title, scale_label, screen_frame, stroke_rect, text, warn_value,
	Labels, DIM, FAINT, GREEN, GREEN_TEXT, GREEN_UNLIT, ORANGE, READOUT, RED, RED_TEXT, RED_UNLIT, TITLE, UNLIT,
};
use crate::gauges::{draw_background, render_static, Gauge, GaugeBase, GaugeConfig};
use crate::text::TextAlign;

const COLUMNS: usize = 17;
const ROWS: usize = 4;
const CELL_W: f32 = 31.0;
const CELL_H: f32 = 36.0;
const STEP_X: f32 = 34.0;
const STEP_Y: f32 = 30.0;
const GRID_LEFT: f32 = 32.0;
const GRID_TOP: f32 = 196.0;
const SCALE_Y: f32 = 340.0;
const LEGEND_Y: f32 = 360.0;

/// One cell: its centre and the value it stands for.
struct Cell {
	cx: f32,
	cy: f32,
	value: f32,
}

pub struct EvaHoneycombGauge {
	base: GaugeBase,
	labels: Labels,
	warn: f32,
	decimals: i32,
	size: (u32, u32),
	cells: Vec<Cell>,
	static_layer: Option<Sprite>,
}

impl EvaHoneycombGauge {
	pub fn new(config: &GaugeConfig, width: u32, height: u32) -> Self {
		let (min, max) = (config.min_value, config.max_value);
		let mut cells = Vec::new();
		for row in 0..ROWS {
			let odd = row % 2 == 1;
			let count = if odd { COLUMNS - 1 } else { COLUMNS };
			for col in 0..count {
				let offset = if odd { 0.5 } else { 0.0 };
				let t = (col as f32 + 0.5 + offset) / COLUMNS as f32;
				cells.push(Cell {
					cx: GRID_LEFT + CELL_W / 2.0 + (col as f32 + offset) * STEP_X,
					cy: GRID_TOP + CELL_H / 2.0 + row as f32 * STEP_Y,
					value: min + t * (max - min),
				});
			}
		}
		Self {
			base: GaugeBase::new(config),
			labels: Labels::new(config),
			warn: warn_value(config, 0.8),
			decimals: config.decimals.max(if max - min <= 10.0 { 2 } else { 0 }),
			size: (width, height),
			cells,
			static_layer: None,
		}
	}

	/// The x of `v` along the grid, for scale labels.
	fn x_of(&self, v: f32) -> f32 {
		GRID_LEFT + CELL_W / 2.0 + self.base.fraction(v) * (COLUMNS as f32 - 1.0) * STEP_X
	}

	fn update_static(&mut self) {
		let (w, h) = (self.size.0 as f32, self.size.1 as f32);
		let (min, max, warn) = (self.base.min_value, self.base.max_value, self.warn);
		let labels = self.labels.clone();
		let mut marks = vec![min, max, warn];
		if min < 0.0 && max > 0.0 {
			marks.push(0.0);
		}
		let marks: Vec<(f32, String)> = marks.into_iter().map(|v| (self.x_of(v), scale_label(v))).collect();
		let cells: Vec<(f32, f32, bool, bool)> =
			self.cells.iter().map(|c| (c.cx, c.cy, c.value < 0.0, c.value > warn)).collect();
		self.static_layer = Some(render_static(self.size.0, self.size.1, |c| {
			draw_background(c);
			screen_frame(c, &labels, w, h);
			jp_or_title(c, &labels, 24.0, 136.0, 64.0);
			let unit = if labels.unit.is_empty() { labels.source.clone() } else { labels.unit.to_uppercase() };
			text(c, &unit, w - 24.0, 172.0, TextAlign::Right, super::label_font(), 20.0, DIM);
			c.draw_line(24.0, 182.0, w - 24.0, 182.0, &crate::canvas::Paint::stroke(FAINT, 1.0));
			for (cx, cy, low, high) in cells {
				let color = if low {
					GREEN_UNLIT
				} else if high {
					RED_UNLIT
				} else {
					UNLIT
				};
				hexagon(c, cx, cy, CELL_W, CELL_H, color);
			}
			for (x, label) in &marks {
				text(c, label, *x, SCALE_Y, TextAlign::Center, super::small_font(), 18.0, DIM);
			}
			// The legend: one box per zone, with its colour and range.
			let zones: Vec<(&str, String, crate::color::Rgba)> = {
				let mut z = Vec::new();
				if min < 0.0 {
					z.push(("VACUUM", format!("{} → 0", scale_label(min)), GREEN_TEXT));
				}
				let from = if min < 0.0 { 0.0 } else { min };
				z.push(("NORMAL", format!("{} → {}", scale_label(from), scale_label(warn)), ORANGE));
				z.push(("LIMIT", format!("> {}", scale_label(warn)), RED_TEXT));
				z
			};
			let gap = 10.0;
			let box_w = (w - 48.0 - gap * (zones.len() as f32 - 1.0)) / zones.len() as f32;
			for (i, (name, range, color)) in zones.iter().enumerate() {
				let x = 24.0 + i as f32 * (box_w + gap);
				stroke_rect(c, x, LEGEND_Y, box_w, 50.0, 1.5, *color);
				text(c, name, x + 10.0, LEGEND_Y + 21.0, TextAlign::Left, super::label_font(), 17.0, *color);
				text(c, range, x + 10.0, LEGEND_Y + 42.0, TextAlign::Left, super::small_font(), 15.0, TITLE);
			}
			footer(c, &format!("{} · {}", labels.title, unit), w, h);
		}));
		self.base.static_cache_valid = true;
	}
}

impl Gauge for EvaHoneycombGauge {
	fn base(&self) -> &GaugeBase {
		&self.base
	}

	fn base_mut(&mut self) -> &mut GaugeBase {
		&mut self.base
	}

	fn draw(&mut self, canvas: &mut Canvas) {
		if !self.base.static_cache_valid || self.static_layer.is_none() {
			self.update_static();
		}
		if let Some(s) = &self.static_layer {
			canvas.draw_sprite(s);
		}
		let value = self.base.value;
		for cell in self.cells.iter().filter(|c| c.value <= value) {
			let color = if cell.value < 0.0 {
				GREEN
			} else if cell.value > self.warn {
				RED
			} else {
				ORANGE
			};
			hexagon(canvas, cell.cx, cell.cy, CELL_W, CELL_H, color);
		}
		let color = if value > self.warn {
			RED_TEXT
		} else if value < 0.0 {
			GREEN_TEXT
		} else {
			READOUT
		};
		let w = self.size.0 as f32;
		digits(canvas, &format_value(value, self.decimals), w - 24.0, 140.0, 72.0, TextAlign::Right, color);
	}
}
