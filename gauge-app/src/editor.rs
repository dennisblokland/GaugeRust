//! On-device config editor (port of `ConfigEditor`): SELECT opens a screen
//! list, each screen's gauge settings and, for Grid gauges, each cell's
//! settings. LEFT/RIGHT change values, START saves `gauges.json`, and
//! leaving the editor with B saves any edits not saved yet.

use std::path::Path;
use std::time::{Duration, Instant};

use gauge_render::canvas::{rect, Paint};
use gauge_render::gauges::{AppConfig, GaugeConfig, GaugeType, GridCellConfig, ScreenConfig};
use gauge_render::text::{typeface, TextAlign};
use gauge_render::{Canvas, Rgba};
use me_data::{DATA_SOURCE_NAMES, LEGACY_ALIASES};
use sdl2::keyboard::Keycode;

use crate::input::GamepadKey;

const W: f32 = 640.0;
const H: f32 = 480.0;
const FONT: &str = "Race Sport";

const COLOR_OPTIONS: [&str; 8] =
	["#00FFFF", "#FF0000", "#00FF00", "#FFFF00", "#FF00FF", "#FF8800", "#FFFFFF", "#0088FF"];

const UNIT_OPTIONS: [&str; 16] =
	["", "rpm", "kPa", "°C", "°F", "V", "psi", "λ", "AFR", "%", "°", "ms", "km/h", "mph", "mg/s", "g/s"];

const CYAN: Rgba = Rgba::rgb(0, 255, 255);
const LIME_GREEN: Rgba = Rgba::rgb(50, 205, 50);
const HIGHLIGHT: Rgba = Rgba::rgb(40, 40, 60);

/// `DataSourceMapper.DataSourceNames`: the channels, then the legacy aliases.
fn data_sources() -> &'static [&'static str] {
	static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
	NAMES.get_or_init(|| {
		DATA_SOURCE_NAMES.iter().copied().chain(LEGACY_ALIASES.iter().map(|(alias, _)| *alias)).collect()
	})
}

/// Steps through `options` from `current` (`IndexOf` + modulo, as in C#).
/// An unknown value counts as index `missing`.
fn cycle<'a>(options: &[&'a str], current: &str, dir: i32, missing: i32) -> &'a str {
	let n = options.len() as i32;
	let idx = options.iter().position(|o| *o == current).map_or(missing, |i| i as i32);
	options[(idx + dir).rem_euclid(n) as usize]
}

/// `ConfigEditor.StatusModeOptions`: the TurboXs gauge's status block modes.
const STATUS_MODES: [&str; 4] = ["Peak", "Min", "Text", "None"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
	Type,
	DataSource,
	Title,
	ColorHex,
	MinValue,
	MaxValue,
	InitialValue,
	Decimals,
	SegmentCount,
	Smoothing,
	MaxDataPoints,
	IntervalMs,
	StatusMode,
	Lcd,
	LcdGhosting,
	/// A Grid cell (0-based); A opens its editor.
	Cell(usize),
}

impl Field {
	fn label(self) -> String {
		match self {
			Field::Cell(i) => format!("Cell {}", i + 1),
			other => format!("{other:?}"),
		}
	}
}

/// `GetFieldsForType`.
fn fields_for(gauge: &GaugeConfig) -> Vec<Field> {
	use Field::*;
	const COMMON: [Field; 8] = [Type, DataSource, Title, ColorHex, MinValue, MaxValue, InitialValue, Decimals];
	let extra: &[Field] = match gauge.gauge_type {
		GaugeType::Bar | GaugeType::Circular | GaugeType::MinMax => &[SegmentCount, Smoothing],
		GaugeType::Histogram => &[MaxDataPoints, IntervalMs],
		GaugeType::Needle | GaugeType::Sweep => &[Smoothing],
		GaugeType::TurboXs => &[Smoothing, StatusMode, Lcd, LcdGhosting],
		GaugeType::Grid => return std::iter::once(Type).chain((0..gauge.cells.len()).map(Cell)).collect(),
		GaugeType::Digital | GaugeType::Custom => &[],
		GaugeType::EvaDial
		| GaugeType::EvaHoneycomb
		| GaugeType::EvaReadout
		| GaugeType::EvaWarning
		| GaugeType::EvaGear => &[],
	};
	COMMON.iter().chain(extra).copied().collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CellField {
	DataSource,
	Title,
	Unit,
	Decimals,
}

const CELL_FIELDS: [CellField; 4] = [CellField::DataSource, CellField::Title, CellField::Unit, CellField::Decimals];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
	ScreenList,
	GaugeEdit,
	GridCellEdit,
}

pub struct ConfigEditor {
	page: Page,
	cursor: usize,
	selected_screen: usize,
	selected_cell: usize,
	/// "Config Saved!" (or the save error) and until when it shows.
	message: Option<(String, Rgba, Instant)>,
	/// False once the user backs out of the screen list.
	pub active: bool,
	/// The config was edited (screens must be rebuilt).
	pub changed: bool,
	/// Edits made since the last successful save.
	unsaved: bool,
	/// The save on B failed; the next B leaves without saving.
	save_failed: bool,
}

impl ConfigEditor {
	pub fn new() -> Self {
		Self {
			page: Page::ScreenList,
			cursor: 0,
			selected_screen: 0,
			selected_cell: 0,
			message: None,
			active: true,
			changed: false,
			unsaved: false,
			save_failed: false,
		}
	}

	/// `HandleInput`. `keyboard` is the SDL key pressed this frame when no
	/// gamepad is in use: arrows, z (A), Escape/x (B), Return/s (START, as in
	/// `KeyBus`) and Delete/Backspace (X) then override the gamepad key, so
	/// the editor works from a keyboard. With a gamepad it must be `None`:
	/// gptokeyb also turns buttons into keys, and remapping those would turn
	/// a START press into something else.
	pub fn handle_input(
		&mut self,
		config: &mut AppConfig,
		path: Option<&Path>,
		key: GamepadKey,
		keyboard: Option<Keycode>,
	) {
		let key = match keyboard {
			Some(Keycode::Up) => GamepadKey::UP,
			Some(Keycode::Down) => GamepadKey::DOWN,
			Some(Keycode::Left) => GamepadKey::LEFT,
			Some(Keycode::Right) => GamepadKey::RIGHT,
			Some(Keycode::Z) => GamepadKey::A_DOWN,
			Some(Keycode::Escape | Keycode::X) => GamepadKey::B_DOWN,
			Some(Keycode::Return | Keycode::S) => GamepadKey::START_DOWN,
			// Not in C#, where `x` is B, so a keyboard couldn't delete screens.
			Some(Keycode::Delete | Keycode::Backspace) => GamepadKey::X_DOWN,
			_ => key,
		};
		if key == GamepadKey::START_DOWN {
			self.save(config, path);
			return;
		}
		match self.page {
			Page::ScreenList => self.handle_screen_list(config, path, key),
			Page::GaugeEdit => self.handle_gauge_edit(config, key),
			Page::GridCellEdit => self.handle_cell_edit(config, key),
		}
	}

	/// The config was edited: screens must be rebuilt and the file saved.
	fn edited(&mut self) {
		self.changed = true;
		self.unsaved = true;
		self.save_failed = false;
	}

	/// B on the screen list: saves unsaved edits, then closes. If the save
	/// fails the editor stays open with "Save failed!", and a second B
	/// leaves anyway (the edits still apply until the app restarts).
	fn close(&mut self, config: &AppConfig, path: Option<&Path>) {
		if self.unsaved && !self.save_failed && !self.save(config, path) {
			self.save_failed = true;
			if let Some(message) = &mut self.message {
				message.0 = "Save failed! B: quit".into();
			}
			return;
		}
		self.active = false;
	}

	fn handle_screen_list(&mut self, config: &mut AppConfig, path: Option<&Path>, key: GamepadKey) {
		let screens = config.screens.len();
		match key {
			GamepadKey::UP => self.cursor = self.cursor.saturating_sub(1),
			// The extra row is "+ Add Screen".
			GamepadKey::DOWN => self.cursor = (self.cursor + 1).min(screens),
			GamepadKey::A_DOWN if self.cursor < screens => {
				self.selected_screen = self.cursor;
				self.page = Page::GaugeEdit;
				self.cursor = 0;
			}
			GamepadKey::A_DOWN if screens < AppConfig::MAX_SCREENS => {
				config.screens.push(ScreenConfig::default());
				self.edited();
				self.cursor = config.screens.len() - 1;
			}
			GamepadKey::B_DOWN => self.close(config, path),
			GamepadKey::X_DOWN if self.cursor < screens && screens > 1 => {
				config.screens.remove(self.cursor);
				self.edited();
				self.cursor = self.cursor.min(config.screens.len());
			}
			_ => {}
		}
	}

	fn handle_gauge_edit(&mut self, config: &mut AppConfig, key: GamepadKey) {
		let gauge = &mut config.screens[self.selected_screen].gauge;
		let fields = fields_for(gauge);
		match key {
			GamepadKey::UP => self.cursor = self.cursor.saturating_sub(1),
			GamepadKey::DOWN => self.cursor = (self.cursor + 1).min(fields.len() - 1),
			GamepadKey::LEFT | GamepadKey::RIGHT => {
				adjust_field(gauge, fields[self.cursor], if key == GamepadKey::RIGHT { 1 } else { -1 });
				// A new type can have fewer fields.
				self.cursor = self.cursor.min(fields_for(gauge).len() - 1);
				self.edited();
			}
			GamepadKey::A_DOWN => {
				if let Field::Cell(i) = fields[self.cursor] {
					self.selected_cell = i;
					self.page = Page::GridCellEdit;
					self.cursor = 0;
				}
			}
			GamepadKey::B_DOWN => {
				self.page = Page::ScreenList;
				self.cursor = self.selected_screen;
			}
			_ => {}
		}
	}

	fn handle_cell_edit(&mut self, config: &mut AppConfig, key: GamepadKey) {
		let cell = &mut config.screens[self.selected_screen].gauge.cells[self.selected_cell];
		match key {
			GamepadKey::UP => self.cursor = self.cursor.saturating_sub(1),
			GamepadKey::DOWN => self.cursor = (self.cursor + 1).min(CELL_FIELDS.len() - 1),
			GamepadKey::LEFT | GamepadKey::RIGHT => {
				adjust_cell(cell, CELL_FIELDS[self.cursor], if key == GamepadKey::RIGHT { 1 } else { -1 });
				self.edited();
			}
			GamepadKey::B_DOWN => {
				self.page = Page::GaugeEdit;
				self.cursor = self.selected_cell + 1;
			}
			_ => {}
		}
	}

	/// Writes `config` to `path` and shows the result; true if it worked.
	fn save(&mut self, config: &AppConfig, path: Option<&Path>) -> bool {
		let until = Instant::now() + Duration::from_secs(2);
		let result = path.map_or(Ok(()), |p| config.save(p));
		self.changed = true;
		match result {
			Ok(()) => {
				self.message = Some(("Config Saved!".into(), LIME_GREEN, until));
				self.unsaved = false;
				true
			}
			Err(e) => {
				eprintln!("[editor] saving failed: {e}");
				self.message = Some(("Save failed!".into(), Rgba::rgb(255, 32, 32), until));
				false
			}
		}
	}

	/// Draws the whole editor (it covers the screen).
	pub fn draw(&self, config: &AppConfig, canvas: &mut Canvas) {
		canvas.clear(Rgba::BLACK);
		match self.page {
			Page::ScreenList => self.draw_screen_list(config, canvas),
			Page::GaugeEdit => self.draw_gauge_edit(config, canvas),
			Page::GridCellEdit => self.draw_cell_edit(config, canvas),
		}
		if let Some((text, color, until)) = &self.message {
			if Instant::now() < *until {
				text_at(canvas, text, W / 2.0 - 60.0, H - 20.0, 20.0, *color);
			}
		}
		let controls = match self.page {
			Page::ScreenList => "UP/DOWN:Nav  A:Edit  X:Delete  START:Save  B:Exit",
			Page::GaugeEdit => "UP/DOWN:Nav  LEFT/RIGHT:Change  A:Edit Cell  B:Back  START:Save",
			Page::GridCellEdit => "UP/DOWN:Nav  LEFT/RIGHT:Change  B:Back  START:Save",
		};
		text_at(canvas, controls, 30.0, H - 10.0, 12.0, Rgba::rgb(100, 100, 100));
	}

	fn draw_screen_list(&self, config: &AppConfig, canvas: &mut Canvas) {
		draw_title(canvas, "Screens");
		let mut y = 80.0;
		for (i, screen) in config.screens.iter().enumerate() {
			let g = &screen.gauge;
			let label = if g.gauge_type == GaugeType::Grid {
				format!("Screen {}: Grid (2x2)", i + 1)
			} else {
				format!("Screen {}: {} ({})", i + 1, g.title, g.data_source)
			};
			draw_menu_item(canvas, &label, y, i == self.cursor, Rgba::WHITE);
			y += 40.0;
		}
		if config.screens.len() < AppConfig::MAX_SCREENS {
			draw_menu_item(canvas, "+ Add Screen", y, self.cursor == config.screens.len(), LIME_GREEN);
		}
	}

	fn draw_gauge_edit(&self, config: &AppConfig, canvas: &mut Canvas) {
		let gauge = &config.screens[self.selected_screen].gauge;
		draw_title(canvas, &format!("Screen {}: {}", self.selected_screen + 1, gauge.title));
		let fields = fields_for(gauge);
		// Scroll so the cursor stays visible (Grid gauges can have many cells).
		let max_visible = (H as usize - 100) / 30;
		let scroll = if fields.len() > max_visible {
			(self.cursor + 2).saturating_sub(max_visible).min(fields.len() - max_visible)
		} else {
			0
		};
		let mut y = 70.0;
		for (i, &field) in fields.iter().enumerate().skip(scroll) {
			if y >= H - 40.0 {
				break;
			}
			let swatch = (field == Field::ColorHex).then_some(gauge.color_hex.as_str());
			draw_field_row(canvas, &field.label(), &field_value(gauge, field), y, i == self.cursor, swatch);
			y += 30.0;
		}
	}

	fn draw_cell_edit(&self, config: &AppConfig, canvas: &mut Canvas) {
		let cell = &config.screens[self.selected_screen].gauge.cells[self.selected_cell];
		draw_title(canvas, &format!("Cell {}", self.selected_cell + 1));
		let mut y = 80.0;
		for (i, &field) in CELL_FIELDS.iter().enumerate() {
			let value = match field {
				CellField::DataSource => cell.data_source.clone(),
				CellField::Title => cell.title.clone(),
				CellField::Unit if cell.unit.is_empty() => "(none)".into(),
				CellField::Unit => cell.unit.clone(),
				CellField::Decimals => cell.decimals.to_string(),
			};
			draw_field_row(canvas, &format!("{field:?}"), &value, y, i == self.cursor, None);
			y += 38.0;
		}
	}
}

/// `AdjustField`.
fn adjust_field(gauge: &mut GaugeConfig, field: Field, dir: i32) {
	match field {
		Field::Type => {
			let types = GaugeType::ALL.map(|t| t.name());
			gauge.gauge_type = GaugeType::parse(cycle(&types, gauge.gauge_type.name(), dir, 0)).unwrap_or_default();
			if gauge.gauge_type == GaugeType::Grid && gauge.cells.len() < 4 {
				gauge.cells.resize_with(4, GridCellConfig::default);
			}
		}
		Field::DataSource => gauge.data_source = cycle(data_sources(), &gauge.data_source, dir, -1).into(),
		Field::Title => gauge.title = cycle(data_sources(), &gauge.title, dir, 0).into(),
		Field::ColorHex => gauge.color_hex = cycle(&COLOR_OPTIONS, &gauge.color_hex, dir, 0).into(),
		Field::MinValue => gauge.min_value += dir as f32,
		Field::MaxValue => gauge.max_value += dir as f32,
		Field::InitialValue => gauge.initial_value += dir as f32 * 0.1,
		Field::Decimals => gauge.decimals = (gauge.decimals + dir).clamp(0, 4),
		Field::SegmentCount => gauge.segment_count = (gauge.segment_count + dir * 4).clamp(4, 64),
		Field::Smoothing => gauge.smoothing = !gauge.smoothing,
		Field::MaxDataPoints => gauge.max_data_points = (gauge.max_data_points + dir * 5).clamp(10, 100),
		Field::IntervalMs => gauge.interval_ms = (gauge.interval_ms + dir * 100).clamp(100, 5000),
		Field::Lcd => gauge.lcd = !gauge.lcd,
		Field::LcdGhosting => {
			gauge.lcd_ghosting = (((gauge.lcd_ghosting + dir as f32 * 0.1) * 10.0).round() / 10.0).clamp(0.0, 0.9)
		}
		Field::StatusMode => {
			let current = STATUS_MODES.iter().position(|m| m.eq_ignore_ascii_case(&gauge.status_mode)).unwrap_or(0);
			gauge.status_mode =
				STATUS_MODES[(current as i32 + dir).rem_euclid(STATUS_MODES.len() as i32) as usize].into();
		}
		Field::Cell(_) => {}
	}
}

/// `AdjustCellField`.
fn adjust_cell(cell: &mut GridCellConfig, field: CellField, dir: i32) {
	match field {
		CellField::DataSource => cell.data_source = cycle(data_sources(), &cell.data_source, dir, 0).into(),
		CellField::Title => cell.title = cycle(data_sources(), &cell.title, dir, 0).into(),
		CellField::Unit => cell.unit = cycle(&UNIT_OPTIONS, &cell.unit, dir, 0).into(),
		CellField::Decimals => cell.decimals = (cell.decimals + dir).clamp(0, 4),
	}
}

/// `GetFieldValue`.
fn field_value(gauge: &GaugeConfig, field: Field) -> String {
	let f1 = |v: f32| gauge_render::format::format_number(v, "F1");
	match field {
		Field::Type => gauge.gauge_type.name().into(),
		Field::DataSource => gauge.data_source.clone(),
		Field::Title => gauge.title.clone(),
		Field::ColorHex => gauge.color_hex.clone(),
		Field::MinValue => f1(gauge.min_value),
		Field::MaxValue => f1(gauge.max_value),
		Field::InitialValue => f1(gauge.initial_value),
		Field::Decimals => gauge.decimals.to_string(),
		Field::SegmentCount => gauge.segment_count.to_string(),
		Field::Smoothing => if gauge.smoothing { "ON" } else { "OFF" }.into(),
		Field::MaxDataPoints => gauge.max_data_points.to_string(),
		Field::IntervalMs => format!("{}ms", gauge.interval_ms),
		Field::StatusMode => gauge.status_mode.clone(),
		Field::Lcd => if gauge.lcd { "ON" } else { "OFF" }.into(),
		// "0.0" rounds halves away from zero, like the C# editor.
		Field::LcdGhosting => gauge_render::format::format_number(gauge.lcd_ghosting, "0.0"),
		Field::Cell(i) => gauge.cells.get(i).map_or("?".into(), |c| format!("{} ({})", c.data_source, c.unit)),
	}
}

fn text_at(canvas: &mut Canvas, text: &str, x: f32, y: f32, size: f32, color: Rgba) {
	canvas.draw_text(text, x, y, TextAlign::Left, typeface(FONT), size, &Paint::fill(color));
}

fn draw_title(canvas: &mut Canvas, title: &str) {
	let width = typeface(FONT).measure(title, 28.0);
	text_at(canvas, title, (W - width) / 2.0, 40.0, 28.0, Rgba::WHITE);
	canvas.draw_line(40.0, 55.0, W - 40.0, 55.0, &Paint::stroke(Rgba::rgb(60, 60, 60), 1.0));
}

fn draw_highlight(canvas: &mut Canvas, y: f32) {
	if let Some(r) = rect(30.0, y - 22.0, W - 30.0, y + 12.0) {
		canvas.draw_round_rect(r, 4.0, 4.0, &Paint::fill(HIGHLIGHT));
	}
}

fn draw_menu_item(canvas: &mut Canvas, label: &str, y: f32, selected: bool, color: Rgba) {
	if selected {
		draw_highlight(canvas, y);
		text_at(canvas, ">", 38.0, y, 18.0, CYAN);
	}
	text_at(canvas, label, 60.0, y, 18.0, if selected { CYAN } else { color });
}

/// A "label ... < value >" row; `swatch` colours the value (the colour field).
fn draw_field_row(canvas: &mut Canvas, label: &str, value: &str, y: f32, selected: bool, swatch: Option<&str>) {
	if selected {
		draw_highlight(canvas, y);
		text_at(canvas, "<", 340.0, y, 16.0, CYAN);
		text_at(canvas, ">", W - 60.0, y, 16.0, CYAN);
	}
	text_at(canvas, label, 60.0, y, 16.0, if selected { CYAN } else { Rgba::rgb(180, 180, 180) });
	let value_color =
		if selected { swatch.and_then(Rgba::parse).unwrap_or(Rgba::WHITE) } else { Rgba::rgb(200, 200, 200) };
	let width = typeface(FONT).measure(value, 16.0);
	text_at(canvas, value, W - 80.0 - width, y, 16.0, value_color);
}

#[cfg(test)]
mod tests {
	use super::*;

	fn press(editor: &mut ConfigEditor, config: &mut AppConfig, keys: &[GamepadKey]) {
		for &k in keys {
			editor.handle_input(config, None, k, None);
		}
	}

	#[test]
	fn edits_screens_gauges_and_grid_cells() {
		let mut config = AppConfig::default_config();
		let mut editor = ConfigEditor::new();
		use GamepadKey as K;

		// Add a second screen via the "+ Add Screen" row, then open it.
		press(&mut editor, &mut config, &[K::DOWN, K::A_DOWN]);
		assert_eq!(config.screens.len(), 2);
		assert!(editor.changed);
		press(&mut editor, &mut config, &[K::A_DOWN]);
		assert_eq!(editor.page, Page::GaugeEdit);

		// Type: Bar -> Circular -> Bar -> (wraps back) TurboXs -> Custom.
		press(&mut editor, &mut config, &[K::RIGHT]);
		assert_eq!(config.screens[1].gauge.gauge_type, GaugeType::Circular);
		press(&mut editor, &mut config, &[K::LEFT, K::LEFT]);
		assert_eq!(config.screens[1].gauge.gauge_type, GaugeType::TurboXs);
		press(&mut editor, &mut config, &[K::LEFT]);
		assert_eq!(config.screens[1].gauge.gauge_type, GaugeType::Custom);
		// Custom -> Grid creates 4 cells, and the field list becomes Type + cells.
		press(&mut editor, &mut config, &[K::LEFT]);
		let gauge = &config.screens[1].gauge;
		assert_eq!(gauge.gauge_type, GaugeType::Grid);
		assert_eq!(gauge.cells.len(), 4);
		assert_eq!(fields_for(gauge).len(), 5);

		// Into cell 2, change its unit and decimals, and back out.
		press(&mut editor, &mut config, &[K::DOWN, K::DOWN, K::A_DOWN]);
		assert_eq!((editor.page, editor.selected_cell), (Page::GridCellEdit, 1));
		press(&mut editor, &mut config, &[K::DOWN, K::DOWN, K::RIGHT, K::RIGHT, K::DOWN, K::RIGHT, K::B_DOWN]);
		let cell = &config.screens[1].gauge.cells[1];
		assert_eq!((cell.unit.as_str(), cell.decimals), ("kPa", 1));
		assert_eq!((editor.page, editor.cursor), (Page::GaugeEdit, 2));

		// Back to the list, delete screen 1; the last screen can't be deleted.
		press(&mut editor, &mut config, &[K::B_DOWN, K::UP, K::X_DOWN, K::X_DOWN]);
		assert_eq!(config.screens.len(), 1);
		assert_eq!(config.screens[0].gauge.gauge_type, GaugeType::Grid);

		press(&mut editor, &mut config, &[K::B_DOWN]);
		assert!(!editor.active);
	}

	#[test]
	fn gauge_fields_follow_csharp_rules() {
		let mut g = GaugeConfig { data_source: "Rpm".into(), ..GaugeConfig::default() };
		adjust_field(&mut g, Field::DataSource, -1);
		assert_eq!(g.data_source, "InjectorDuty", "wraps to the last alias");
		adjust_field(&mut g, Field::DataSource, 1);
		assert_eq!(g.data_source, "Rpm");
		g.data_source = "Unknown".into();
		adjust_field(&mut g, Field::DataSource, 1);
		assert_eq!(g.data_source, "Rpm", "unknown counts as -1");
		adjust_field(&mut g, Field::ColorHex, -1);
		assert_eq!(g.color_hex, "#0088FF");
		g.segment_count = 62;
		adjust_field(&mut g, Field::SegmentCount, 1);
		assert_eq!(g.segment_count, 64);
		adjust_field(&mut g, Field::IntervalMs, -1);
		assert_eq!(g.interval_ms, 900);
		assert_eq!(field_value(&g, Field::IntervalMs), "900ms");
		adjust_field(&mut g, Field::Smoothing, 1);
		assert_eq!(field_value(&g, Field::Smoothing), "OFF");
		assert_eq!(field_value(&g, Field::MaxValue), "100.0");
		adjust_field(&mut g, Field::StatusMode, 1);
		assert_eq!(field_value(&g, Field::StatusMode), "Min");
		adjust_field(&mut g, Field::StatusMode, -2);
		assert_eq!(g.status_mode, "None", "wraps backwards");
		adjust_field(&mut g, Field::Lcd, 1);
		assert_eq!(field_value(&g, Field::Lcd), "ON");
		adjust_field(&mut g, Field::LcdGhosting, -1);
		assert_eq!(field_value(&g, Field::LcdGhosting), "0.1");
		for _ in 0..12 {
			adjust_field(&mut g, Field::LcdGhosting, 1);
		}
		assert_eq!(field_value(&g, Field::LcdGhosting), "0.9", "capped");
	}

	#[test]
	fn start_saves_and_keyboard_overrides() {
		let dir = std::env::temp_dir().join(format!("gauge-editor-{}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("gauges.json");
		let mut config = AppConfig::default_config();
		let mut editor = ConfigEditor::new();
		// z = A (open screen 1), Right = next type, s = START (save).
		for k in [Keycode::Z, Keycode::Right, Keycode::S] {
			editor.handle_input(&mut config, Some(&path), GamepadKey::NONE, Some(k));
		}
		let saved = AppConfig::load_or_create(&path);
		assert_eq!(saved.screens[0].gauge.gauge_type, GaugeType::Circular);
		assert_eq!(editor.message.as_ref().map(|m| m.0.as_str()), Some("Config Saved!"));
		// Escape = B: back to the list, then out.
		editor.handle_input(&mut config, Some(&path), GamepadKey::MENU_DOWN, Some(Keycode::Escape));
		editor.handle_input(&mut config, Some(&path), GamepadKey::MENU_DOWN, Some(Keycode::Escape));
		assert!(!editor.active);
		let _ = std::fs::remove_dir_all(dir);
	}

	#[test]
	fn leaving_with_b_saves_edits() {
		let dir = std::env::temp_dir().join(format!("gauge-editor-b-{}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("gauges.json");
		let mut config = AppConfig::default_config();
		let mut editor = ConfigEditor::new();
		use GamepadKey as K;
		// Open screen 1, change its type, back to the list and out: no START.
		for k in [K::A_DOWN, K::RIGHT, K::B_DOWN, K::B_DOWN] {
			editor.handle_input(&mut config, Some(&path), k, None);
		}
		assert!(!editor.active && editor.changed);
		assert_eq!(AppConfig::load_or_create(&path).screens[0].gauge.gauge_type, GaugeType::Circular);
		let _ = std::fs::remove_dir_all(dir);
	}

	#[test]
	fn leaving_without_edits_writes_nothing() {
		let path = std::env::temp_dir().join(format!("gauge-editor-none-{}", std::process::id())).join("gauges.json");
		let mut config = AppConfig::default_config();
		let mut editor = ConfigEditor::new();
		editor.handle_input(&mut config, Some(&path), GamepadKey::B_DOWN, None);
		assert!(!editor.active && !editor.changed);
		assert!(!path.exists());
	}

	#[test]
	fn failed_save_on_b_keeps_editor_open_once() {
		// The parent directory doesn't exist, so the save fails.
		let path =
			std::env::temp_dir().join(format!("gauge-editor-missing-{}", std::process::id())).join("gauges.json");
		let mut config = AppConfig::default_config();
		let mut editor = ConfigEditor::new();
		use GamepadKey as K;
		for k in [K::A_DOWN, K::RIGHT, K::B_DOWN, K::B_DOWN] {
			editor.handle_input(&mut config, Some(&path), k, None);
		}
		assert!(editor.active, "stays open so the failure is seen");
		assert_eq!(editor.message.as_ref().map(|m| m.0.as_str()), Some("Save failed! B: quit"));
		editor.handle_input(&mut config, Some(&path), K::B_DOWN, None);
		assert!(!editor.active, "second B leaves anyway");
	}

	#[test]
	fn start_with_gamepad_is_not_remapped() {
		// With a gamepad, gptokeyb may also send START as Return; the caller
		// passes no keyboard key then, and Return itself means START anyway.
		let dir = std::env::temp_dir().join(format!("gauge-editor-start-{}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("gauges.json");
		let mut config = AppConfig::default_config();
		let mut editor = ConfigEditor::new();
		editor.handle_input(&mut config, Some(&path), GamepadKey::START_DOWN, None);
		assert_eq!(editor.page, Page::ScreenList, "START didn't open a screen");
		assert!(path.exists());
		std::fs::remove_file(&path).unwrap();
		editor.handle_input(&mut config, Some(&path), GamepadKey::START_DOWN, Some(Keycode::Return));
		assert_eq!(editor.page, Page::ScreenList);
		assert!(path.exists());
		let _ = std::fs::remove_dir_all(dir);
	}

	#[test]
	fn draws_every_page() {
		let mut config = AppConfig::default_config();
		let mut editor = ConfigEditor::new();
		let mut canvas = Canvas::new(640, 480);
		let lit = |c: &Canvas| c.pixmap().pixels().iter().filter(|p| p.red() > 100 || p.green() > 100).count();
		editor.draw(&config, &mut canvas);
		assert!(lit(&canvas) > 500);
		// Selected row is cyan.
		assert!(
			(60..300).any(|x| (60..86).any(|y| canvas.pixel(x, y).is_some_and(|p| p.r < 40 && p.g > 200 && p.b > 200)))
		);
		editor.handle_input(&mut config, None, GamepadKey::A_DOWN, None);
		config.screens[0].gauge.gauge_type = GaugeType::Grid;
		config.screens[0].gauge.cells.resize_with(16, GridCellConfig::default);
		editor.cursor = 16;
		editor.draw(&config, &mut canvas);
		assert!(lit(&canvas) > 500);
		editor.handle_input(&mut config, None, GamepadKey::A_DOWN, None);
		assert_eq!(editor.page, Page::GridCellEdit);
		editor.draw(&config, &mut canvas);
		assert!(lit(&canvas) > 500);
	}
}
