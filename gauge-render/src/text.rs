//! Font loading, measuring and glyph-outline generation.
//!
//! The bundled fonts in `gauge-render/fonts` are embedded into the
//! binary, so the renderer does not depend on a `fonts/` folder at runtime.

use std::sync::OnceLock;

use ab_glyph::{Font as _, FontRef, OutlineCurve};
use tiny_skia::{Path, PathBuilder};

/// Horizontal text alignment relative to the x coordinate (`SKTextAlign`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
	Left,
	Center,
	Right,
}

/// A loaded typeface (`SKTypeface`).
pub struct Typeface {
	name: &'static str,
	font: FontRef<'static>,
	units_per_em: f32,
}

/// Skia-style vertical metrics in pixels: `ascent` is negative (above the
/// baseline), `descent` positive, like `SKFontMetrics`.
#[derive(Clone, Copy, Debug)]
pub struct FontMetrics {
	pub ascent: f32,
	pub descent: f32,
}

impl Typeface {
	fn load(name: &'static str, data: &'static [u8]) -> Self {
		let font = FontRef::try_from_slice(data).expect("bundled font must parse");
		let units_per_em = font.units_per_em().unwrap_or(1000.0);
		Self { name, font, units_per_em }
	}

	pub fn name(&self) -> &'static str {
		self.name
	}

	pub fn metrics(&self, size: f32) -> FontMetrics {
		let s = size / self.units_per_em;
		FontMetrics { ascent: -self.font.ascent_unscaled() * s, descent: -self.font.descent_unscaled() * s }
	}

	/// Advance width of `text` at `size` px, like `SKFont.MeasureText`
	/// (no kerning, matching Skia's unshaped `DrawText`).
	/// Whether the font has a glyph for `c` (not just `.notdef`).
	pub fn has_glyph(&self, c: char) -> bool {
		self.font.glyph_id(c).0 != 0
	}

	pub fn measure(&self, text: &str, size: f32) -> f32 {
		let s = size / self.units_per_em;
		text.chars().map(|c| self.font.h_advance_unscaled(self.font.glyph_id(c)) * s).sum()
	}

	/// Builds the filled outline of `text` with its baseline at `y` and the
	/// given alignment around `x`. Returns `None` for blank text.
	pub fn text_path(&self, text: &str, x: f32, y: f32, size: f32, align: TextAlign) -> Option<Path> {
		let s = size / self.units_per_em;
		let width = self.measure(text, size);
		let mut pen_x = match align {
			TextAlign::Left => x,
			TextAlign::Center => x - width / 2.0,
			TextAlign::Right => x - width,
		};

		let mut pb = PathBuilder::new();
		for c in text.chars() {
			let id = self.font.glyph_id(c);
			if let Some(outline) = self.font.outline(id) {
				let map = |p: ab_glyph::Point| (pen_x + p.x * s, y - p.y * s);
				let mut last: Option<ab_glyph::Point> = None;
				for curve in &outline.curves {
					let (start, end) = match *curve {
						OutlineCurve::Line(a, b) => (a, b),
						OutlineCurve::Quad(a, _, b) => (a, b),
						OutlineCurve::Cubic(a, _, _, b) => (a, b),
					};
					if last != Some(start) {
						if last.is_some() {
							pb.close();
						}
						let (mx, my) = map(start);
						pb.move_to(mx, my);
					}
					match *curve {
						OutlineCurve::Line(_, b) => {
							let (bx, by) = map(b);
							pb.line_to(bx, by);
						}
						OutlineCurve::Quad(_, c1, b) => {
							let (cx, cy) = map(c1);
							let (bx, by) = map(b);
							pb.quad_to(cx, cy, bx, by);
						}
						OutlineCurve::Cubic(_, c1, c2, b) => {
							let (c1x, c1y) = map(c1);
							let (c2x, c2y) = map(c2);
							let (bx, by) = map(b);
							pb.cubic_to(c1x, c1y, c2x, c2y, bx, by);
						}
					}
					last = Some(end);
				}
				if last.is_some() {
					pb.close();
				}
			}
			pen_x += self.font.h_advance_unscaled(id) * s;
		}
		pb.finish()
	}
}

struct FontSet {
	barlow: Typeface,
	race_sport: Typeface,
	dseg7: Typeface,
	dseg14: Typeface,
	aldrich: Typeface,
	saira_xc: Typeface,
}

fn fonts() -> &'static FontSet {
	static FONTS: OnceLock<FontSet> = OnceLock::new();
	FONTS.get_or_init(|| FontSet {
		barlow: Typeface::load("Barlow Condensed", include_bytes!("../fonts/BarlowCondensed-Regular.ttf")),
		race_sport: Typeface::load("Race Sport", include_bytes!("../fonts/Race Sport.ttf")),
		dseg7: Typeface::load("DSEG7 Classic", include_bytes!("../fonts/DSEG7Classic-Regular.ttf")),
		dseg14: Typeface::load("DSEG14 Classic", include_bytes!("../fonts/DSEG14Classic-Regular.ttf")),
		aldrich: Typeface::load("Aldrich", include_bytes!("../fonts/Aldrich-Regular.ttf")),
		saira_xc: Typeface::load("Saira Extra Condensed", include_bytes!("../fonts/SairaExtraCondensed-SemiBold.ttf")),
	})
}

/// The default typeface (`FontHelper.Default`): Barlow Condensed.
pub fn default_typeface() -> &'static Typeface {
	&fonts().barlow
}

/// Looks up a font by the key used in gauge JSON (`"Race Sport"`,
/// `"DSEG7 Classic"`, ...). Unknown keys fall back to the default typeface,
/// like `RenderContext.GetTypeface`.
pub fn typeface(key: &str) -> &'static Typeface {
	let f = fonts();
	match key {
		"DSEG14 Classic" => &f.dseg14,
		"DSEG7 Classic" => &f.dseg7,
		"Race Sport" => &f.race_sport,
		"Barlow Condensed" => &f.barlow,
		"Aldrich" => &f.aldrich,
		"Saira Extra Condensed" => &f.saira_xc,
		_ => &f.barlow,
	}
}

/// Noto Serif JP Black (SIL OFL 1.1), subset to the kanji labels of the EVA
/// gauges (`gauges::eva`). It has no Latin glyphs, so it isn't one of the
/// [`FONT_KEYS`] offered to custom gauges.
pub fn jp_typeface() -> &'static Typeface {
	static JP: OnceLock<Typeface> = OnceLock::new();
	JP.get_or_init(|| Typeface::load("Noto Serif JP", include_bytes!("../fonts/NotoSerifJP-Black-subset.otf")))
}

/// Font keys available to gauge definitions.
pub const FONT_KEYS: [&str; 6] =
	["DSEG14 Classic", "DSEG7 Classic", "Race Sport", "Barlow Condensed", "Aldrich", "Saira Extra Condensed"];

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn all_fonts_load_and_measure() {
		for key in FONT_KEYS {
			let tf = typeface(key);
			assert_eq!(tf.name(), key);
			assert!(tf.measure("88", 40.0) > 0.0, "{key}");
			let m = tf.metrics(40.0);
			assert!(m.ascent < 0.0 && m.descent >= 0.0, "{key}: {m:?}");
		}
	}

	#[test]
	fn alignment_shifts_path() {
		let tf = default_typeface();
		let w = tf.measure("ABC", 20.0);
		let left = tf.text_path("ABC", 100.0, 50.0, 20.0, TextAlign::Left).unwrap().bounds();
		let right = tf.text_path("ABC", 100.0, 50.0, 20.0, TextAlign::Right).unwrap().bounds();
		assert!((left.left() - right.left() - w).abs() < 0.01);
		assert!(left.bottom() <= 50.5 && left.top() < 50.0);
	}
}
