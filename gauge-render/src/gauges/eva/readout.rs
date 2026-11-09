//! `EvaReadout`: a large seven-segment readout in a panel, three zone chips
//! and a scale bar with a marker. On an AFR channel the zones read
//! RICH / STOICH / LEAN (14.2 and 15.2 unless `WarnLowValue` / `WarnValue`
//! say otherwise) and the panel shows lambda; on others LOW / NORMAL / HIGH.

use crate::canvas::{Canvas, Sprite};
use crate::color::Rgba;
use crate::gauges::eva::{
	chip, digits, fill_rect, footer, format_value, panel, scale_label, screen_frame, segment_bar, text, warn_low_value,
	warn_value, Labels, BLUE, DIM, GREEN, GREEN_TEXT, ORANGE, READOUT, RED, RED_TEXT, WHITE,
};
use crate::gauges::{draw_background, render_static, Gauge, GaugeBase, GaugeConfig};
use crate::text::TextAlign;

const PANEL_X: f32 = 24.0;
const PANEL_Y: f32 = 66.0;
const PANEL_H: f32 = 196.0;
const CHIPS_Y: f32 = 284.0;
const BAR_Y: f32 = 350.0;
const BAR_H: f32 = 16.0;
const BAR_SEGMENTS: usize = 64;
const STOICH: f32 = 14.7;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Zone {
	Low,
	Mid,
	High,
}

pub struct EvaReadoutGauge {
	base: GaugeBase,
	labels: Labels,
	low: f32,
	high: f32,
	afr: bool,
	decimals: i32,
	size: (u32, u32),
	static_layer: Option<Sprite>,
}

impl EvaReadoutGauge {
	pub fn new(config: &GaugeConfig, width: u32, height: u32) -> Self {
		let afr = config.data_source.to_ascii_lowercase().starts_with("afr");
		let (low, high) = if afr {
			(config.warn_low_value.unwrap_or(14.2), config.warn_value.unwrap_or(15.2))
		} else {
			(warn_low_value(config, 0.25), warn_value(config, 0.75))
		};
		Self {
			base: GaugeBase::new(config),
			labels: Labels::new(config),
			low,
			high,
			afr,
			decimals: if afr { config.decimals.max(1) } else { config.decimals },
			size: (width, height),
			static_layer: None,
		}
	}

	fn zone(&self, v: f32) -> Zone {
		if v < self.low {
			Zone::Low
		} else if v <= self.high {
			Zone::Mid
		} else {
			Zone::High
		}
	}

	fn names(&self) -> [&'static str; 3] {
		if self.afr {
			["RICH", "STOICH", "LEAN"]
		} else {
			["LOW", "NORMAL", "HIGH"]
		}
	}

	/// Chip and bar colour per zone. Rich is orange and lean blue (the
	/// colours of a wideband display); elsewhere high is the alarm.
	fn color(&self, zone: Zone) -> Rgba {
		match (self.afr, zone) {
			(_, Zone::Mid) => GREEN,
			(true, Zone::Low) => ORANGE,
			(true, Zone::High) => BLUE,
			(false, Zone::Low) => BLUE,
			(false, Zone::High) => RED,
		}
	}

	fn text_color(&self, zone: Zone) -> Rgba {
		match (self.afr, zone) {
			(_, Zone::Mid) => GREEN_TEXT,
			(true, Zone::Low) => READOUT,
			(false, Zone::Low) => BLUE,
			(true, Zone::High) => BLUE,
			(false, Zone::High) => RED_TEXT,
		}
	}

	fn bar_x(&self, v: f32) -> f32 {
		let w = self.size.0 as f32 - 48.0;
		24.0 + self.base.fraction(v) * w
	}

	fn update_static(&mut self) {
		let (w, h) = (self.size.0 as f32, self.size.1 as f32);
		let (min, max) = (self.base.min_value, self.base.max_value);
		let labels = self.labels.clone();
		let tab = if labels.unit.is_empty() { "READOUT".to_string() } else { labels.unit.to_uppercase() };
		let bar_colors: Vec<Rgba> = (0..BAR_SEGMENTS)
			.map(|i| {
				let v = min + (max - min) * (i as f32 + 0.5) / BAR_SEGMENTS as f32;
				let c = self.color(self.zone(v));
				Rgba::rgb(c.r / 2, c.g / 2, c.b / 2)
			})
			.collect();
		let mut marks = vec![min, self.low, self.high, max];
		if self.afr && STOICH > min && STOICH < max {
			marks.push(STOICH);
		}
		let marks: Vec<(f32, String)> = marks.into_iter().map(|v| (self.bar_x(v), scale_label(v))).collect();
		self.static_layer = Some(render_static(self.size.0, self.size.1, |c| {
			draw_background(c);
			screen_frame(c, &labels, w, h);
			panel(c, PANEL_X, PANEL_Y, w - 48.0, PANEL_H, ORANGE, &tab, None);
			segment_bar(c, 24.0, BAR_Y, w - 48.0, BAR_H, BAR_SEGMENTS, 3.0, false, |i| Some(bar_colors[i]));
			for (x, label) in &marks {
				let x = x.clamp(36.0, w - 36.0);
				text(c, label, x, BAR_Y + BAR_H + 26.0, TextAlign::Center, super::small_font(), 17.0, DIM);
			}
			footer(c, &format!("{} · {}", labels.title, tab), w, h);
		}));
		self.base.static_cache_valid = true;
	}
}

impl Gauge for EvaReadoutGauge {
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
		let w = self.size.0 as f32;
		let value = self.base.value;
		let zone = self.zone(value);
		let s = format_value(value, self.decimals);
		digits(canvas, &s, w / 2.0, PANEL_Y + PANEL_H - 32.0, 132.0, TextAlign::Center, self.text_color(zone));
		if self.afr {
			let lambda = format!("LAMBDA {}", format_value(value / STOICH, 2));
			text(canvas, &lambda, w - 36.0, PANEL_Y + 22.0, TextAlign::Right, super::small_font(), 18.0, DIM);
		}

		let gap = 10.0;
		let chip_w = (w - 48.0 - 2.0 * gap) / 3.0;
		for (i, (z, name)) in [Zone::Low, Zone::Mid, Zone::High].into_iter().zip(self.names()).enumerate() {
			let on = (z == zone).then(|| self.color(z));
			chip(canvas, 24.0 + i as f32 * (chip_w + gap), CHIPS_Y, chip_w, 42.0, name, on);
		}
		let x = self.bar_x(value);
		fill_rect(canvas, x - 2.0, BAR_Y - 9.0, 4.0, BAR_H + 18.0, WHITE);
	}
}
