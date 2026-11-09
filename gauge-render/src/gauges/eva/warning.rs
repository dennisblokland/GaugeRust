//! `EvaWarning`: a vertical segment bar (blue, green, orange, red as the
//! value climbs) beside a badge. At or above `WarnValue` the badge reads
//! 警告 WARNING in red and blinks; below it, 正常 NOMINAL in green.

use crate::canvas::{Canvas, Paint, Sprite};
use crate::color::Rgba;
use crate::gauges::eva::{
	badge, digits, footer, format_value, scale_label, screen_frame, segment_bar, text, warn_value, Blink, Labels, BLUE,
	BLUE_UNLIT, DIM, GREEN, GREEN_TEXT, GREEN_UNLIT, INK, ORANGE, RED, RED_TEXT, RED_UNLIT, UNLIT,
};
use crate::gauges::{draw_background, render_static, Gauge, GaugeBase, GaugeConfig};
use crate::text::{jp_typeface, TextAlign};

const BAR_X: f32 = 30.0;
const BAR_Y: f32 = 62.0;
const BAR_W: f32 = 54.0;
const BAR_H: f32 = 376.0;
const SEGMENTS: usize = 20;
const SCALE_LABELS: usize = 5;
const RIGHT_X: f32 = 150.0;
const BADGE_Y: f32 = 70.0;
const BADGE_H: f32 = 164.0;
const DARK_GREEN: Rgba = Rgba::rgb(0x0C, 0x1F, 0x12);
const DARK_RED: Rgba = Rgba::rgb(0x4A, 0x0D, 0x0A);

pub struct EvaWarningGauge {
	base: GaugeBase,
	labels: Labels,
	warn: f32,
	decimals: i32,
	size: (u32, u32),
	blink: Blink,
	static_layer: Option<Sprite>,
}

impl EvaWarningGauge {
	pub fn new(config: &GaugeConfig, width: u32, height: u32) -> Self {
		Self {
			base: GaugeBase::new(config),
			labels: Labels::new(config),
			warn: warn_value(config, 0.8),
			decimals: config.decimals,
			size: (width, height),
			blink: Blink::new(),
			static_layer: None,
		}
	}

	/// The value in the middle of segment `i` (0 = bottom).
	fn segment_value(&self, i: usize) -> f32 {
		let (min, max) = (self.base.min_value, self.base.max_value);
		min + (max - min) * (i as f32 + 0.5) / SEGMENTS as f32
	}

	/// `(lit, unlit)` colours for a segment standing for `v`: red from the
	/// limit, orange in the band just below it, green, then blue when cold.
	fn segment_colors(&self, v: f32) -> (Rgba, Rgba) {
		let range = self.base.max_value - self.base.min_value;
		if v >= self.warn {
			(RED, RED_UNLIT)
		} else if v >= self.warn - range * 0.15 {
			(ORANGE, UNLIT)
		} else if v >= self.base.min_value + range * 0.35 {
			(GREEN, GREEN_UNLIT)
		} else {
			(BLUE, BLUE_UNLIT)
		}
	}

	fn update_static(&mut self) {
		let (w, h) = (self.size.0 as f32, self.size.1 as f32);
		let (min, max) = (self.base.min_value, self.base.max_value);
		let labels = self.labels.clone();
		let unlit: Vec<Rgba> = (0..SEGMENTS).map(|i| self.segment_colors(self.segment_value(i)).1).collect();
		let unit = labels.unit.clone();
		let warn = self.warn;
		self.static_layer = Some(render_static(self.size.0, self.size.1, |c| {
			draw_background(c);
			screen_frame(c, &labels, w, h);
			super::draw_logo(c, &labels, 500.0, 296.0, 116.0, 116.0);
			segment_bar(c, BAR_X, BAR_Y, BAR_W, BAR_H, SEGMENTS, 3.0, true, |i| Some(unlit[i]));
			for i in 0..SCALE_LABELS {
				let t = i as f32 / (SCALE_LABELS - 1) as f32;
				let v = max - (max - min) * t;
				let y = BAR_Y + 12.0 + (BAR_H - 12.0) * t;
				text(c, &scale_label(v), BAR_X + BAR_W + 10.0, y, TextAlign::Left, super::small_font(), 15.0, DIM);
			}
			footer(c, &format!("LIMIT {} {}", scale_label(warn), unit), w, h);
		}));
		self.base.static_cache_valid = true;
	}
}

impl Gauge for EvaWarningGauge {
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
		let alarm = self.base.value >= self.warn;
		super::status_line(canvas, self.size.1 as f32, alarm, self.blink.on(0.9));
		let w = self.size.0 as f32;
		let value = self.base.value;
		segment_bar(canvas, BAR_X, BAR_Y, BAR_W, BAR_H, SEGMENTS, 3.0, true, |i| {
			let v = self.segment_value(i);
			(v <= value).then(|| self.segment_colors(v).0)
		});

		let warn = value >= self.warn;
		let badge_w = w - 28.0 - RIGHT_X;
		let cx = RIGHT_X + badge_w / 2.0;
		let (kanji, word, ink, line, line_color, value_color) = if warn {
			let lit = self.blink.on(0.9);
			badge(canvas, RIGHT_X, BADGE_Y, badge_w, BADGE_H, &Paint::fill(if lit { RED } else { DARK_RED }));
			("警告", "WARNING", if lit { INK } else { RED_TEXT }, "LIMIT EXCEEDED", RED_TEXT, RED_TEXT)
		} else {
			badge(canvas, RIGHT_X, BADGE_Y, badge_w, BADGE_H, &Paint::fill(GREEN));
			badge(canvas, RIGHT_X + 4.0, BADGE_Y + 4.0, badge_w - 8.0, BADGE_H - 8.0, &Paint::fill(DARK_GREEN));
			("正常", "NOMINAL", GREEN, "WITHIN RANGE", GREEN_TEXT, GREEN_TEXT)
		};
		text(canvas, kanji, cx, BADGE_Y + 104.0, TextAlign::Center, jp_typeface(), 84.0, ink);
		text(canvas, word, cx, BADGE_Y + 144.0, TextAlign::Center, super::label_font(), 28.0, ink);

		let title = format!("{} · {line}", self.labels.title);
		text(canvas, &title, RIGHT_X, 280.0, TextAlign::Left, super::label_font(), 22.0, line_color);
		let s = format_value(value, self.decimals);
		digits(canvas, &s, RIGHT_X, 404.0, 104.0, TextAlign::Left, value_color);
		if !self.labels.unit.is_empty() {
			let x = RIGHT_X + super::digit_font().measure(&s, 104.0) + 14.0;
			text(canvas, &self.labels.unit, x, 404.0, TextAlign::Left, super::label_font(), 30.0, value_color);
		}
	}
}
