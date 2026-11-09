//! `EvaGear`: the gear as one large digit in a panel, with a column of every
//! position beside it and the current one lit. 0 reads N and anything below
//! zero R. `MaxValue` from 1 to 10 sets the number of forward gears (else 6).

use crate::canvas::{Canvas, Sprite};
use crate::color::Rgba;
use crate::gauges::eva::{chip, footer, panel, screen_frame, text, Labels, GHOST, GREEN, ORANGE, RED, RED_TEXT};
use crate::gauges::{draw_background, render_static, Gauge, GaugeBase, GaugeConfig};
use crate::text::{typeface, TextAlign};

const PANEL_X: f32 = 40.0;
const PANEL_Y: f32 = 70.0;
const PANEL_W: f32 = 400.0;
const PANEL_H: f32 = 360.0;
const COLUMN_X: f32 = 472.0;
const COLUMN_W: f32 = 140.0;
const GEAR_COLOR: Rgba = Rgba::rgb(0xFF, 0xB0, 0x00);

pub struct EvaGearGauge {
	base: GaugeBase,
	labels: Labels,
	gears: i32,
	size: (u32, u32),
	static_layer: Option<Sprite>,
}

/// The label for a gear position.
fn gear_label(gear: i32) -> String {
	match gear {
		g if g < 0 => "R".into(),
		0 => "N".into(),
		g => g.to_string(),
	}
}

fn gear_color(gear: i32) -> Rgba {
	match gear {
		g if g < 0 => RED,
		0 => GREEN,
		_ => ORANGE,
	}
}

impl EvaGearGauge {
	pub fn new(config: &GaugeConfig, width: u32, height: u32) -> Self {
		let gears = if (1.0..=10.0).contains(&config.max_value) { config.max_value as i32 } else { 6 };
		Self {
			base: GaugeBase::new(config),
			labels: Labels::new(config),
			gears,
			size: (width, height),
			static_layer: None,
		}
	}

	/// Every position from R to the top gear.
	fn positions(&self) -> Vec<i32> {
		(-1..=self.gears).collect()
	}

	fn chip_rect(&self, index: usize) -> (f32, f32, f32, f32) {
		let n = self.positions().len() as f32;
		let gap = 6.0;
		let h = ((PANEL_H - gap * (n - 1.0)) / n).min(48.0);
		(COLUMN_X, PANEL_Y + index as f32 * (h + gap), COLUMN_W, h)
	}

	fn update_static(&mut self) {
		let (w, h) = (self.size.0 as f32, self.size.1 as f32);
		let labels = self.labels.clone();
		let chips: Vec<_> = self.positions().into_iter().enumerate().map(|(i, g)| (self.chip_rect(i), g)).collect();
		self.static_layer = Some(render_static(self.size.0, self.size.1, |c| {
			draw_background(c);
			screen_frame(c, &labels, w, h);
			panel(c, PANEL_X, PANEL_Y, PANEL_W, PANEL_H, ORANGE, "POSITION", None);
			for ((x, y, cw, ch), g) in chips {
				chip(c, x, y, cw, ch, &gear_label(g), None);
			}
			footer(c, "N = 0 · R < 0", w, h);
		}));
		self.base.static_cache_valid = true;
	}
}

impl Gauge for EvaGearGauge {
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
		let gear = self.base.value.round() as i32;
		let gear = gear.clamp(-1, self.gears);
		let font = typeface("DSEG14 Classic");
		let (cx, baseline) = (PANEL_X + PANEL_W / 2.0, PANEL_Y + PANEL_H - 50.0);
		text(canvas, "8", cx, baseline, TextAlign::Center, font, 260.0, GHOST);
		let color = match gear {
			g if g < 0 => RED_TEXT,
			0 => GREEN,
			_ => GEAR_COLOR,
		};
		text(canvas, &gear_label(gear), cx, baseline, TextAlign::Center, font, 260.0, color);
		if let Some(i) = self.positions().iter().position(|g| *g == gear) {
			let (x, y, w, h) = self.chip_rect(i);
			chip(canvas, x, y, w, h, &gear_label(gear), Some(gear_color(gear)));
		}
	}
}
