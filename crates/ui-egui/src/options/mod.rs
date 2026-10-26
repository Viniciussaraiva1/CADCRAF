//! OPTIONS: the user's preferences, in one dialog and one JSON form.
//!
//! Typed (OPTIONS, OP), chosen from Tools ▸ Options… or, on macOS, CADCraft ▸ Settings… (⌘,),
//! OPTIONS opens the Options dialog ([`dialog`]). A JSON call never opens it: `options {}` reports
//! every preference and `options {"PICKBOX": 5, "modelBackground": "#ffffff"}` changes some (all
//! or nothing; `"reset": true` restores the defaults first). Upper-case names are the engine's
//! profile variables (`cadcraft_engine::cmd::options`); camel-case names are the interface's own:
//!
//! | name | value |
//! |---|---|
//! | `theme` | `"system"`, `"light"` or `"dark"` |
//! | `animations` | `"system"`, `"on"` or `"off"`: dialog and menu fades and smoothed scrolling |
//! | `interfaceLanguage` | `"auto"`, `"en"`, `"uk"`, `"pt-BR"`, `"de"`, `"es"`, `"cs"`, `"ru"`, `"fr"`, `"zh"`, `"it"`, `"ko"`, `"sv"`, `"ar"`, `"hu"`, `"tr"` or `"el"` |
//! | `saveFormat` | `"dxf"` or `"dwg"`: the format Save As suggests for new drawings |
//! | `saveVersion` | `"2000"` … `"2018"`: the version drawings with no file yet are saved in |
//! | `SAVETIME` | minutes between automatic saves, 0–600 (0: off) |
//! | `SAVEFILEPATH` | the folder automatic saves go to |
//! | `timeSensitiveRightClick` | while a command runs, a quick right-click is Enter and holding the button opens the shortcut menu (default); `false`: a right-click always opens the menu |
//! | `systemCursor` | keep the system cursor visible over the drawing area |
//! | `historyLines` | command-line history lines, 0–12 |
//! | `modelBackground`, `paperBackground` | `"#rrggbb"`, or `null` for the default |
//! | `selectionColor` | the colour selected objects are highlighted in: `"#rrggbb"`, or `null` for the theme's |
//! | `snapMarkerSize` | object snap marker size in points, 5–31 |
//! | `interfaceScale` | the interface's size, 0.5–3 (1: the system's own) |
//! | `dimStyle` | `{"DIMDEC": 2, "DIMDSEP": ","}`: dimension variables ([`DIM_OPTION_VARS`]) set on the current dimension style of the open drawing and kept for the drawings made from now on; reported as the current style's values |
//! | `dimDefaults` | the same kind of object, kept for new drawings only (`null` or `{}` forgets them) |
//! | `QNEWTEMPLATE` | the template file new drawings are made from (`""`: none) |
//! | `FONTALT` | the font that replaces a text style font that can't be found (`""`: the built-in one) |
//! | `FONTFALLBACK` | fonts searched for characters a text's font lacks, separated by commas |
//!
//! SAVETIME and SAVEFILEPATH are the engine's autosave settings, kept with the autosave state
//! rather than in the profile.
//!
//! The profile variables and the interface preferences survive restarts: the host saves
//! [`CadApp::profile_json`] under [`PROFILE_KEY`] next to the interface prefs and restores it with
//! [`CadApp::load_profile`].

mod advanced;
mod dialog;
mod dimensions;
mod locations;

pub use dialog::dialog;

use egui::Color32;
use serde_json::{Map, Value, json};

use crate::CadApp;
use crate::animation::AnimationPref;
use crate::theme::{self, ThemePref};

/// The host storage key for [`CadApp::profile_json`].
pub const PROFILE_KEY: &str = "cadcraft.profile";

/// The largest saved profile [`CadApp::load_profile`] reads.
const MAX_PROFILE_BYTES: usize = 64 * 1024;

/// The longest SAVEFILEPATH the JSON form accepts.
const MAX_PATH_CHARS: usize = 1024;

/// Object snap marker sizes the Options dialog offers, in points.
pub const SNAP_MARKER_SIZES: std::ops::RangeInclusive<f32> = 5.0..=31.0;

/// Interface scales the JSON form accepts (1: the system's own size).
pub const INTERFACE_SCALES: std::ops::RangeInclusive<f32> = 0.5..=3.0;

/// The dimension variables the Options dialog and the `dimStyle` / `dimDefaults` JSON keys edit:
/// units and precision, the decimal separator, zero suppression, rounding, prefix and suffix,
/// angular units, text and arrow sizes.
pub const DIM_OPTION_VARS: &[&str] = &[
    "DIMLUNIT", "DIMDEC", "DIMDSEP", "DIMZIN", "DIMRND", "DIMLFAC", "DIMPOST", "DIMAUNIT", "DIMADEC", "DIMAZIN", "DIMFRAC", "DIMTXT", "DIMGAP",
    "DIMTAD", "DIMASZ", "DIMEXO", "DIMEXE", "DIMSCALE",
];

/// A `dimStyle` / `dimDefaults` object, checked: only [`DIM_OPTION_VARS`] (any case), and values a
/// dimension style takes. `null` is an empty object.
fn dim_map(key: &str, v: &Value) -> Result<Map<String, Value>, String> {
    let obj = match v {
        Value::Null => return Ok(Map::new()),
        Value::Object(o) => o,
        _ => return Err(format!("{key} must be an object of dimension variables")),
    };
    let mut out = Map::new();
    for (name, value) in obj {
        let n = name.trim().to_ascii_uppercase();
        if !DIM_OPTION_VARS.contains(&n.as_str()) {
            return Err(format!("{key}: {name} is not a dimension option ({})", DIM_OPTION_VARS.join(", ")));
        }
        out.insert(n, value.clone());
    }
    let rejected = cadcraft_engine::doc::DimStyle::default().apply_fields(&out);
    if !rejected.is_empty() {
        return Err(format!("{key}: invalid value for {}", rejected.join(", ")));
    }
    Ok(out)
}

/// The interface's own preferences (the drafting ones are engine settings).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiOptions {
    /// Model-space background; `None` keeps the theme's.
    pub model_background: Option<Color32>,
    /// The sheet colour on layout tabs; `None` is white.
    pub paper_background: Option<Color32>,
    /// Object snap marker size in points.
    pub snap_marker_size: f32,
    /// The selection highlight colour; `None` keeps the theme's.
    pub selection_color: Option<Color32>,
    /// While a command runs: a quick right-click is Enter and a held one opens the shortcut menu
    /// (AutoCAD's time-sensitive right-click). Off: a right-click always opens the menu.
    pub time_sensitive_right_click: bool,
    /// The size of every menu, palette, dialog and the command line (1: the system's own).
    pub interface_scale: f32,
}

impl Default for UiOptions {
    fn default() -> Self {
        UiOptions {
            model_background: None,
            paper_background: None,
            snap_marker_size: theme::SNAP_MARKER,
            selection_color: None,
            time_sensitive_right_click: true,
            interface_scale: 1.0,
        }
    }
}

impl UiOptions {
    /// What the drawing area paints with.
    pub fn canvas(&self) -> theme::Canvas {
        theme::Canvas {
            model: self.model_background,
            paper: self.paper_background,
            snap_marker: self.snap_marker_size,
            selection: self.selection_color,
        }
    }
}

/// `"#rrggbb"`.
pub fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// Parse `"#rrggbb"` (the `#` is optional).
pub fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

fn color_json(c: Option<Color32>) -> Value {
    c.map_or(Value::Null, |c| json!(hex(c)))
}

/// Open the dialog for a typed or menu-invoked OPTIONS; run the JSON form for a call with
/// parameters. `None` if `id` isn't OPTIONS.
pub fn route(app: &mut CadApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !matches!(id, "options" | "op" | "preferences") {
        return None;
    }
    if params.is_null() {
        app.ui.dialog = Some("options".into());
        return Some(Ok(Value::Null));
    }
    Some(run_json(app, params))
}

/// The interface preferences a JSON call may set, parsed before anything changes.
#[derive(Default)]
struct UiChanges {
    theme: Option<ThemePref>,
    animations: Option<AnimationPref>,
    language: Option<crate::i18n::Preference>,
    save_format: Option<crate::SaveFormat>,
    system_cursor: Option<bool>,
    history_lines: Option<usize>,
    model_background: Option<Option<Color32>>,
    paper_background: Option<Option<Color32>>,
    selection_color: Option<Option<Color32>>,
    snap_marker_size: Option<f32>,
    save_version: Option<crate::SaveVersion>,
    savetime: Option<u32>,
    savefilepath: Option<String>,
    time_sensitive_right_click: Option<bool>,
    interface_scale: Option<f32>,
    dim_style: Option<Map<String, Value>>,
    dim_defaults: Option<Map<String, Value>>,
    qnew_template: Option<String>,
    font_alt: Option<String>,
    font_fallback: Option<String>,
}

/// A colour setting of the JSON form: `"#rrggbb"`, or null for the theme's own.
fn color_param(key: &str, v: &Value) -> Result<Option<Color32>, String> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => parse_hex(s).map(Some).ok_or_else(|| format!("{key} must be \"#rrggbb\" or null")),
        _ => Err(format!("{key} must be \"#rrggbb\" or null")),
    }
}

/// Read one interface key of the OPTIONS JSON form into `c`: `Ok(true)` when it is one, `Ok(false)`
/// for a key that belongs to the engine, an error for a bad value.
fn parse_ui(key: &str, v: &Value, c: &mut UiChanges) -> Result<bool, String> {
    let text = || v.as_str().ok_or_else(|| format!("{key} must be text"));
    match key {
        "theme" => c.theme = Some(ThemePref::parse(text()?).ok_or("theme must be system, light or dark")?),
        "animations" => c.animations = Some(AnimationPref::parse(text()?).ok_or("animations must be system, on or off")?),
        "interfaceLanguage" => {
            c.language = Some(
                crate::i18n::Preference::parse(text()?).ok_or_else(|| format!("interfaceLanguage must be {}", crate::i18n::Preference::codes()))?,
            )
        }
        "saveFormat" => c.save_format = Some(crate::SaveFormat::parse(text()?).ok_or("saveFormat must be dxf or dwg")?),
        "systemCursor" => c.system_cursor = Some(v.as_bool().ok_or("systemCursor must be true or false")?),
        "historyLines" => {
            let n = v.as_u64().filter(|n| *n <= crate::cmdline::MAX_LINES as u64).ok_or("historyLines must be a whole number from 0 to 12")?;
            c.history_lines = Some(n as usize);
        }
        "modelBackground" => c.model_background = Some(color_param(key, v)?),
        "paperBackground" => c.paper_background = Some(color_param(key, v)?),
        "selectionColor" => c.selection_color = Some(color_param(key, v)?),
        "snapMarkerSize" => {
            let n = v.as_f64().filter(|n| SNAP_MARKER_SIZES.contains(&(*n as f32)) && n.is_finite()).ok_or("snapMarkerSize must be from 5 to 31")?;
            c.snap_marker_size = Some(n as f32);
        }
        "interfaceScale" => {
            let n = v.as_f64().filter(|n| n.is_finite() && INTERFACE_SCALES.contains(&(*n as f32))).ok_or("interfaceScale must be from 0.5 to 3")?;
            c.interface_scale = Some(n as f32);
        }
        "saveVersion" => {
            let name = v.as_str().map_or_else(|| v.to_string(), str::to_string);
            c.save_version = Some(crate::SaveVersion::parse(name.trim()).ok_or("saveVersion must be 2000, 2004, 2007, 2010, 2013 or 2018")?);
        }
        "timeSensitiveRightClick" => c.time_sensitive_right_click = Some(v.as_bool().ok_or("timeSensitiveRightClick must be true or false")?),
        "dimStyle" => c.dim_style = Some(dim_map(key, v)?),
        "dimDefaults" => c.dim_defaults = Some(dim_map(key, v)?),
        k if k.eq_ignore_ascii_case("SAVETIME") => {
            let max = cadcraft_engine::autosave::MAX_SAVETIME;
            let n = v
                .as_f64()
                .filter(|n| n.is_finite() && (0.0..=f64::from(max)).contains(n))
                .ok_or_else(|| format!("SAVETIME must be from 0 to {max} minutes"))?;
            c.savetime = Some(n.round() as u32);
        }
        k if k.eq_ignore_ascii_case("QNEWTEMPLATE") => c.qnew_template = Some(setting_text(key, v)?),
        k if k.eq_ignore_ascii_case("FONTALT") => c.font_alt = Some(setting_text(key, v)?),
        k if k.eq_ignore_ascii_case("FONTFALLBACK") => c.font_fallback = Some(setting_text(key, v)?),
        k if k.eq_ignore_ascii_case("SAVEFILEPATH") => {
            let p = v.as_str().map(str::trim).filter(|p| p.chars().count() <= MAX_PATH_CHARS && !p.chars().any(char::is_control));
            c.savefilepath = Some(p.ok_or("SAVEFILEPATH must be a folder path")?.to_string());
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// A file path or font names: text of at most [`MAX_PATH_CHARS`] characters, no control characters.
fn setting_text(key: &str, v: &Value) -> Result<String, String> {
    v.as_str()
        .map(str::trim)
        .filter(|p| p.chars().count() <= MAX_PATH_CHARS && !p.chars().any(char::is_control))
        .map(str::to_string)
        .ok_or_else(|| format!("{} must be text of at most {MAX_PATH_CHARS} characters", key.to_ascii_uppercase()))
}

/// The interface preferences as the JSON form reports them.
fn ui_json(app: &CadApp) -> Map<String, Value> {
    let o = &app.options;
    let mut m = Map::new();
    m.insert("theme".into(), json!(app.ui.theme.as_str()));
    m.insert("animations".into(), json!(app.ui.animations.as_str()));
    m.insert("interfaceLanguage".into(), json!(app.ui.interface_language.code()));
    m.insert("saveFormat".into(), json!(app.ui.save_format.as_str()));
    m.insert("systemCursor".into(), json!(app.ui.system_cursor));
    m.insert("historyLines".into(), json!(app.ui.history_lines));
    m.insert("modelBackground".into(), color_json(o.model_background));
    m.insert("paperBackground".into(), color_json(o.paper_background));
    m.insert("selectionColor".into(), color_json(o.selection_color));
    m.insert("snapMarkerSize".into(), json!(o.snap_marker_size));
    m.insert("interfaceScale".into(), json!(o.interface_scale));
    m.insert("saveVersion".into(), json!(app.ui.save_version));
    m.insert("SAVETIME".into(), json!(cadcraft_engine::autosave::savetime()));
    m.insert("SAVEFILEPATH".into(), json!(cadcraft_engine::autosave::savefilepath()));
    m.insert("timeSensitiveRightClick".into(), json!(o.time_sensitive_right_click));
    m.insert("dimStyle".into(), Value::Object(current_dim_style(app)));
    m.insert("dimDefaults".into(), Value::Object(app.session.dim_defaults.clone()));
    m.insert("QNEWTEMPLATE".into(), json!(cadcraft_engine::cmd::file::qnew_template()));
    m.insert("FONTALT".into(), json!(cadcraft_fonts::ttf::font_alt()));
    m.insert("FONTFALLBACK".into(), json!(cadcraft_fonts::ttf::fallback_fonts()));
    m
}

/// The name of the open drawing's current dimension style.
pub fn current_dim_style_name(app: &CadApp) -> Option<String> {
    app.session.doc().ok().map(|d| d.header.str("DIMSTYLE", "Standard"))
}

/// [`DIM_OPTION_VARS`] as the open drawing's current dimension style has them (empty with no
/// drawing).
pub fn current_dim_style(app: &CadApp) -> Map<String, Value> {
    let Ok(d) = app.session.doc() else { return Map::new() };
    let Some(style) = d.dim_style(&d.header.str("DIMSTYLE", "Standard")) else { return Map::new() };
    let fields = serde_json::to_value(style).unwrap_or(Value::Null);
    cadcraft_engine::doc::DIMVARS
        .iter()
        .filter(|(var, _)| DIM_OPTION_VARS.contains(var))
        .filter_map(|(var, field)| fields.get(*field).map(|v| ((*var).to_string(), v.clone())))
        .collect()
}

/// Every preference: the interface's and the engine's profile variables.
pub fn report(app: &CadApp) -> Value {
    let mut m = ui_json(app);
    if let Value::Object(engine) = cadcraft_engine::cmd::options::profile(&app.session) {
        m.extend(engine);
    }
    Value::Object(m)
}

/// The OPTIONS JSON form: validate everything, then change it.
fn run_json(app: &mut CadApp, params: &Value) -> Result<Value, String> {
    let Some(obj) = params.as_object() else { return Err("options: expected an object".into()) };
    let mut ui = UiChanges::default();
    let mut engine = Map::new();
    for (k, v) in obj {
        if !parse_ui(k, v, &mut ui)? {
            engine.insert(k.clone(), v.clone());
        }
    }
    let reset = engine.get("reset").and_then(Value::as_bool) == Some(true);
    // The engine checks its part (and refuses unknown names) before anything here changes.
    app.session.execute("options", &Value::Object(engine)).map_err(|e| e.to_string())?;
    if reset {
        reset_ui(app);
    }
    app.apply_file_settings(&ui);
    if let Some(t) = ui.theme {
        app.ui.theme = t;
    }
    if let Some(a) = ui.animations {
        app.ui.animations = a;
    }
    if let Some(l) = ui.language {
        app.ui.interface_language = l;
    }
    if let Some(f) = ui.save_format {
        app.ui.save_format = f;
    }
    if let Some(b) = ui.system_cursor {
        app.ui.system_cursor = b;
    }
    if let Some(n) = ui.history_lines {
        app.ui.history_lines = n;
    }
    if let Some(c) = ui.model_background {
        app.options.model_background = c;
    }
    if let Some(c) = ui.paper_background {
        app.options.paper_background = c;
    }
    if let Some(c) = ui.selection_color {
        app.options.selection_color = c;
    }
    if let Some(n) = ui.snap_marker_size {
        app.options.snap_marker_size = n;
    }
    if let Some(n) = ui.interface_scale {
        app.options.interface_scale = n;
    }
    if let Some(v) = ui.save_version {
        app.ui.save_version = v.year().to_string();
    }
    if let Some(m) = ui.savetime {
        cadcraft_engine::autosave::set_savetime(m);
    }
    if let Some(p) = &ui.savefilepath {
        cadcraft_engine::autosave::set_savefilepath(p);
    }
    if let Some(b) = ui.time_sensitive_right_click {
        app.options.time_sensitive_right_click = b;
    }
    if let Some(defaults) = ui.dim_defaults {
        app.session.set_dim_defaults(defaults);
    }
    if let Some(style) = ui.dim_style {
        apply_dim_style(app, &style)?;
    }
    Ok(report(app))
}

/// Set `style` on the open drawing's current dimension style, then keep it for new drawings: a
/// style the drawing refuses is an error and the defaults stay as they were.
fn apply_dim_style(app: &mut CadApp, style: &Map<String, Value>) -> Result<(), String> {
    if style.is_empty() {
        return Ok(());
    }
    if let Some(name) = current_dim_style_name(app) {
        let mut params = style.clone();
        params.insert("name".into(), json!(name));
        params.insert("current".into(), json!(true));
        let r = app.session.execute("dimstyle", &Value::Object(params)).map_err(|e| e.to_string())?;
        if let Some(ignored) = r.get("ignored").and_then(Value::as_array).filter(|a| !a.is_empty()) {
            let names: Vec<String> = ignored.iter().map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string)).collect();
            return Err(format!("dimStyle: invalid value for {}", names.join(", ")));
        }
    }
    let mut defaults = app.session.dim_defaults.clone();
    defaults.extend(style.clone());
    app.session.set_dim_defaults(defaults);
    Ok(())
}

/// The interface preferences back to their defaults (the interface language stays).
fn reset_ui(app: &mut CadApp) {
    app.session.set_dim_defaults(Map::new());
    let none = || Some(String::new());
    app.apply_file_settings(&UiChanges { qnew_template: none(), font_alt: none(), font_fallback: none(), ..UiChanges::default() });
    let d = crate::UiState::default();
    app.ui.theme = d.theme;
    app.ui.animations = d.animations;
    app.ui.save_format = d.save_format;
    app.ui.save_version = d.save_version;
    app.ui.system_cursor = d.system_cursor;
    app.ui.history_lines = d.history_lines;
    app.options = UiOptions::default();
}

impl CadApp {
    /// The user profile kept across restarts, as JSON for the host's storage ([`PROFILE_KEY`]):
    /// the engine's profile variables and the interface preferences the interface prefs
    /// ([`CadApp::prefs_json`]) and the desktop workspace file (panels, command line) do not
    /// already keep.
    pub fn profile_json(&self) -> String {
        json!({
            "version": 1,
            "sysvars": cadcraft_engine::cmd::options::profile(&self.session),
            "systemCursor": self.ui.system_cursor,
            "modelBackground": color_json(self.options.model_background),
            "paperBackground": color_json(self.options.paper_background),
            "selectionColor": color_json(self.options.selection_color),
            "snapMarkerSize": self.options.snap_marker_size,
            "timeSensitiveRightClick": self.options.time_sensitive_right_click,
            "interfaceScale": self.options.interface_scale,
            "dimDefaults": self.session.dim_defaults,
            "QNEWTEMPLATE": cadcraft_engine::cmd::file::qnew_template(),
            "FONTALT": cadcraft_fonts::ttf::font_alt(),
            "FONTFALLBACK": cadcraft_fonts::ttf::fallback_fonts(),
        })
        .to_string()
    }

    /// Restore a profile saved by [`Self::profile_json`]. Unknown, malformed or out-of-range
    /// entries are skipped; a profile from a newer version still restores what it can.
    pub fn load_profile(&mut self, json: &str) {
        if json.len() > MAX_PROFILE_BYTES {
            return;
        }
        let Ok(v) = serde_json::from_str::<Value>(json) else { return };
        if let Some(vars) = v.get("sysvars") {
            cadcraft_engine::cmd::options::restore(&mut self.session, vars);
        }
        let mut c = UiChanges::default();
        for key in [
            "systemCursor",
            "modelBackground",
            "paperBackground",
            "selectionColor",
            "snapMarkerSize",
            "timeSensitiveRightClick",
            "interfaceScale",
            "dimDefaults",
            "QNEWTEMPLATE",
            "FONTALT",
            "FONTFALLBACK",
        ] {
            if let Some(value) = v.get(key) {
                let _ = parse_ui(key, value, &mut c);
            }
        }
        // CADCRAFT_SYSTEM_CURSOR=1 turns the system cursor on whatever was saved.
        if c.system_cursor == Some(true) {
            self.ui.system_cursor = true;
        }
        if let Some(bg) = c.model_background {
            self.options.model_background = bg;
        }
        if let Some(bg) = c.paper_background {
            self.options.paper_background = bg;
        }
        if let Some(sel) = c.selection_color {
            self.options.selection_color = sel;
        }
        if let Some(n) = c.snap_marker_size {
            self.options.snap_marker_size = n;
        }
        if let Some(b) = c.time_sensitive_right_click {
            self.options.time_sensitive_right_click = b;
        }
        if let Some(n) = c.interface_scale {
            self.options.interface_scale = n;
        }
        self.apply_file_settings(&c);
        if let Some(defaults) = c.dim_defaults {
            self.session.restore_dim_defaults(defaults);
        }
    }

    /// The Files tab's settings (process-wide): the default template and the font substitutes. A
    /// font change redraws the text.
    fn apply_file_settings(&mut self, c: &UiChanges) {
        if let Some(p) = &c.qnew_template {
            cadcraft_engine::cmd::file::set_qnew_template(p);
        }
        if let Some(name) = &c.font_alt {
            cadcraft_fonts::ttf::set_font_alt(name);
        }
        if let Some(list) = &c.font_fallback {
            cadcraft_fonts::ttf::set_fallback_fonts(list);
        }
        if c.font_alt.is_some() || c.font_fallback.is_some() {
            self.session.touch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours_round_trip_and_bad_ones_are_refused() {
        let c = Color32::from_rgb(0x12, 0xab, 0xff);
        assert_eq!(parse_hex(&hex(c)), Some(c));
        assert_eq!(parse_hex("FFFFFF"), Some(Color32::WHITE));
        for bad in ["#+f+0+0", "", "#fff", "#gggggg", "#12345678", "#ééé", "#1234567"] {
            assert_eq!(parse_hex(bad), None, "{bad}");
        }
    }
}
