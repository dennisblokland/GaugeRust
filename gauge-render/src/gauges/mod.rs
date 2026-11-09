//! Built-in gauge screens (`BaseGauge` and subclasses) plus the `gauges.json`
//! screen factory (`GaugeFactory`).

mod bar;
mod circular;
mod config;
mod digital;
mod eva;
mod grid;
mod histogram;
mod minmax;
mod needle;
mod segment_display;
mod sweep;
mod turboxs;

pub use bar::BarGauge;
pub use circular::CircularGauge;
pub use config::{AppConfig, GaugeConfig, GaugeType, GridCellConfig, ScreenConfig};
pub use digital::DigitalGauge;
pub use eva::{EvaDialGauge, EvaGearGauge, EvaHoneycombGauge, EvaReadoutGauge, EvaWarningGauge};
pub use grid::GridGauge;
pub use histogram::HistogramGauge;
pub use minmax::MinMaxGauge;
pub use needle::NeedleGauge;
pub use segment_display::SegmentDisplay;
pub use sweep::SweepGauge;
pub use turboxs::TurboXsGauge;

use crate::canvas::{rect, Canvas, Paint, Sprite};
use crate::color::Rgba;
use crate::custom::CustomGauge;

/// Blur sigma shared by the built-in gauges' glow effects.
pub(crate) const SHADOW_BLUR: f32 = 15.0;

/// State shared by all built-in gauges (`BaseGauge`).
#[derive(Clone, Debug)]
pub struct GaugeBase {
	pub value: f32,
	pub min_value: f32,
	pub max_value: f32,
	pub unit: String,
	pub title: String,
	active: Rgba,
	inactive: Rgba,
	/// Cleared when colours change so the static background is redrawn.
	pub static_cache_valid: bool,
}

impl GaugeBase {
	pub fn new(config: &GaugeConfig) -> Self {
		let active = Rgba::parse("#00FFFF").unwrap();
		Self {
			value: config.initial_value,
			min_value: config.min_value,
			max_value: config.max_value,
			unit: config.unit.clone(),
			title: config.title.clone(),
			active,
			inactive: dim(active),
			static_cache_valid: false,
		}
	}

	pub fn set_color_hex(&mut self, hex: &str) {
		self.active = Rgba::parse(hex).unwrap_or(Rgba::BLACK);
		self.inactive = dim(self.active);
		self.static_cache_valid = false;
	}

	/// `(active, inactive)` colours; inactive is the active colour at 10 % brightness.
	pub fn colors(&self) -> (Rgba, Rgba) {
		(self.active, self.inactive)
	}

	/// `(value - min) / (max - min)` clamped to 0..1.
	pub fn fraction(&self, value: f32) -> f32 {
		((value - self.min_value) / (self.max_value - self.min_value)).clamp(0.0, 1.0)
	}
}

fn dim(c: Rgba) -> Rgba {
	let d = |v: u8| (v as f32 * 0.1).round() as u8;
	Rgba::new(d(c.r), d(c.g), d(c.b), c.a)
}

/// Black full-screen fill (`BaseGauge.DrawBackground`).
pub(crate) fn draw_background(canvas: &mut Canvas) {
	if let Some(r) = rect(0.0, 0.0, canvas.width() as f32, canvas.height() as f32) {
		canvas.draw_rect(r, &Paint::fill(Rgba::BLACK));
	}
}

/// Renders a static layer and trims it to its content.
pub(crate) fn render_static(width: u32, height: u32, draw: impl FnOnce(&mut Canvas)) -> Sprite {
	Sprite::render(width, height, draw)
}

/// `Math.Round(v)` (banker's rounding) printed like `double.ToString()`.
pub(crate) fn rounded_label(v: f32) -> String {
	crate::format::format_number(v.round_ties_even(), "")
}

/// A built-in gauge screen.
pub trait Gauge {
	fn base(&self) -> &GaugeBase;
	fn base_mut(&mut self) -> &mut GaugeBase;
	fn draw(&mut self, canvas: &mut Canvas);

	fn set_value(&mut self, v: f32) {
		self.base_mut().value = v;
	}

	/// Clears accumulated state (peaks, history) when the screen is re-entered.
	fn reset_saved_state(&mut self) {}

	fn set_color_hex(&mut self, hex: &str) {
		self.base_mut().set_color_hex(hex);
	}
}

/// One configured screen: a built-in gauge bound to a data source, or a custom gauge.
pub enum Screen {
	Builtin {
		gauge: Box<dyn Gauge>,
		data_source: String,
	},
	Grid(Box<GridGauge>),
	Custom(Box<CustomGauge>),
	/// A screen whose custom definition couldn't be loaded. It keeps its
	/// place, so screen numbers still match `config.screens`, and shows why.
	Failed(String),
}

impl Screen {
	/// Draws the screen and returns what changed. Built-in gauges always
	/// report the full frame; custom gauges report only repainted regions.
	pub fn draw(&mut self, canvas: &mut Canvas) -> crate::Damage {
		match self {
			Screen::Builtin { gauge, .. } => gauge.draw(canvas),
			Screen::Grid(g) => g.draw(canvas),
			Screen::Custom(g) => return g.draw(canvas),
			Screen::Failed(message) => draw_failed(canvas, message),
		}
		crate::Damage::full(canvas.width(), canvas.height())
	}

	/// Repaints `r` on the next draw, for areas an overlay drew over. Built-in
	/// gauges repaint everything anyway.
	pub fn add_damage(&mut self, r: crate::IRect) {
		if let Screen::Custom(g) = self {
			g.add_damage(r);
		}
	}

	/// Forces a full repaint on the next draw (call when switching to this screen).
	pub fn invalidate(&mut self) {
		if let Screen::Custom(g) = self {
			g.invalidate();
		}
	}

	/// Feeds the latest data (`GaugeFactory.UpdateGaugeValues`); `read`
	/// returns `None` for unknown sources, which leaves the value unchanged.
	pub fn update(&mut self, mut read: impl FnMut(&str) -> Option<f32>) {
		match self {
			Screen::Builtin { gauge, data_source } => {
				if let Some(v) = read(data_source) {
					gauge.set_value(v);
				}
			}
			Screen::Grid(g) => {
				for i in 0..g.cell_count() {
					if let Some(v) = read(&g.cell_config(i).data_source) {
						g.set_cell_value(i, v);
					}
				}
			}
			Screen::Custom(g) => g.update_from(read),
			Screen::Failed(_) => {}
		}
	}

	pub fn reset_saved_state(&mut self) {
		match self {
			Screen::Builtin { gauge, .. } => gauge.reset_saved_state(),
			Screen::Grid(g) => g.reset_saved_state(),
			Screen::Custom(_) | Screen::Failed(_) => {}
		}
	}
}

/// Segment `i` of a horizontal segmented bar at (`x`, `y`), with a 2 px gap on each side.
pub(crate) fn segment_rect(x: f32, y: f32, height: f32, segment_width: f32, i: usize) -> Option<tiny_skia::Rect> {
	let seg_x = x + i as f32 * segment_width;
	crate::canvas::rect(seg_x + 2.0, y, seg_x + segment_width - 2.0, y + height)
}

/// The placeholder for a [`Screen::Failed`]: the reason in red on black.
fn draw_failed(canvas: &mut Canvas, message: &str) {
	use crate::text::{default_typeface, TextAlign};
	canvas.clear(Rgba::BLACK);
	let x = canvas.width() as f32 / 2.0;
	let red = Paint::fill(Rgba::rgb(255, 0, 0));
	canvas.draw_text("Screen failed to load", x, 200.0, TextAlign::Center, default_typeface(), 28.0, &red);
	let mut y = 240.0;
	for line in message.lines() {
		let white = Paint::fill(Rgba::WHITE);
		canvas.draw_text(line, x, y, TextAlign::Center, default_typeface(), 18.0, &white);
		y += 24.0;
	}
}

/// Creates the gauge for one config entry (`GaugeFactory.CreateGauge`).
pub fn create_gauge(config: &GaugeConfig, width: u32, height: u32) -> Box<dyn Gauge> {
	let mut gauge: Box<dyn Gauge> = match config.gauge_type {
		GaugeType::Circular => Box::new(CircularGauge::new(config, width, height)),
		GaugeType::Histogram => Box::new(HistogramGauge::new(config, width, height)),
		GaugeType::Needle => Box::new(NeedleGauge::new(config, width, height)),
		GaugeType::TurboXs => Box::new(TurboXsGauge::new(config, width, height)),
		GaugeType::Digital => Box::new(DigitalGauge::new(config, width, height)),
		GaugeType::Sweep => Box::new(SweepGauge::new(config, width, height)),
		GaugeType::MinMax => Box::new(MinMaxGauge::new(config, width, height)),
		GaugeType::Grid => Box::new(GridGauge::new(config, width, height)),
		GaugeType::EvaDial => Box::new(EvaDialGauge::new(config, width, height)),
		GaugeType::EvaHoneycomb => Box::new(EvaHoneycombGauge::new(config, width, height)),
		GaugeType::EvaReadout => Box::new(EvaReadoutGauge::new(config, width, height)),
		GaugeType::EvaWarning => Box::new(EvaWarningGauge::new(config, width, height)),
		GaugeType::EvaGear => Box::new(EvaGearGauge::new(config, width, height)),
		GaugeType::Bar | GaugeType::Custom => Box::new(BarGauge::new(config, width, height)),
	};
	if config.gauge_type != GaugeType::Grid {
		gauge.set_color_hex(&config.color_hex);
	}
	gauge
}

/// Builds every configured screen (`GaugeFactory.BuildScreens`). Custom
/// definition paths resolve relative to `config_dir`, the folder holding
/// `gauges.json`. C# resolves them against the executable's folder, which
/// is the same place: C# always loads the `gauges.json` next to the
/// executable, and only the Rust app can load another one (`--config`).
pub fn build_screens(
	config: &AppConfig,
	width: u32,
	height: u32,
	config_dir: Option<&std::path::Path>,
) -> Vec<Result<Screen, String>> {
	config
		.screens
		.iter()
		.map(|screen| {
			let gc = &screen.gauge;
			match (&gc.gauge_type, screen.custom_definition_file.as_deref()) {
				(GaugeType::Custom, Some(file)) if !file.trim().is_empty() => {
					let path = crate::images::resolve(file, config_dir);
					CustomGauge::load(path).map(|g| Screen::Custom(Box::new(g)))
				}
				(GaugeType::Grid, _) => Ok(Screen::Grid(Box::new(GridGauge::new(gc, width, height)))),
				_ => {
					Ok(Screen::Builtin { gauge: create_gauge(gc, width, height), data_source: gc.data_source.clone() })
				}
			}
		})
		.collect()
}
