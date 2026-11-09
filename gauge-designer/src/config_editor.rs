//! Whole-config editor: the `gauges.json` screen list, each screen's
//! built-in gauge settings and grid cells, with a live preview of every
//! screen. Covers what the on-device config editor does.

use std::path::{Path, PathBuf};

use eframe::egui::{self, vec2, Color32, ComboBox, RichText, TextureHandle, TextureOptions};
use gauge_render::gauges::{build_screens, Screen};
use gauge_render::gauges::{AppConfig, GaugeType, GridCellConfig, ScreenConfig};
use gauge_render::Canvas;
use serde_json::Value;

use crate::fields::{Field, Kind};
use crate::props::{self, PropContext};

const fn f(key: &'static str, label: &'static str, kind: Kind) -> Field {
	Field { key, label, kind }
}

const BIG: Kind = Kind::Float(-100_000.0, 100_000.0, 0.1);

/// Settings every built-in gauge has (`GetFieldsForType` in the device editor), plus Unit.
const COMMON: &[Field] = &[
	f("DataSource", "Data source", Kind::Source),
	f("Title", "Title", Kind::Text),
	f("Unit", "Unit", Kind::Text),
	f("ColorHex", "Colour", Kind::Color),
	f("MinValue", "Min value", BIG),
	f("MaxValue", "Max value", BIG),
	f("InitialValue", "Initial value", BIG),
	f("Decimals", "Decimals", Kind::Int(0, 6)),
];

fn extra(t: GaugeType) -> &'static [Field] {
	const SEGMENTS: &[Field] =
		&[f("SegmentCount", "Segments", Kind::Int(1, 200)), f("Smoothing", "Smoothing", Kind::Bool)];
	const HISTOGRAM: &[Field] =
		&[f("MaxDataPoints", "Data points", Kind::Int(1, 500)), f("IntervalMs", "Interval (ms)", Kind::Int(0, 60_000))];
	const SMOOTH: &[Field] = &[f("Smoothing", "Smoothing", Kind::Bool)];
	const TURBOXS: &[Field] = &[
		f("Smoothing", "Smoothing", Kind::Bool),
		f("StatusLabel", "Status label", Kind::Text),
		f("StatusText", "Status text", Kind::Text),
		f("Lcd", "LCD look", Kind::Bool),
		f("LcdGhosting", "LCD ghosting", Kind::Float(0.0, 0.9, 0.05)),
	];
	match t {
		GaugeType::Bar | GaugeType::Circular | GaugeType::MinMax => SEGMENTS,
		GaugeType::Histogram => HISTOGRAM,
		GaugeType::Needle | GaugeType::Sweep => SMOOTH,
		GaugeType::TurboXs => TURBOXS,
		GaugeType::Digital | GaugeType::Custom | GaugeType::Grid => &[],
		GaugeType::EvaDial
		| GaugeType::EvaHoneycomb
		| GaugeType::EvaReadout
		| GaugeType::EvaWarning
		| GaugeType::EvaGear => &[],
	}
}

const CELL: &[Field] = &[
	f("DataSource", "Data source", Kind::Source),
	f("Title", "Title", Kind::Text),
	f("Unit", "Unit", Kind::Text),
	f("Decimals", "Decimals", Kind::Int(0, 6)),
];

const STATUS_MODES: [&str; 4] = ["Peak", "Min", "Text", "None"];
const PREVIEW_W: u32 = 640;
const PREVIEW_H: u32 = 480;

pub struct ConfigEditor {
	path: Option<PathBuf>,
	config: AppConfig,
	selected: usize,
	revision: u64,
	saved_revision: u64,
	previews: Vec<Option<TextureHandle>>,
	errors: Vec<Option<String>>,
	previewed: Option<(u64, u32)>,
	/// Where in each screen's range the previews sit (0–100 %).
	preview_percent: u32,
	message: Option<(String, bool)>,
	upload: Option<std::thread::JoinHandle<Result<String, (String, String)>>>,
}

impl ConfigEditor {
	pub fn empty() -> Self {
		Self {
			path: None,
			config: AppConfig::default_config(),
			selected: 0,
			revision: 1,
			saved_revision: 1,
			previews: Vec::new(),
			errors: Vec::new(),
			previewed: None,
			preview_percent: 60,
			message: None,
			upload: None,
		}
	}

	pub fn open(path: &Path) -> Self {
		let mut editor = Self::empty();
		editor.load(path);
		editor
	}

	fn load(&mut self, path: &Path) {
		// Unlike the device, never write defaults over a file that doesn't parse.
		match std::fs::read_to_string(path)
			.map_err(|e| e.to_string())
			.and_then(|json| AppConfig::from_json(&json).map_err(|e| e.to_string()))
		{
			Ok(config) => {
				self.config = config;
				self.path = Some(path.to_path_buf());
				self.selected = 0;
				self.revision += 1;
				self.saved_revision = self.revision;
				self.message = Some((format!("Opened {}", path.display()), false));
			}
			Err(e) => self.message = Some((format!("Couldn't open {}: {e}", path.display()), true)),
		}
	}

	fn dir(&self) -> Option<PathBuf> {
		self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf)
	}

	fn save(&mut self, save_as: bool) {
		let path = match (&self.path, save_as) {
			(Some(p), false) => p.clone(),
			_ => match rfd::FileDialog::new()
				.add_filter("gauges.json", &["json"])
				.set_file_name("gauges.json")
				.save_file()
			{
				Some(p) => p,
				None => return,
			},
		};
		// AppConfig::save writes atomically (temp file, fsync, rename).
		match self.config.save(&path) {
			Ok(()) => {
				self.path = Some(path.clone());
				self.saved_revision = self.revision;
				self.message = Some((format!("Saved {}", path.display()), false));
			}
			Err(e) => self.message = Some((format!("Save failed: {e}"), true)),
		}
	}

	fn changed(&mut self) {
		self.revision += 1;
	}

	/// Re-renders every screen's preview when something changed.
	fn refresh_previews(&mut self, ctx: &egui::Context) {
		let key = (self.revision, self.preview_percent);
		if self.previewed == Some(key) {
			return;
		}
		self.previewed = Some(key);
		let t = self.preview_percent as f32 / 100.0;
		// Built-in gauges start at InitialValue, so place them there (no smoothing to wait for).
		let mut config = self.config.clone();
		for s in &mut config.screens {
			let g = &mut s.gauge;
			g.initial_value = g.min_value + (g.max_value - g.min_value) * t;
		}
		let screens = build_screens(&config, PREVIEW_W, PREVIEW_H, self.dir().as_deref());
		self.previews.resize_with(screens.len(), || None);
		self.previews.truncate(screens.len());
		self.errors = vec![None; screens.len()];
		for (i, screen) in screens.into_iter().enumerate() {
			let mut canvas = Canvas::new(PREVIEW_W, PREVIEW_H);
			match screen {
				Ok(mut screen) => {
					let cfg = &config.screens[i].gauge;
					match &mut screen {
						Screen::Custom(g) => {
							let ranges = custom_ranges(g.definition());
							g.update_from(|name| {
								ranges
									.iter()
									.find(|r| gauge_render::eq_ignore_case(&r.0, name))
									.map(|r| r.1 + (r.2 - r.1) * t)
							});
							g.snap_to_targets();
						}
						Screen::Grid(_) => screen.update(|_| Some(cfg.initial_value)),
						Screen::Builtin { .. } | Screen::Failed(_) => {}
					}
					screen.draw(&mut canvas);
				}
				Err(e) => {
					canvas.clear(gauge_render::Rgba::rgb(40, 0, 0));
					self.errors[i] = Some(e);
				}
			}
			let image = egui::ColorImage::from_rgba_premultiplied(
				[PREVIEW_W as usize, PREVIEW_H as usize],
				canvas.pixmap().data(),
			);
			match &mut self.previews[i] {
				Some(tex) => tex.set(image, TextureOptions::LINEAR),
				slot => *slot = Some(ctx.load_texture(format!("screen{i}"), image, TextureOptions::LINEAR)),
			}
		}
	}

	/// Draws the window. Returns a custom definition to open in the designer.
	pub fn window(&mut self, ctx: &egui::Context, open: &mut bool) -> Option<PathBuf> {
		self.refresh_previews(ctx);
		if self.upload.as_ref().is_some_and(|h| h.is_finished()) {
			self.message = Some(match self.upload.take().unwrap().join() {
				Ok(Ok(_)) => ("Uploaded gauges.json to the device. Restart the app there to load it.".into(), false),
				Ok(Err((log, e))) => (format!("Upload failed: {e}\n{log}"), true),
				Err(_) => ("Upload failed".into(), true),
			});
		}
		let mut request = None;
		let title = format!(
			"Screens — {}{}",
			self.path.as_deref().map(|p| p.display().to_string()).unwrap_or_else(|| "new gauges.json".into()),
			if self.revision != self.saved_revision { " •" } else { "" }
		);
		egui::Window::new(title)
			.id(egui::Id::new("config-editor"))
			.open(open)
			.default_size([1000.0, 620.0])
			.resizable(true)
			.show(ctx, |ui| {
				self.toolbar(ui);
				ui.separator();
				egui::Panel::left("screens").resizable(true).default_size(250.0).show(ui, |ui| self.screen_list(ui));
				egui::Panel::right("previews").resizable(true).default_size(330.0).show(ui, |ui| self.preview_grid(ui));
				egui::CentralPanel::default().show(ui, |ui| {
					egui::ScrollArea::vertical()
						.auto_shrink([false, false])
						.show(ui, |ui| request = self.screen_settings(ui));
				});
			});
		request
	}

	fn toolbar(&mut self, ui: &mut egui::Ui) {
		ui.horizontal_wrapped(|ui| {
			if ui.button("Open…").clicked() {
				if let Some(p) = rfd::FileDialog::new().add_filter("gauges.json", &["json"]).pick_file() {
					self.load(&p);
				}
			}
			if ui.button("New").on_hover_text("The device's default config").clicked() {
				*self = Self::empty();
			}
			if ui.button("Save").clicked() {
				self.save(false);
			}
			if ui.button("Save as…").clicked() {
				self.save(true);
			}
			let can_upload = self.path.is_some() && self.upload.is_none();
			if ui.add_enabled(can_upload, egui::Button::new("Upload to device")).on_hover_text("Saves, then copies gauges.json into the app folder (settings from the Deploy window's environment variables)").clicked() {
				self.save(false);
				if let Some(path) = self.path.clone() {
					let target = crate::deploy::DeployTarget::from_env();
					self.upload = Some(std::thread::spawn(move || target.upload_config(&path)));
				}
			}
			if self.upload.is_some() {
				ui.spinner();
			}
			ui.separator();
			let mut changed = ui.checkbox(&mut self.config.demo_mode, "Demo mode").changed();
			let mut battery = self.config.simulate_battery_percent.is_some();
			changed |= ui.checkbox(&mut battery, "Simulate battery").changed();
			if battery {
				let mut pct = self.config.simulate_battery_percent.unwrap_or(100);
				changed |= ui.add(egui::DragValue::new(&mut pct).range(0..=100).suffix(" %")).changed();
				self.config.simulate_battery_percent = Some(pct);
			} else {
				self.config.simulate_battery_percent = None;
			}
			ui.label("ECU MAC");
			let mut mac = self.config.device_mac_address.clone().unwrap_or_default();
			if ui.add(egui::TextEdit::singleline(&mut mac).desired_width(140.0).hint_text("first RaceChrono device")).changed() {
				self.config.device_mac_address = (!mac.is_empty()).then_some(mac);
				changed = true;
			}
			if changed {
				self.changed();
			}
		});
		if let Some((text, error)) = &self.message {
			ui.label(if *error { RichText::new(text).color(props::ERROR) } else { RichText::new(text).weak() });
		}
	}

	fn screen_list(&mut self, ui: &mut egui::Ui) {
		ui.heading("Screens");
		let n = self.config.screens.len();
		ui.horizontal(|ui| {
			if ui.add_enabled(n < AppConfig::MAX_SCREENS, egui::Button::new("+ Add")).clicked() {
				self.config.screens.push(ScreenConfig::default());
				self.selected = self.config.screens.len() - 1;
				self.changed();
			}
			if ui.add_enabled(n > 1, egui::Button::new("Delete")).clicked() {
				self.config.screens.remove(self.selected);
				self.selected = self.selected.min(self.config.screens.len() - 1);
				self.changed();
			}
			if ui.add_enabled(self.selected > 0, egui::Button::new("⬆")).clicked() {
				self.config.screens.swap(self.selected, self.selected - 1);
				self.selected -= 1;
				self.changed();
			}
			if ui.add_enabled(self.selected + 1 < n, egui::Button::new("⬇")).clicked() {
				self.config.screens.swap(self.selected, self.selected + 1);
				self.selected += 1;
				self.changed();
			}
		});
		ui.label(RichText::new(format!("{n} of {} screens", AppConfig::MAX_SCREENS)).small().weak());
		for (i, s) in self.config.screens.iter().enumerate() {
			if ui.selectable_label(i == self.selected, screen_label(i, s)).clicked() {
				self.selected = i;
			}
		}
	}

	fn screen_settings(&mut self, ui: &mut egui::Ui) -> Option<PathBuf> {
		let mut request = None;
		let dir = self.dir();
		let screen = self.config.screens.get_mut(self.selected)?;
		let mut changed = false;
		ui.heading(format!("Screen {}", self.selected + 1));
		egui::Grid::new("type").num_columns(2).show(ui, |ui| {
			ui.label("Type");
			let mut t = screen.gauge.gauge_type;
			ComboBox::from_id_salt("gauge-type").selected_text(t.name()).show_ui(ui, |ui| {
				for option in GaugeType::ALL {
					ui.selectable_value(&mut t, option, option.name());
				}
			});
			if t != screen.gauge.gauge_type {
				screen.gauge.gauge_type = t;
				if t == GaugeType::Grid && screen.gauge.cells.len() < 4 {
					// The device editor starts a grid with four cells too.
					screen.gauge.cells.resize_with(4, GridCellConfig::default);
				}
				changed = true;
			}
			ui.end_row();
			if t == GaugeType::TurboXs {
				ui.label("Status");
				let mut mode = screen.gauge.status_mode.clone();
				ComboBox::from_id_salt("status-mode").selected_text(&mode).show_ui(ui, |ui| {
					for m in STATUS_MODES {
						ui.selectable_value(&mut mode, m.to_string(), m);
					}
				});
				if mode != screen.gauge.status_mode {
					screen.gauge.status_mode = mode;
					changed = true;
				}
				ui.end_row();
			}
		});

		let sources: Vec<String> = me_data::DATA_SOURCE_NAMES.iter().map(|s| s.to_string()).collect();
		let ctx = PropContext { sources: &sources, start_dir: dir.as_deref() };
		match screen.gauge.gauge_type {
			GaugeType::Custom => {
				ui.label("Custom definition file (relative to gauges.json)");
				let mut file = screen.custom_definition_file.clone().unwrap_or_default();
				ui.horizontal(|ui| {
					if ui.button("…").clicked() {
						let mut dialog = rfd::FileDialog::new().add_filter("Gauge JSON", &["json"]);
						if let Some(d) = &dir {
							dialog = dialog.set_directory(d);
						}
						if let Some(p) = dialog.pick_file() {
							file = relative_to(&p, dir.as_deref());
						}
					}
					ui.add(egui::TextEdit::singleline(&mut file).desired_width(f32::INFINITY));
				});
				if Some(file.as_str()) != screen.custom_definition_file.as_deref()
					&& !(file.is_empty() && screen.custom_definition_file.is_none())
				{
					screen.custom_definition_file = (!file.is_empty()).then(|| file.clone());
					changed = true;
				}
				if ui.add_enabled(!file.is_empty(), egui::Button::new("Open in designer")).clicked() {
					request = Some(gauge_render::images::resolve(&file, dir.as_deref()));
				}
			}
			GaugeType::Grid => {
				let mut remove = None;
				for (i, cell) in screen.gauge.cells.iter_mut().enumerate() {
					ui.separator();
					ui.horizontal(|ui| {
						ui.label(RichText::new(format!("Cell {}", i + 1)).strong());
						if ui.small_button("✖").clicked() {
							remove = Some(i);
						}
					});
					changed |= edit_json(ui, &format!("cell{i}"), CELL, cell, &ctx);
				}
				if let Some(i) = remove {
					screen.gauge.cells.remove(i);
					changed = true;
				}
				if ui.button("+ Add cell").clicked() {
					screen.gauge.cells.push(GridCellConfig::default());
					changed = true;
				}
			}
			t => {
				let fields: Vec<Field> = COMMON.iter().chain(extra(t)).copied().collect();
				changed |= edit_json(ui, "gauge", &fields, &mut screen.gauge, &ctx);
			}
		}
		if changed {
			self.changed();
		}
		if let Some(Some(e)) = self.errors.get(self.selected) {
			ui.label(RichText::new(format!("⛔ {e}")).color(props::ERROR));
		}
		request
	}

	fn preview_grid(&mut self, ui: &mut egui::Ui) {
		ui.horizontal(|ui| {
			ui.heading("Preview");
			ui.add(egui::Slider::new(&mut self.preview_percent, 0..=100).suffix(" %"))
				.on_hover_text("Where in each screen's range the values sit");
		});
		egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
			let width = (ui.available_width() - 8.0).max(120.0);
			let size = vec2(width, width * PREVIEW_H as f32 / PREVIEW_W as f32);
			for (i, tex) in self.previews.iter().enumerate() {
				let Some(tex) = tex else { continue };
				let label = self.config.screens.get(i).map(|s| screen_label(i, s)).unwrap_or_default();
				ui.label(if i == self.selected { RichText::new(label).strong() } else { RichText::new(label) });
				let resp = ui.add(egui::Image::new((tex.id(), size)).sense(egui::Sense::click()));
				if i == self.selected {
					ui.painter().rect_stroke(
						resp.rect,
						0.0,
						egui::Stroke::new(2.0, Color32::from_rgb(0x4D, 0xC3, 0xFF)),
						egui::StrokeKind::Outside,
					);
				}
				if resp.clicked() {
					self.selected = i;
				}
				ui.add_space(6.0);
			}
		});
	}
}

fn screen_label(i: usize, s: &ScreenConfig) -> String {
	let g = &s.gauge;
	let what = match g.gauge_type {
		GaugeType::Custom => s.custom_definition_file.clone().unwrap_or_else(|| "(no file)".into()),
		GaugeType::Grid => format!("{} cells", g.cells.len()),
		_ if !g.title.is_empty() => g.title.clone(),
		_ => g.data_source.clone(),
	};
	format!("{}. {} · {what}", i + 1, g.gauge_type.name())
}

/// Edits a serde struct through its JSON form with the field table.
fn edit_json<T: serde::Serialize + serde::de::DeserializeOwned>(
	ui: &mut egui::Ui,
	grid_id: &str,
	fields: &[Field],
	target: &mut T,
	ctx: &PropContext,
) -> bool {
	let Ok(mut value) = serde_json::to_value(&*target) else { return false };
	let Some(obj) = value.as_object_mut() else { return false };
	if props::edit_fields(ui, grid_id, fields, obj, ctx).is_none() {
		return false;
	}
	// These configs have no optional strings: "(none)" means empty.
	for v in obj.values_mut() {
		if v.is_null() {
			*v = Value::String(String::new());
		}
	}
	match serde_json::from_value(value) {
		Ok(t) => {
			*target = t;
			true
		}
		Err(_) => false,
	}
}

/// Each data source of a custom definition with the widest range bound to it.
fn custom_ranges(def: &gauge_render::CustomGaugeDefinition) -> Vec<(String, f32, f32)> {
	let mut out: Vec<(String, f32, f32)> = Vec::new();
	for e in &def.elements {
		let b = e.base();
		if let Some(s) = b.source() {
			match out.iter_mut().find(|r| gauge_render::eq_ignore_case(&r.0, s)) {
				Some(r) => (r.1, r.2) = (r.1.min(b.min_value), r.2.max(b.max_value)),
				None => out.push((s.to_string(), b.min_value, b.max_value)),
			}
		}
	}
	out
}

/// `path` relative to `dir` when it's inside it (with forward slashes), else absolute.
fn relative_to(path: &Path, dir: Option<&Path>) -> String {
	dir.and_then(|d| path.strip_prefix(d).ok())
		.map(|rel| rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("/"))
		.unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
	use super::*;
	use gauge_render::gauges::GaugeConfig;

	#[test]
	fn fields_exist_in_the_config() {
		let value = serde_json::to_value(GaugeConfig::default()).unwrap();
		for t in GaugeType::ALL {
			for field in COMMON.iter().chain(extra(t)) {
				assert!(value.get(field.key).is_some(), "{}", field.key);
			}
		}
		let cell = serde_json::to_value(GridCellConfig::default()).unwrap();
		for field in CELL {
			assert!(cell.get(field.key).is_some(), "{}", field.key);
		}
	}

	#[test]
	fn relative_paths_use_forward_slashes() {
		assert_eq!(relative_to(Path::new("/a/b/custom/x.json"), Some(Path::new("/a/b"))), "custom/x.json");
		assert_eq!(relative_to(Path::new("/c/x.json"), Some(Path::new("/a/b"))), "/c/x.json");
	}

	#[test]
	fn custom_ranges_merge_names_ignoring_unicode_case() {
		let def = gauge_render::CustomGaugeDefinition::from_json(
			r##"{ "Elements": [ { "$type": "arc", "DataSource": "Öltemp", "MaxValue": 10 },
				{ "$type": "arc", "DataSource": "öltemp", "MinValue": -5, "MaxValue": 3 } ] }"##,
		)
		.unwrap();
		assert_eq!(custom_ranges(&def), [("Öltemp".to_string(), -5.0, 10.0)]);
	}
}
