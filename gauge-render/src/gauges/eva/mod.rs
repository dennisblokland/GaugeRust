//! EVA-style gauges: black screens, orange line work, segmented displays and
//! kanji labels. Five gauge types share the palette, the screen frame and the
//! drawing widgets in this module:
//!
//! | Type | Shows |
//! |------|-------|
//! | `EvaDial` | 270° segmented arc with ticks, a limit zone and a centre readout |
//! | `EvaHoneycomb` | Rows of hexagon cells that light up to the value |
//! | `EvaReadout` | Large seven-segment readout, three zone chips and a scale bar |
//! | `EvaWarning` | Vertical segment bar with a WARNING / NOMINAL badge |
//! | `EvaGear` | Large gear digit with a column of gear positions |
//!
//! `WarnValue` sets the limit (redline, overboost, overheat); without it each
//! gauge takes a share of the `MinValue`–`MaxValue` range. `ColorHex` is
//! ignored: the palette is fixed.

mod dial;
mod gear;
mod honeycomb;
mod readout;
mod warning;

pub use dial::EvaDialGauge;
pub use gear::EvaGearGauge;
pub use honeycomb::EvaHoneycombGauge;
pub use readout::EvaReadoutGauge;
pub use warning::EvaWarningGauge;

use std::time::Instant;

use tiny_skia::{LineCap, PathBuilder};

use crate::canvas::{arc_path, rect, Canvas, Paint};
use crate::color::Rgba;
use crate::format::format_number;
use crate::gauges::GaugeConfig;
use crate::text::{jp_typeface, typeface, TextAlign, Typeface};

pub(crate) const ORANGE: Rgba = Rgba::rgb(0xFF, 0x7A, 0x1A);
pub(crate) const TITLE: Rgba = Rgba::rgb(0xFF, 0x8A, 0x2A);
pub(crate) const AMBER: Rgba = Rgba::rgb(0xFF, 0xB2, 0x4A);
pub(crate) const READOUT: Rgba = Rgba::rgb(0xFF, 0xD0, 0x8A);
pub(crate) const DIM: Rgba = Rgba::rgb(0xC7, 0x74, 0x2C);
pub(crate) const FAINT: Rgba = Rgba::rgb(0x5A, 0x2E, 0x0C);
pub(crate) const UNLIT: Rgba = Rgba::rgb(0x24, 0x14, 0x0A);
pub(crate) const GHOST: Rgba = Rgba::rgb(0x2A, 0x17, 0x0A);
pub(crate) const RED: Rgba = Rgba::rgb(0xFF, 0x2D, 0x1F);
pub(crate) const RED_TEXT: Rgba = Rgba::rgb(0xFF, 0x3B, 0x2B);
pub(crate) const RED_UNLIT: Rgba = Rgba::rgb(0x2A, 0x0B, 0x08);
pub(crate) const GREEN: Rgba = Rgba::rgb(0x39, 0xE8, 0x6A);
pub(crate) const GREEN_TEXT: Rgba = Rgba::rgb(0x5C, 0xF0, 0x8A);
pub(crate) const GREEN_UNLIT: Rgba = Rgba::rgb(0x0F, 0x22, 0x16);
pub(crate) const BLUE: Rgba = Rgba::rgb(0x4A, 0x9B, 0xFF);
pub(crate) const BLUE_UNLIT: Rgba = Rgba::rgb(0x0B, 0x16, 0x26);
pub(crate) const INK: Rgba = Rgba::rgb(0x08, 0x07, 0x06);
pub(crate) const WHITE: Rgba = Rgba::rgb(0xFF, 0xF4, 0xE0);

/// Labels and titles.
pub(crate) fn label_font() -> &'static Typeface {
	typeface("Barlow Condensed")
}

/// Small print: scale labels, footers.
pub(crate) fn small_font() -> &'static Typeface {
	typeface("Saira Extra Condensed")
}

/// Seven-segment readouts.
pub(crate) fn digit_font() -> &'static Typeface {
	typeface("DSEG7 Classic")
}

/// English title and kanji for the ECU channels. The kanji are the only
/// characters in the bundled Japanese subset font.
const CHANNELS: &[(&str, &str, &str)] = &[
	("Rpm", "ENGINE SPEED", "回転数"),
	("Map", "BOOST", "過給圧"),
	("AfrCurr1", "AIR / FUEL", "空燃比"),
	("AfrCurr2", "AIR / FUEL 2", "空燃比"),
	("AfrTarget", "AFR TARGET", "空燃比"),
	("Clt", "COOLANT", "水温"),
	("OilTemp", "OIL TEMP", "油温"),
	("OilPressure", "OIL PRESSURE", "油圧"),
	("Iat", "INTAKE TEMP", "吸気温"),
	("Vbat", "BATTERY", "電圧"),
	("BatteryVoltage", "BATTERY", "電圧"),
	("GearPos", "GEAR", "変速"),
	("VehicleSpeed", "SPEED", "速度"),
	("ThrottlePosition", "PEDAL", "踏込"),
	("IgnAdvAngle", "IGNITION", "点火時期"),
	("PriInjDuty", "INJECTOR DUTY", "噴射"),
	("InjectorDuty", "INJECTOR DUTY", "噴射"),
	("Egt1", "EXHAUST TEMP 1", "排気温"),
	("Egt2", "EXHAUST TEMP 2", "排気温"),
	("FuelPressure", "FUEL PRESSURE", "燃圧"),
	("FuelTemp", "FUEL TEMP", "燃温"),
];

fn channel(source: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
	CHANNELS.iter().find(|(name, ..)| crate::eq_ignore_case(name, source))
}

/// The screen's labels: `Title` (or the channel's English name), its kanji
/// and the data source shown at the top right.
#[derive(Clone, Debug)]
pub(crate) struct Labels {
	pub title: String,
	pub kanji: Option<&'static str>,
	pub source: String,
	pub unit: String,
}

impl Labels {
	pub fn new(config: &GaugeConfig) -> Self {
		let known = channel(&config.data_source);
		let title = if !config.title.trim().is_empty() {
			config.title.trim().to_uppercase()
		} else {
			known.map_or_else(|| config.data_source.to_uppercase(), |c| c.1.to_string())
		};
		Self { title, kanji: known.map(|c| c.2), source: config.data_source.to_uppercase(), unit: config.unit.clone() }
	}
}

/// The limit: `WarnValue`, else `share` of the way from `MinValue` to `MaxValue`.
pub(crate) fn warn_value(config: &GaugeConfig, share: f32) -> f32 {
	config.warn_value.unwrap_or(config.min_value + (config.max_value - config.min_value) * share)
}

/// The lower zone edge: `WarnLowValue`, else `share` of the range.
pub(crate) fn warn_low_value(config: &GaugeConfig, share: f32) -> f32 {
	config.warn_low_value.unwrap_or(config.min_value + (config.max_value - config.min_value) * share)
}

/// `value` with `decimals` places (`F0`, `F1`, ...).
pub(crate) fn format_value(value: f32, decimals: i32) -> String {
	format_number(value, &format!("F{}", decimals.clamp(0, 6)))
}

/// A scale label without trailing zeros (`7`, `2.5`, `-1`).
pub(crate) fn scale_label(value: f32) -> String {
	format_number(value, "0.##")
}

/// A round step that splits `range` into about `target` parts (1, 2, 2.5 or 5 × 10ⁿ).
pub(crate) fn nice_step(range: f32, target: f32) -> f32 {
	if !(range.is_finite() && range > 0.0) {
		return 1.0;
	}
	let raw = range / target.max(1.0);
	let magnitude = 10f32.powf(raw.log10().floor());
	[1.0, 2.0, 2.5, 5.0, 10.0].into_iter().map(|m| m * magnitude).find(|s| *s >= raw).unwrap_or(10.0 * magnitude)
}

/// Every multiple of `step` from `min` to `max` (inclusive, with a little tolerance).
pub(crate) fn scale_values(min: f32, max: f32, step: f32) -> Vec<f32> {
	let first = (min / step).ceil() as i32;
	let last = (max / step + 1e-3).floor() as i32;
	(first..=last).map(|i| i as f32 * step).collect()
}

/// A steady on/off rhythm for warnings, from when the gauge was created.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Blink(Instant);

impl Blink {
	pub fn new() -> Self {
		Self(Instant::now())
	}

	/// On for the first 60 % of each `period` seconds.
	pub fn on(&self, period: f32) -> bool {
		let t = self.0.elapsed().as_secs_f32() / period;
		t.fract() < 0.6
	}
}

pub(crate) fn fill_rect(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
	if let Some(r) = rect(x, y, x + w, y + h) {
		c.draw_rect(r, &Paint::fill(color));
	}
}

pub(crate) fn stroke_rect(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, width: f32, color: Rgba) {
	if let Some(r) = rect(x, y, x + w, y + h) {
		c.draw_rect(r, &Paint::stroke(color, width));
	}
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn text(c: &mut Canvas, s: &str, x: f32, y: f32, align: TextAlign, font: &Typeface, size: f32, color: Rgba) {
	c.draw_text(s, x, y, align, font, size, &Paint::fill(color));
}

/// Closed polygon through `points`.
fn polygon(c: &mut Canvas, points: &[(f32, f32)], paint: &Paint) {
	let mut pb = PathBuilder::new();
	for (i, &(x, y)) in points.iter().enumerate() {
		if i == 0 {
			pb.move_to(x, y);
		} else {
			pb.line_to(x, y);
		}
	}
	pb.close();
	if let Some(path) = pb.finish() {
		c.draw_path(&path, paint);
	}
}

/// A pointy-top hexagon `w` wide and `h` tall around (`cx`, `cy`).
pub(crate) fn hexagon(c: &mut Canvas, cx: f32, cy: f32, w: f32, h: f32, color: Rgba) {
	let (hw, hh, q) = (w / 2.0, h / 2.0, h / 4.0);
	let points =
		[(cx, cy - hh), (cx + hw, cy - q), (cx + hw, cy + q), (cx, cy + hh), (cx - hw, cy + q), (cx - hw, cy - q)];
	polygon(c, &points, &Paint::fill(color));
}

/// A wide badge with pointed ends (a flat hexagon) in the box `x, y, w, h`.
pub(crate) fn badge(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, paint: &Paint) {
	let inset = w * 0.08;
	let points = [
		(x + inset, y),
		(x + w - inset, y),
		(x + w, y + h / 2.0),
		(x + w - inset, y + h),
		(x + inset, y + h),
		(x, y + h / 2.0),
	];
	polygon(c, &points, paint);
}

/// L-shaped corner marks just outside the box `x, y, w, h`.
pub(crate) fn corner_brackets(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, len: f32, color: Rgba) {
	let p = Paint::stroke(color, 2.0).with_cap(LineCap::Square);
	let (r, b) = (x + w, y + h);
	for (cx, cy, dx, dy) in [(x, y, 1.0, 1.0), (r, y, -1.0, 1.0), (x, b, 1.0, -1.0), (r, b, -1.0, -1.0)] {
		c.draw_line(cx, cy, cx + dx * len, cy, &p);
		c.draw_line(cx, cy, cx, cy + dy * len, &p);
	}
}

/// The title strip and corner marks every EVA screen shares: an orange block,
/// the title, its kanji, a rule and the data source at the right.
pub(crate) fn screen_frame(c: &mut Canvas, labels: &Labels, w: f32, h: f32) {
	fill_rect(c, 20.0, 16.0, 8.0, 22.0, ORANGE);
	let mut x = 38.0;
	text(c, &labels.title, x, 35.0, TextAlign::Left, label_font(), 24.0, TITLE);
	x += label_font().measure(&labels.title, 24.0) + 12.0;
	if let Some(k) = labels.kanji {
		text(c, k, x, 35.0, TextAlign::Left, jp_typeface(), 21.0, TITLE);
		x += jp_typeface().measure(k, 21.0) + 12.0;
	}
	let tag_size = 14.0;
	let tag_w = small_font().measure(&labels.source, tag_size);
	let rule_end = w - 20.0 - tag_w - 12.0;
	if rule_end > x {
		c.draw_line(x, 27.0, rule_end, 27.0, &Paint::stroke(FAINT, 1.0));
	}
	text(c, &labels.source, w - 20.0, 33.0, TextAlign::Right, small_font(), tag_size, DIM);
	corner_brackets(c, 8.0, 8.0, w - 16.0, h - 16.0, 16.0, ORANGE);
}

/// A big heading at (`x`, `baseline`): the kanji when the channel has one,
/// else the English title.
pub(crate) fn jp_or_title(c: &mut Canvas, labels: &Labels, x: f32, baseline: f32, size: f32) {
	match labels.kanji {
		Some(k) => text(c, k, x, baseline, TextAlign::Left, jp_typeface(), size, TITLE),
		None => text(c, &labels.title, x, baseline, TextAlign::Left, label_font(), size * 0.8, TITLE),
	}
}

/// Small centred print along the bottom of the screen.
pub(crate) fn footer(c: &mut Canvas, s: &str, w: f32, h: f32) {
	text(c, s, w / 2.0, h - 16.0, TextAlign::Center, small_font(), 15.0, DIM);
}

/// A bordered panel with a filled tab at its top-left corner holding `label`
/// (and `kanji`), plus corner marks.
#[allow(clippy::too_many_arguments)]
pub(crate) fn panel(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, color: Rgba, label: &str, kanji: Option<&str>) {
	stroke_rect(c, x, y, w, h, 2.0, color);
	let size = 16.0;
	let mut tab_w = label_font().measure(label, size) + 20.0;
	if let Some(k) = kanji {
		tab_w += jp_typeface().measure(k, 15.0) + 8.0;
	}
	fill_rect(c, x - 1.0, y - 1.0, tab_w, 24.0, color);
	text(c, label, x + 9.0, y + 17.0, TextAlign::Left, label_font(), size, INK);
	if let Some(k) = kanji {
		let kx = x + 9.0 + label_font().measure(label, size) + 8.0;
		text(c, k, kx, y + 17.0, TextAlign::Left, jp_typeface(), 15.0, INK);
	}
	corner_brackets(c, x - 8.0, y - 8.0, w + 16.0, h + 16.0, 12.0, AMBER);
}

/// A label box: filled with `on` (dark text) when lit, else a faint outline.
pub(crate) fn chip(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, label: &str, on: Option<Rgba>) {
	let size = (h * 0.55).min(24.0);
	let baseline = y + h / 2.0 + size * 0.36;
	match on {
		Some(color) => {
			fill_rect(c, x, y, w, h, color);
			text(c, label, x + w / 2.0, baseline, TextAlign::Center, label_font(), size, INK);
		}
		None => {
			stroke_rect(c, x, y, w, h, 2.0, FAINT);
			text(c, label, x + w / 2.0, baseline, TextAlign::Center, label_font(), size, FAINT);
		}
	}
}

/// `count` segments in the box `x, y, w, h`, `gap` px apart. Segment 0 is the
/// left one, or the bottom one when `vertical`. `color` picks each segment's
/// colour; `None` skips it (so unlit segments can live in a static layer).
#[allow(clippy::too_many_arguments)]
pub(crate) fn segment_bar(
	c: &mut Canvas,
	x: f32,
	y: f32,
	w: f32,
	h: f32,
	count: usize,
	gap: f32,
	vertical: bool,
	color: impl Fn(usize) -> Option<Rgba>,
) {
	if count == 0 {
		return;
	}
	let n = count as f32;
	for i in 0..count {
		let Some(col) = color(i) else { continue };
		if vertical {
			let seg = (h - gap * (n - 1.0)) / n;
			let top = y + h - (i as f32 + 1.0) * seg - i as f32 * gap;
			fill_rect(c, x, top, w, seg, col);
		} else {
			let seg = (w - gap * (n - 1.0)) / n;
			fill_rect(c, x + i as f32 * (seg + gap), y, seg, h, col);
		}
	}
}

/// A ring of `count` arc segments around (`cx`, `cy`) from `start_deg`
/// sweeping `sweep_deg` (clockwise, 0° = 3 o'clock), `gap_deg` apart.
/// `color` works as in [`segment_bar`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn segmented_arc(
	c: &mut Canvas,
	cx: f32,
	cy: f32,
	radius: f32,
	width: f32,
	start_deg: f32,
	sweep_deg: f32,
	count: usize,
	gap_deg: f32,
	color: impl Fn(usize) -> Option<Rgba>,
) {
	let Some(oval) = rect(cx - radius, cy - radius, cx + radius, cy + radius) else { return };
	let n = count as f32;
	let seg = (sweep_deg - gap_deg * (n - 1.0)) / n;
	for i in 0..count {
		let Some(col) = color(i) else { continue };
		let a = start_deg + i as f32 * (seg + gap_deg);
		if let Some(path) = arc_path(oval, a, seg) {
			c.draw_path(&path, &Paint::stroke(col, width).with_cap(LineCap::Butt));
		}
	}
}

/// A point `radius` from (`cx`, `cy`) at `deg` (0° = 3 o'clock, clockwise).
pub(crate) fn polar(cx: f32, cy: f32, radius: f32, deg: f32) -> (f32, f32) {
	let a = deg.to_radians();
	(cx + radius * a.cos(), cy + radius * a.sin())
}

/// The unlit look of a seven-segment string: every digit becomes `8`.
pub(crate) fn ghost(s: &str) -> String {
	s.chars().map(|ch| if ch == '.' || ch == ':' { ch } else { '8' }).collect()
}

/// A seven-segment readout over its unlit segments.
pub(crate) fn digits(c: &mut Canvas, s: &str, x: f32, baseline: f32, size: f32, align: TextAlign, color: Rgba) {
	let font = digit_font();
	text(c, &ghost(s), x, baseline, align, font, size, GHOST);
	text(c, s, x, baseline, align, font, size, color);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_kanji_label_is_in_the_subset_font() {
		let jp = jp_typeface();
		for (name, _, kanji) in CHANNELS {
			assert!(kanji.chars().all(|c| jp.has_glyph(c)), "{name}: {kanji}");
		}
		for word in ["警告", "正常"] {
			assert!(word.chars().all(|c| jp.has_glyph(c)), "{word}");
		}
	}

	#[test]
	fn nice_steps() {
		assert_eq!(nice_step(9000.0, 9.0), 1000.0);
		assert_eq!(nice_step(3.0, 6.0), 0.5);
		assert_eq!(nice_step(10.0, 4.0), 2.5);
		assert_eq!(scale_values(-1.0, 2.0, 0.5), vec![-1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0]);
		assert_eq!(scale_label(2.5), "2.5");
		assert_eq!(scale_label(7.0), "7");
	}

	#[test]
	fn labels_come_from_the_channel() {
		let config = GaugeConfig { data_source: "rpm".into(), title: String::new(), ..GaugeConfig::default() };
		let labels = Labels::new(&config);
		assert_eq!((labels.title.as_str(), labels.kanji), ("ENGINE SPEED", Some("回転数")));
		let config = GaugeConfig { data_source: "Gpt1".into(), title: "Gear oil".into(), ..GaugeConfig::default() };
		let labels = Labels::new(&config);
		assert_eq!((labels.title.as_str(), labels.kanji), ("GEAR OIL", None));
	}

	#[test]
	fn ghost_keeps_the_decimal_point() {
		assert_eq!(ghost("-0.45"), "88.88");
		assert_eq!(ghost("14.7"), "88.8");
	}
}
