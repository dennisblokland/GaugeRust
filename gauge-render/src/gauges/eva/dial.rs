//! `EvaDial`: a 270° ring of segments that fills to the value, ticks and
//! labels outside it, a red zone from `WarnValue` up and a seven-segment
//! readout in the middle. `LIMIT` blinks once the value reaches the zone.

use crate::canvas::{rect, Canvas, Paint, Sprite};
use crate::gauges::eva::{
	chip, digits, footer, format_value, nice_step, polar, scale_label, scale_values, screen_frame, segmented_arc, text,
	warn_value, Blink, Labels, AMBER, DIM, FAINT, ORANGE, READOUT, RED, RED_TEXT, RED_UNLIT, UNLIT, WHITE,
};
use crate::gauges::{draw_background, render_static, Gauge, GaugeBase, GaugeConfig};
use crate::text::TextAlign;

const CX: f32 = 320.0;
const CY: f32 = 262.0;
const START_DEG: f32 = 135.0;
const SWEEP_DEG: f32 = 270.0;
const RING_RADIUS: f32 = 160.0;
const RING_WIDTH: f32 = 24.0;
const SEGMENTS: usize = 60;
const SEGMENT_GAP_DEG: f32 = 1.2;
const TICK_OUTER: f32 = 202.0;
const MAJOR_INNER: f32 = 182.0;
const MINOR_INNER: f32 = 192.0;
const LABEL_RADIUS: f32 = 128.0;
const OUTER_RING: f32 = 212.0;

pub struct EvaDialGauge {
	base: GaugeBase,
	labels: Labels,
	warn: f32,
	decimals: i32,
	size: (u32, u32),
	blink: Blink,
	static_layer: Option<Sprite>,
}

impl EvaDialGauge {
	pub fn new(config: &GaugeConfig, width: u32, height: u32) -> Self {
		Self {
			base: GaugeBase::new(config),
			labels: Labels::new(config),
			warn: warn_value(config, 0.78),
			decimals: config.decimals,
			size: (width, height),
			blink: Blink::new(),
			static_layer: None,
		}
	}

	fn angle(&self, v: f32) -> f32 {
		START_DEG + self.base.fraction(v) * SWEEP_DEG
	}

	/// The value at the far end of segment `i`.
	fn segment_value(&self, i: usize) -> f32 {
		let (min, max) = (self.base.min_value, self.base.max_value);
		min + (max - min) * (i as f32 + 1.0) / SEGMENTS as f32
	}

	/// Labels divide by 1000 on big ranges (an RPM dial reads 0–9).
	fn label_divisor(&self) -> f32 {
		if self.base.max_value.abs() >= 2000.0 {
			1000.0
		} else {
			1.0
		}
	}

	fn update_static(&mut self) {
		let (w, h) = (self.size.0 as f32, self.size.1 as f32);
		let (min, max, warn) = (self.base.min_value, self.base.max_value, self.warn);
		let step = nice_step(max - min, 9.0);
		let majors = scale_values(min, max, step);
		let minors = scale_values(min, max, step / 4.0);
		let divisor = self.label_divisor();
		let angle = |v: f32| self.angle(v);
		let segment_value = |i: usize| self.segment_value(i);
		let labels = self.labels.clone();
		self.static_layer = Some(render_static(self.size.0, self.size.1, |c| {
			draw_background(c);
			screen_frame(c, &labels, w, h);
			if let Some(oval) = rect(CX - OUTER_RING, CY - OUTER_RING, CX + OUTER_RING, CY + OUTER_RING) {
				c.draw_arc(oval, START_DEG, SWEEP_DEG, &Paint::stroke(FAINT, 1.5));
			}
			for v in &minors {
				let (x0, y0) = polar(CX, CY, MINOR_INNER, angle(*v));
				let (x1, y1) = polar(CX, CY, TICK_OUTER - 2.0, angle(*v));
				let color = if *v >= warn { RED } else { DIM };
				c.draw_line(x0, y0, x1, y1, &Paint::stroke(color, 1.5));
			}
			for v in &majors {
				let a = angle(*v);
				let (x0, y0) = polar(CX, CY, MAJOR_INNER, a);
				let (x1, y1) = polar(CX, CY, TICK_OUTER, a);
				let over = *v >= warn;
				c.draw_line(x0, y0, x1, y1, &Paint::stroke(if over { RED } else { ORANGE }, 3.0));
				let (lx, ly) = polar(CX, CY, LABEL_RADIUS, a);
				let label = scale_label(*v / divisor);
				text(
					c,
					&label,
					lx,
					ly + 8.0,
					TextAlign::Center,
					super::small_font(),
					24.0,
					if over { RED_TEXT } else { AMBER },
				);
			}
			segmented_arc(c, CX, CY, RING_RADIUS, RING_WIDTH, START_DEG, SWEEP_DEG, SEGMENTS, SEGMENT_GAP_DEG, |i| {
				Some(if segment_value(i) > warn { RED_UNLIT } else { UNLIT })
			});
			let unit = if labels.unit.is_empty() { labels.source.clone() } else { labels.unit.to_uppercase() };
			text(c, &unit, CX, CY + 52.0, TextAlign::Center, super::label_font(), 18.0, DIM);
			let mut foot = format!("LIMIT {}", scale_label(warn));
			if divisor > 1.0 {
				foot.push_str(" · SCALE ×1000");
			}
			footer(c, &foot, w, h);
		}));
		self.base.static_cache_valid = true;
	}
}

impl Gauge for EvaDialGauge {
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
		let lit = (self.base.fraction(value) * SEGMENTS as f32).round() as usize;
		let warn = self.warn;
		segmented_arc(canvas, CX, CY, RING_RADIUS, RING_WIDTH, START_DEG, SWEEP_DEG, SEGMENTS, SEGMENT_GAP_DEG, |i| {
			(i < lit).then(|| if self.segment_value(i) > warn { RED } else { ORANGE })
		});
		let a = self.angle(value);
		let (x0, y0) = polar(CX, CY, RING_RADIUS - 16.0, a);
		let (x1, y1) = polar(CX, CY, TICK_OUTER + 12.0, a);
		canvas.draw_line(x0, y0, x1, y1, &Paint::stroke(WHITE, 3.0));

		let over = value >= warn;
		let color = if over { RED_TEXT } else { READOUT };
		digits(canvas, &format_value(value, self.decimals), CX, CY + 24.0, 54.0, TextAlign::Center, color);
		if over && self.blink.on(0.8) {
			chip(canvas, CX - 64.0, CY + 96.0, 128.0, 32.0, "LIMIT", Some(RED));
		}
	}
}
