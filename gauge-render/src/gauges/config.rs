//! `gauges.json` model (`AppConfig`, `ScreenConfig`, `GaugeConfig`).

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum GaugeType {
	#[default]
	Bar,
	Circular,
	Histogram,
	Needle,
	Digital,
	Sweep,
	MinMax,
	Grid,
	Custom,
	TurboXs,
	EvaDial,
	EvaHoneycomb,
	EvaReadout,
	EvaWarning,
	EvaGear,
}

impl GaugeType {
	pub const ALL: [GaugeType; 15] = [
		Self::Bar,
		Self::Circular,
		Self::Histogram,
		Self::Needle,
		Self::Digital,
		Self::Sweep,
		Self::MinMax,
		Self::Grid,
		Self::Custom,
		Self::TurboXs,
		Self::EvaDial,
		Self::EvaHoneycomb,
		Self::EvaReadout,
		Self::EvaWarning,
		Self::EvaGear,
	];

	/// The C# enum name (also its JSON spelling).
	pub fn name(self) -> &'static str {
		match self {
			Self::Bar => "Bar",
			Self::Circular => "Circular",
			Self::Histogram => "Histogram",
			Self::Needle => "Needle",
			Self::Digital => "Digital",
			Self::Sweep => "Sweep",
			Self::MinMax => "MinMax",
			Self::Grid => "Grid",
			Self::Custom => "Custom",
			Self::TurboXs => "TurboXs",
			Self::EvaDial => "EvaDial",
			Self::EvaHoneycomb => "EvaHoneycomb",
			Self::EvaReadout => "EvaReadout",
			Self::EvaWarning => "EvaWarning",
			Self::EvaGear => "EvaGear",
		}
	}

	/// `Enum.TryParse(s, ignoreCase: true)`: names in any case, numeric
	/// strings and comma lists. A number with no matching member gives
	/// `Bar`, which is what `GaugeFactory` builds for such a value.
	pub fn parse(s: &str) -> Option<Self> {
		let names = Self::ALL.map(Self::name);
		enum_try_parse(s, &names).map(Self::from_value)
	}

	/// The member for a C# enum value; values with no member give `Bar`.
	fn from_value(n: i64) -> Self {
		usize::try_from(n).ok().and_then(|n| Self::ALL.get(n).copied()).unwrap_or_default()
	}
}

/// `Enum.TryParse(s, ignoreCase: true, out _)` for an enum whose members are
/// `names` with the values 0, 1, 2, ... Returns the parsed value, which can
/// be one with no member, as in .NET: surrounding whitespace is ignored, a
/// string starting with a digit, `+` or `-` is read as an `int`, and a comma
/// list of names gives the members' values OR-ed together.
pub fn enum_try_parse(s: &str, names: &[&str]) -> Option<i64> {
	let s = s.trim();
	if s.is_empty() {
		return None;
	}
	if s.starts_with(|c: char| c.is_ascii_digit() || c == '+' || c == '-') {
		if let Ok(n) = s.parse::<i32>() {
			return Some(n as i64);
		}
	}
	let mut value = 0i64;
	for part in s.split(',') {
		let part = part.trim();
		let i = names.iter().position(|n| n.eq_ignore_ascii_case(part))?;
		value |= i as i64;
	}
	Some(value)
}

/// Mirrors `TolerantGaugeTypeConverter`: names (any case) or numbers, with
/// unknown values falling back to `Bar` instead of failing the whole file.
impl<'de> Deserialize<'de> for GaugeType {
	fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
		let v = serde_json::Value::deserialize(d)?;
		Ok(match v {
			serde_json::Value::String(s) => Self::parse(&s).unwrap_or_default(),
			serde_json::Value::Number(n) => n.as_i64().map(Self::from_value).unwrap_or_default(),
			_ => Self::default(),
		})
	}
}

/// A JSON `null` reads as the type's default (`""`, an empty list) instead of
/// failing the whole file. System.Text.Json stores the `null`, and the C#
/// app treats a null string or list as empty.
fn null_as_default<'de, D: Deserializer<'de>, T: Deserialize<'de> + Default>(d: D) -> Result<T, D::Error> {
	Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct GridCellConfig {
	#[serde(deserialize_with = "null_as_default")]
	pub data_source: String,
	#[serde(deserialize_with = "null_as_default")]
	pub title: String,
	#[serde(deserialize_with = "null_as_default")]
	pub unit: String,
	pub decimals: i32,
}

impl Default for GridCellConfig {
	fn default() -> Self {
		Self { data_source: "Rpm".into(), title: String::new(), unit: String::new(), decimals: 0 }
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct GaugeConfig {
	#[serde(rename = "Type")]
	pub gauge_type: GaugeType,
	#[serde(deserialize_with = "null_as_default")]
	pub data_source: String,
	#[serde(deserialize_with = "null_as_default")]
	pub color_hex: String,
	#[serde(deserialize_with = "null_as_default")]
	pub title: String,
	#[serde(deserialize_with = "null_as_default")]
	pub unit: String,
	pub min_value: f32,
	pub max_value: f32,
	pub initial_value: f32,
	pub segment_count: i32,
	pub smoothing: bool,
	pub decimals: i32,
	pub max_data_points: i32,
	pub interval_ms: i32,
	/// TurboXs: `Peak`, `Min`, `Text` or `None` (the status block under the title).
	#[serde(deserialize_with = "null_as_default")]
	pub status_mode: String,
	/// TurboXs: first status line; empty picks `Peak:`, `Min:` or `Datalog:`.
	#[serde(deserialize_with = "null_as_default")]
	pub status_label: String,
	/// TurboXs: second status line in `Text` mode.
	#[serde(deserialize_with = "null_as_default")]
	pub status_text: String,
	/// TurboXs: old-LCD look (low resolution, pixel grid, backlight tint, ghosting).
	pub lcd: bool,
	/// TurboXs: share of the previous frame that lingers in Lcd mode (0 = none, up to 0.9).
	pub lcd_ghosting: f32,
	#[serde(deserialize_with = "null_as_default")]
	pub cells: Vec<GridCellConfig>,
	/// Eva gauges: the limit (redline, overboost, overheat); unset takes a share of the range.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub warn_value: Option<f32>,
	/// EvaReadout: the lower zone edge (rich below it on an AFR channel).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub warn_low_value: Option<f32>,
}

impl Default for GaugeConfig {
	fn default() -> Self {
		Self {
			gauge_type: GaugeType::Bar,
			data_source: "AfrCurr1".into(),
			color_hex: "#00FFFF".into(),
			title: String::new(),
			unit: String::new(),
			min_value: 0.0,
			max_value: 100.0,
			initial_value: 0.0,
			segment_count: 32,
			smoothing: true,
			decimals: 0,
			max_data_points: 50,
			interval_ms: 1000,
			status_mode: "Peak".into(),
			status_label: String::new(),
			status_text: "Stop".into(),
			lcd: false,
			lcd_ghosting: 0.2,
			cells: Vec::new(),
			warn_value: None,
			warn_low_value: None,
		}
	}
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ScreenConfig {
	#[serde(deserialize_with = "null_as_default")]
	pub gauge: GaugeConfig,
	pub custom_definition_file: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct AppConfig {
	pub demo_mode: bool,
	pub simulate_battery_percent: Option<i32>,
	pub device_mac_address: Option<String>,
	#[serde(deserialize_with = "null_as_default")]
	pub screens: Vec<ScreenConfig>,
}

impl Default for AppConfig {
	fn default() -> Self {
		Self {
			demo_mode: false,
			simulate_battery_percent: None,
			device_mac_address: Some("20:E7:C8:B9:20:2A".into()),
			screens: Vec::new(),
		}
	}
}

impl AppConfig {
	/// Most screens a config may hold (`AppConfig.MaxScreens`).
	pub const MAX_SCREENS: usize = 10;

	pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
		serde_json::from_str(json.trim_start_matches('\u{feff}'))
	}

	/// The config as System.Text.Json writes it, byte for byte: indented,
	/// non-ASCII and HTML-sensitive characters escaped, floats without a
	/// trailing `.0`.
	pub fn to_json(&self) -> String {
		let mut out = Vec::new();
		let mut ser = serde_json::Serializer::with_formatter(&mut out, DotNetFormatter::default());
		self.serialize(&mut ser).expect("config serialises");
		String::from_utf8(out).expect("escaped JSON is ASCII")
	}

	/// The config written when none exists (`ConfigService.CreateDefault`).
	pub fn default_config() -> Self {
		Self {
			screens: vec![ScreenConfig {
				gauge: GaugeConfig {
					gauge_type: GaugeType::Bar,
					data_source: "AfrCurr1".into(),
					color_hex: "#00FFFF".into(),
					title: "AFR".into(),
					unit: String::new(),
					min_value: 8.0,
					max_value: 18.0,
					initial_value: 14.7,
					decimals: 2,
					segment_count: 32,
					smoothing: true,
					..GaugeConfig::default()
				},
				custom_definition_file: None,
			}],
			..Self::default()
		}
	}

	/// Loads `path` like `ConfigService.Load`: a missing or empty config is
	/// replaced by the default (and written back), and extra screens beyond
	/// [`Self::MAX_SCREENS`] are dropped. A config that exists but can't be
	/// parsed (say, truncated by a power cut) is first copied to
	/// `gauges.json.bad` so the user's settings can still be recovered.
	pub fn load_or_create(path: &std::path::Path) -> Self {
		let loaded = match std::fs::read_to_string(path) {
			Ok(json) => match Self::from_json(&json) {
				Ok(c) => Some(c),
				Err(e) => {
					eprintln!("[config] {}: {e}; using defaults", path.display());
					backup_bad(path);
					None
				}
			},
			Err(_) => None,
		};
		let mut config = match loaded {
			Some(c) if !c.screens.is_empty() => c,
			_ => {
				let c = Self::default_config();
				if let Err(e) = c.save(path) {
					eprintln!("[config] could not write {}: {e}", path.display());
				}
				c
			}
		};
		config.screens.truncate(Self::MAX_SCREENS);
		config
	}

	/// Writes `path` atomically: the JSON goes to `<path>.tmp`, is fsynced,
	/// then renamed over `path`, and the directory is fsynced. A power cut
	/// on the SD card leaves either the old or the new file, never a
	/// truncated one.
	pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
		use std::io::Write;
		let tmp = sibling(path, ".tmp");
		let result = (|| {
			let mut file = std::fs::File::create(&tmp)?;
			file.write_all(self.to_json().as_bytes())?;
			file.sync_all()?;
			drop(file);
			std::fs::rename(&tmp, path)
		})();
		if result.is_err() {
			let _ = std::fs::remove_file(&tmp);
		}
		result?;
		// Makes the rename itself durable. Not every filesystem lets a
		// directory be opened or synced, and the data is already safe.
		let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
		if let Ok(d) = std::fs::File::open(dir) {
			let _ = d.sync_all();
		}
		Ok(())
	}
}

/// Writes JSON like `Utf8JsonWriter` with `WriteIndented` and the default
/// `JavaScriptEncoder`: two-space indents, everything outside printable
/// ASCII plus ``"&'+<>` `` as `\uXXXX`, and floats in their shortest form.
#[derive(Default)]
struct DotNetFormatter {
	pretty: serde_json::ser::PrettyFormatter<'static>,
}

impl DotNetFormatter {
	fn escape<W: ?Sized + std::io::Write>(writer: &mut W, units: &[u16]) -> std::io::Result<()> {
		for u in units {
			write!(writer, "\\u{u:04X}")?;
		}
		Ok(())
	}
}

impl serde_json::ser::Formatter for DotNetFormatter {
	fn write_f32<W: ?Sized + std::io::Write>(&mut self, writer: &mut W, value: f32) -> std::io::Result<()> {
		writer.write_all(crate::format::format_number(value, "R").as_bytes())
	}

	fn write_string_fragment<W: ?Sized + std::io::Write>(
		&mut self,
		writer: &mut W,
		fragment: &str,
	) -> std::io::Result<()> {
		for c in fragment.chars() {
			if c.is_ascii() && !c.is_ascii_control() && !matches!(c, '&' | '\'' | '+' | '<' | '>' | '`') {
				writer.write_all(&[c as u8])?;
			} else {
				Self::escape(writer, c.encode_utf16(&mut [0; 2]))?;
			}
		}
		Ok(())
	}

	fn write_char_escape<W: ?Sized + std::io::Write>(
		&mut self,
		writer: &mut W,
		escape: serde_json::ser::CharEscape,
	) -> std::io::Result<()> {
		use serde_json::ser::CharEscape;
		match escape {
			CharEscape::Quote => Self::escape(writer, &[0x22]),
			CharEscape::ReverseSolidus => writer.write_all(b"\\\\"),
			CharEscape::Solidus => writer.write_all(b"/"),
			CharEscape::Backspace => writer.write_all(b"\\b"),
			CharEscape::FormFeed => writer.write_all(b"\\f"),
			CharEscape::LineFeed => writer.write_all(b"\\n"),
			CharEscape::CarriageReturn => writer.write_all(b"\\r"),
			CharEscape::Tab => writer.write_all(b"\\t"),
			CharEscape::AsciiControl(b) => Self::escape(writer, &[b.into()]),
		}
	}

	fn begin_array<W: ?Sized + std::io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
		self.pretty.begin_array(w)
	}
	fn end_array<W: ?Sized + std::io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
		self.pretty.end_array(w)
	}
	fn begin_array_value<W: ?Sized + std::io::Write>(&mut self, w: &mut W, first: bool) -> std::io::Result<()> {
		self.pretty.begin_array_value(w, first)
	}
	fn end_array_value<W: ?Sized + std::io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
		self.pretty.end_array_value(w)
	}
	fn begin_object<W: ?Sized + std::io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
		self.pretty.begin_object(w)
	}
	fn end_object<W: ?Sized + std::io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
		self.pretty.end_object(w)
	}
	fn begin_object_key<W: ?Sized + std::io::Write>(&mut self, w: &mut W, first: bool) -> std::io::Result<()> {
		self.pretty.begin_object_key(w, first)
	}
	fn begin_object_value<W: ?Sized + std::io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
		self.pretty.begin_object_value(w)
	}
	fn end_object_value<W: ?Sized + std::io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
		self.pretty.end_object_value(w)
	}
}

/// `path` with `suffix` appended to its file name (`gauges.json.tmp`).
fn sibling(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
	let mut name = path.file_name().unwrap_or_default().to_os_string();
	name.push(suffix);
	path.with_file_name(name)
}

/// Keeps an unparsable config as `gauges.json.bad` before the defaults
/// replace it.
fn backup_bad(path: &std::path::Path) {
	let bad = sibling(path, ".bad");
	match std::fs::copy(path, &bad) {
		Ok(_) => eprintln!("[config] kept the unreadable config as {}", bad.display()),
		Err(e) => eprintln!("[config] could not back up {}: {e}", path.display()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn load_or_create_writes_default_and_caps_screens() {
		let dir = std::env::temp_dir().join(format!("gauge-config-{}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("gauges.json");
		let _ = std::fs::remove_file(&path);

		let c = AppConfig::load_or_create(&path);
		assert_eq!(c.screens.len(), 1);
		assert_eq!(c.screens[0].gauge.data_source, "AfrCurr1");
		let written = std::fs::read_to_string(&path).unwrap();
		assert!(written.contains("\"Type\": \"Bar\""), "{written}");

		let mut many = AppConfig::default_config();
		many.screens = (0..14).map(|_| many.screens[0].clone()).collect();
		many.save(&path).unwrap();
		assert_eq!(AppConfig::load_or_create(&path).screens.len(), AppConfig::MAX_SCREENS);

		std::fs::write(&path, "{ not json").unwrap();
		assert_eq!(AppConfig::load_or_create(&path).screens.len(), 1);
		let bad = dir.join("gauges.json.bad");
		assert_eq!(std::fs::read_to_string(&bad).unwrap(), "{ not json", "the broken config is kept");
		assert!(!dir.join("gauges.json.tmp").exists(), "save leaves no temp file");
		let _ = std::fs::remove_dir_all(dir);
	}

	#[test]
	fn tolerant_gauge_type() {
		let cfg = AppConfig::from_json(
			r#"{"Screens":[
				{"Gauge":{"Type":"needle","MaxValue":8000}},
				{"Gauge":{"Type":3}},
				{"Gauge":{"Type":"Legacy"}},
				{"Gauge":{"Type":"Custom"},"CustomDefinitionFile":"custom-gauge.json"}
			]}"#,
		)
		.unwrap();
		let types: Vec<_> = cfg.screens.iter().map(|s| s.gauge.gauge_type).collect();
		assert_eq!(types, [GaugeType::Needle, GaugeType::Needle, GaugeType::Bar, GaugeType::Custom]);
		assert_eq!(cfg.screens[0].gauge.max_value, 8000.0);
		assert_eq!(cfg.screens[0].gauge.segment_count, 32);
		assert_eq!(cfg.screens[3].custom_definition_file.as_deref(), Some("custom-gauge.json"));
	}

	#[test]
	fn gauge_type_parses_like_enum_try_parse() {
		assert_eq!(GaugeType::parse(" Needle "), Some(GaugeType::Needle));
		assert_eq!(GaugeType::parse("3"), Some(GaugeType::Needle));
		assert_eq!(GaugeType::parse("+9"), Some(GaugeType::TurboXs));
		assert_eq!(GaugeType::parse("Circular, Histogram"), Some(GaugeType::Needle), "1 | 2");
		assert_eq!(GaugeType::parse("42"), Some(GaugeType::Bar), "no member: GaugeFactory builds a bar");
		assert_eq!(GaugeType::parse("Circular, Legacy"), None);
		assert_eq!(GaugeType::parse(""), None);
	}

	#[test]
	fn json_is_written_like_system_text_json() {
		let mut cfg = AppConfig::default_config();
		cfg.screens[0].gauge.unit = "\u{b0}C <&> 'x' \"q\" \\ \u{1F600}".into();
		let json = cfg.to_json();
		let unit = r#""Unit": "\u00B0C \u003C\u0026\u003E \u0027x\u0027 \u0022q\u0022 \\ \uD83D\uDE00","#;
		assert!(json.contains(unit), "{json}");
		assert!(json.contains(r#""MinValue": 8,"#), "{json}");
		assert!(json.contains(r#""InitialValue": 14.7,"#), "{json}");
		assert!(json.contains("\n  \"Screens\": [\n    {\n      \"Gauge\": {\n"), "{json}");
		assert_eq!(AppConfig::from_json(&json).unwrap().screens[0].gauge.unit, cfg.screens[0].gauge.unit);
	}

	#[test]
	fn nulls_read_as_empty() {
		let cfg = AppConfig::from_json(
			r#"{"Screens":[{"Gauge":{"Title":null,"Unit":null,"Cells":null,"StatusMode":null},"CustomDefinitionFile":null}]}"#,
		)
		.unwrap();
		let g = &cfg.screens[0].gauge;
		assert_eq!((g.title.as_str(), g.unit.as_str(), g.cells.len()), ("", "", 0));
		assert!(AppConfig::from_json(r#"{"Screens":null}"#).unwrap().screens.is_empty());
	}
}
