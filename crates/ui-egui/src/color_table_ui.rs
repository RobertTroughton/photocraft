//! Image › Mode › Color Table, and the Custom palette of Indexed Color: the table's entries as a
//! 16 × 16 grid (click an entry, or drag across several), a Table preset menu and Load… / Save…
//! (`.act`, `.aco`, `.ase`). The entries live in the dialog fields (`colors` as `#rrggbb`), so
//! `ui.dialog.set` drives it. A single selected entry is edited through its hex field; with
//! several selected, the typed colour becomes the end of a gradient from the first entry. An
//! empty cell right after the last entry adds one; Delete removes the selection.
//!
//! Opened from Color Table… it runs `image.mode.colorTable` on OK. Opened from Indexed Color's
//! Custom palette (`__parent` is that dialog) OK hands the entries to it as its `table`, and
//! Cancel restores the palette it had before.

use egui::{Rect, Sense, Stroke, StrokeKind, vec2};
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::state::DialogKind;
use crate::theme::Tokens;
use crate::widgets;

const COMMAND: &str = "image.mode.colorTable";
const COLUMNS: usize = 16;
const MAX_ENTRIES: usize = COLUMNS * COLUMNS;
const CELL: f32 = 20.0;
const GAP: f32 = 2.0;
const PRESETS: [(&str, &str); 6] = [
    ("custom", "Custom"),
    ("blackBody", "Black Body"),
    ("grayscale", "Grayscale"),
    ("spectrum", "Spectrum"),
    ("systemMac", "System (Mac OS)"),
    ("systemWindows", "System (Windows)"),
];

pub fn owns(f: &Map<String, Value>) -> bool {
    f.contains_key("__colorTable")
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

fn parse_list(v: Option<&Value>) -> Vec<[u8; 3]> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().and_then(parse_hex)).collect()).unwrap_or_default()
}

fn colors(f: &Map<String, Value>) -> Vec<[u8; 3]> {
    parse_list(f.get("colors"))
}

fn hexes(c: &[[u8; 3]]) -> Value {
    json!(c.iter().map(|c| hex(*c)).collect::<Vec<_>>())
}

fn selection(f: &Map<String, Value>) -> Option<(usize, usize)> {
    let a = f.get("__sel")?.as_array()?;
    Some((usize::try_from(a.first()?.as_u64()?).ok()?, usize::try_from(a.get(1)?.as_u64()?).ok()?))
}

fn fields(colors: &[[u8; 3]], transparent: Option<u8>) -> Map<String, Value> {
    let mut f = Map::new();
    f.insert("__colorTable".into(), json!(true));
    f.insert("__label".into(), json!("Color Table"));
    f.insert("colors".into(), hexes(colors));
    f.insert("transparent".into(), json!(transparent));
    f.insert("table".into(), json!("custom"));
    f
}

/// Image › Mode › Color Table… for the active Indexed Color document.
pub fn open(app: &mut PhotocraftApp) -> Result<u64, String> {
    let t = app.session.active().and_then(|d| d.doc.color_table.clone()).ok_or("the document has no color table")?;
    let mut f = fields(&t.colors, t.transparent);
    f.insert("__target".into(), json!("table"));
    Ok(app.ui.open_dialog(DialogKind::Command, f))
}

/// The Custom palette's editor, over the Indexed Color dialog `parent` (whose fields are `parent_fields`).
fn open_for_indexed(app: &mut PhotocraftApp, parent: u64, parent_fields: &Map<String, Value>) -> u64 {
    let have = parse_list(parent_fields.get("table"));
    let n = parent_fields.get("colors").and_then(Value::as_f64).map_or(256, |v| v as usize);
    let start = if have.is_empty() { photocraft_engine::mode_cmds::initial_palette(&app.session, n) } else { have };
    let mut f = fields(&start, None);
    f.insert("__target".into(), json!("indexed"));
    f.insert("__parent".into(), json!(parent));
    f.insert("__before".into(), parent_fields.get("__palette_before").cloned().unwrap_or(Value::Null));
    app.ui.open_dialog(DialogKind::Command, f)
}

/// An Indexed Color dialog asked for the Custom palette editor (`__open_table`).
pub fn take_request(app: &mut PhotocraftApp, parent: u64, f: &mut Map<String, Value>) {
    if f.remove("__open_table").is_some() {
        open_for_indexed(app, parent, f);
    }
}

fn status(app: &mut PhotocraftApp, r: Result<Value, String>) {
    if let Err(e) = r
        && e != "cancelled"
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

fn load_into(app: &mut PhotocraftApp, id: u64, name: &str, bytes: &[u8]) -> Result<Value, String> {
    let t = photocraft_engine::palette_file::parse(&app.session, COMMAND, name, bytes).map_err(|e| e.to_string())?;
    let d = app.ui.dialog_mut(id).ok_or("the Color Table dialog was closed")?;
    d.fields.insert("colors".into(), hexes(&t.colors));
    d.fields.insert("transparent".into(), json!(t.transparent));
    d.fields.insert("table".into(), json!("custom"));
    d.fields.remove("__sel");
    Ok(json!({"colors": t.colors.len()}))
}

fn save_from(app: &mut PhotocraftApp, id: u64, path: &str) -> Result<Value, String> {
    let d = app.ui.dialogs.iter().find(|d| d.id == id).ok_or("the Color Table dialog was closed")?;
    let transparent = d.fields.get("transparent").and_then(Value::as_u64).and_then(|i| u8::try_from(i).ok());
    let bytes = photocraft_engine::palette_file::encode(COMMAND, path, &colors(&d.fields), transparent).map_err(|e| e.to_string())?;
    let write = app.services.write.as_mut().ok_or("no writer configured")?;
    write(path, &bytes)?;
    app.ui.status_error = false;
    app.ui.status = format!("Saved color table to {}", crate::file_open::display_name(path));
    Ok(json!({"path": path}))
}

/// Cell under the pointer in the grid at `origin`.
fn cell_at(origin: egui::Pos2, p: egui::Pos2) -> Option<usize> {
    let d = p - origin - vec2(GAP, GAP);
    if d.x < 0.0 || d.y < 0.0 {
        return None;
    }
    let (col, row) = ((d.x / (CELL + GAP)) as usize, (d.y / (CELL + GAP)) as usize);
    (col < COLUMNS && row < COLUMNS).then_some(row * COLUMNS + col)
}

fn cell_rect(origin: egui::Pos2, i: usize) -> Rect {
    let (col, row) = ((i % COLUMNS) as f32, (i / COLUMNS) as f32);
    Rect::from_min_size(origin + vec2(GAP + col * (CELL + GAP), GAP + row * (CELL + GAP)), vec2(CELL, CELL))
}

pub fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, id: u64, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let target_table = f.get("__target").and_then(Value::as_str) == Some("table");
    let mut cols = colors(f);
    let mut sel = selection(f).filter(|&(a, b)| a <= b && b < cols.len());
    let mut transparent = f.get("transparent").and_then(Value::as_u64).and_then(|i| u8::try_from(i).ok());
    let mut edited = false;
    let mut picked: Option<&str> = None;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(crate::filter_dialog::label("table")).color(t.text_dim));
        let mut cur = f.get("table").and_then(Value::as_str).unwrap_or("custom").to_string();
        let opts: Vec<(String, &str)> = PRESETS.iter().map(|(k, l)| (k.to_string(), *l)).collect();
        if widgets::dropdown(ui, "ct-table", &mut cur, &opts, 190.0) && cur != "custom" {
            if let Some(p) = photocraft_engine::mode_cmds::preset_table(&cur) {
                cols = p;
                sel = None;
                transparent = transparent.filter(|&i| usize::from(i) < cols.len());
            }
            f.insert("table".into(), json!(cur));
        }
    });
    ui.add_space(6.0);

    ui.horizontal_top(|ui| {
        let side = COLUMNS as f32 * (CELL + GAP) + GAP;
        let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
        let anchor_id = ui.id().with("ct-anchor");
        if resp.is_pointer_button_down_on() {
            let hit = resp.interact_pointer_pos().and_then(|p| cell_at(rect.min, p));
            let anchor: Option<usize> = ui.ctx().data(|d| d.get_temp(anchor_id));
            if let Some(i) = hit {
                match anchor {
                    None if i == cols.len() && cols.len() < MAX_ENTRIES => {
                        cols.push(cols.last().copied().unwrap_or([0, 0, 0]));
                        sel = Some((i, i));
                        edited = true;
                        ui.ctx().data_mut(|d| d.insert_temp(anchor_id, i));
                    }
                    None if i < cols.len() => {
                        let from = if ui.input(|inp| inp.modifiers.shift) { sel.map_or(i, |s| s.0) } else { i };
                        sel = Some((from.min(i), from.max(i)));
                        ui.ctx().data_mut(|d| d.insert_temp(anchor_id, from));
                    }
                    Some(a) if i < cols.len() => sel = Some((a.min(i), a.max(i))),
                    _ => {}
                }
            }
        } else {
            ui.ctx().data_mut(|d| d.remove_temp::<usize>(anchor_id));
        }
        if let Some((a, b)) = sel
            && ui.input(|inp| inp.key_pressed(egui::Key::Delete))
            && !ui.ctx().memory(|m| m.focused().is_some())
            && cols.len() > b - a + 1
        {
            cols.drain(a..=b);
            transparent = transparent.and_then(|i| match usize::from(i) {
                i if i < a => u8::try_from(i).ok(),
                i if i > b => u8::try_from(i - (b - a + 1)).ok(),
                _ => None,
            });
            sel = None;
            edited = true;
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, t.field);
        for i in 0..MAX_ENTRIES {
            let r = cell_rect(rect.min, i);
            match cols.get(i) {
                Some(c) => {
                    painter.rect_filled(r, 0.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                    if transparent.is_some_and(|ti| usize::from(ti) == i) {
                        widgets::checker(&painter, Rect::from_min_size(r.min, vec2(CELL / 2.0, CELL / 2.0)), CELL / 4.0);
                    }
                }
                None => widgets::checker(&painter, r, CELL / 4.0),
            }
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, t.card_border), StrokeKind::Inside);
            if sel.is_some_and(|(a, b)| (a..=b).contains(&i)) {
                painter.rect_stroke(r, 0.0, Stroke::new(2.0, t.accent), StrokeKind::Inside);
            }
        }

        ui.vertical(|ui| {
            if widgets::secondary_button(ui, tl!("Load…"), 84.0).clicked() {
                picked = Some("load");
            }
            if widgets::secondary_button(ui, tl!("Save…"), 84.0).clicked() {
                picked = Some("save");
            }
        });
    });

    if let Some((a, b)) = sel {
        ui.add_space(6.0);
        let key = format!("{a}-{b}-{}", cols.get(a).copied().map(hex).unwrap_or_default());
        if f.get("__hexKey").and_then(Value::as_str) != Some(key.as_str()) {
            f.insert("__hex".into(), json!(cols.get(a).copied().map(hex).unwrap_or_default()));
            f.insert("__hexKey".into(), json!(key));
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(crate::filter_dialog::label("color")).color(t.text_dim));
            let mut text = f.get("__hex").and_then(Value::as_str).unwrap_or_default().to_string();
            let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(80.0));
            f.insert("__hex".into(), json!(text));
            if let Some(c) = parse_hex(&text) {
                if a == b && r.changed() {
                    if let Some(e) = cols.get_mut(a) {
                        *e = c;
                    }
                    edited = true;
                } else if a != b && r.lost_focus() {
                    let from = cols.get(a).copied().unwrap_or(c);
                    for (i, e) in cols.iter_mut().enumerate().take(b + 1).skip(a) {
                        let k = (i - a) as f32 / (b - a) as f32;
                        *e = std::array::from_fn(|ch| (f32::from(from[ch]) + (f32::from(c[ch]) - f32::from(from[ch])) * k).round() as u8);
                    }
                    edited = true;
                }
            }
            if target_table {
                let mut on = transparent.is_some_and(|ti| usize::from(ti) == a);
                if widgets::checkbox(ui, &mut on, tl!("Transparent")).changed() {
                    transparent = if on { u8::try_from(a).ok() } else { None };
                }
            }
        });
    }

    if edited {
        f.insert("table".into(), json!("custom"));
    }
    f.insert("colors".into(), hexes(&cols));
    f.insert("transparent".into(), json!(transparent));
    match sel {
        Some((a, b)) => f.insert("__sel".into(), json!([a, b])),
        None => f.remove("__sel"),
    };
    match picked {
        Some("load") => {
            let r = app.pick_file_bytes_of(Some(("Color Table", &["act"])), move |app, name, bytes| load_into(app, id, &name, &bytes));
            status(app, r);
        }
        Some(_) => {
            let r = app.pick_save("Color Table.act", move |app, path| save_from(app, id, &path));
            status(app, r);
        }
        None => {}
    }
}

/// OK: apply the entries to the document, or hand them to the Indexed Color dialog.
pub fn confirm(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    let cols = colors(f);
    if cols.is_empty() {
        return Err("a color table needs at least one entry".into());
    }
    if f.get("__target").and_then(Value::as_str) == Some("indexed") {
        let parent = f.get("__parent").and_then(Value::as_u64).unwrap_or_default();
        let d = app.ui.dialog_mut(parent).ok_or("the Indexed Color dialog was closed")?;
        d.fields.insert("table".into(), hexes(&cols));
        d.fields.insert("palette".into(), json!("custom"));
        d.fields.remove("__palette_before");
        return Ok(Value::Null);
    }
    app.run(COMMAND, json!({"colors": hexes(&cols), "transparent": f.get("transparent").cloned().unwrap_or(Value::Null)}))
}

/// Cancel from the Custom palette editor: the Indexed Color dialog goes back to the palette it had
/// unless it already holds an edited custom one.
pub fn cancelled(app: &mut PhotocraftApp, f: &Map<String, Value>) {
    if f.get("__target").and_then(Value::as_str) != Some("indexed") {
        return;
    }
    let Some(parent) = f.get("__parent").and_then(Value::as_u64).and_then(|p| app.ui.dialog_mut(p)) else { return };
    if !parent.fields.contains_key("table") {
        let before = f.get("__before").cloned().filter(Value::is_string).unwrap_or_else(|| json!("selective"));
        parent.fields.insert("palette".into(), before);
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use photocraft_engine::Session;

    use super::*;

    fn app() -> Harness<'static, PhotocraftApp> {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 16, "height": 16})).unwrap();
        let app = PhotocraftApp::new(s, crate::Services::default());
        let h = Harness::builder().with_size(egui::vec2(1000.0, 800.0)).build_ui_state(|ui, app| crate::dialogs::show(app, ui.ctx()), app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h
    }

    fn top(h: &Harness<'static, PhotocraftApp>) -> u64 {
        h.state().ui.dialogs.last().map(|d| d.id).unwrap()
    }

    #[test]
    fn custom_palette_editor_feeds_the_indexed_color_dialog() {
        let mut h = app();
        let parent = crate::dialogs::open_command_dialog(h.state_mut(), "image.mode.indexedColor", "Indexed Color…");
        {
            let d = h.state_mut().ui.dialog_mut(parent).unwrap();
            d.fields.insert("palette".into(), json!("custom"));
            d.fields.insert("__palette_before".into(), json!("perceptual"));
            d.fields.insert("__open_table".into(), json!(true));
        }
        h.run_steps(3);
        let editor = top(&h);
        assert_ne!(editor, parent);
        assert!(owns(&h.state().ui.dialogs.last().unwrap().fields));

        // Edit the first entry and add one after the last.
        let n = {
            let d = h.state_mut().ui.dialog_mut(editor).unwrap();
            let mut c = colors(&d.fields);
            c.truncate(3);
            c[0] = [1, 2, 3];
            d.fields.insert("colors".into(), hexes(&c));
            c.len()
        };
        crate::dialogs::confirm(h.state_mut(), editor).unwrap();
        let table = h.state().ui.dialogs.iter().find(|d| d.id == parent).unwrap().fields["table"].clone();
        assert_eq!(table.as_array().map(Vec::len), Some(n));
        assert_eq!(table[0], "#010203");

        crate::dialogs::confirm(h.state_mut(), parent).unwrap();
        let doc = &h.state().session.active().unwrap().doc;
        assert_eq!(doc.color_table.as_ref().unwrap().colors.len(), n);
        assert_eq!(doc.color_table.as_ref().unwrap().colors[0], [1, 2, 3]);
    }

    #[test]
    fn cancelling_the_editor_restores_the_previous_palette() {
        let mut h = app();
        let parent = crate::dialogs::open_command_dialog(h.state_mut(), "image.mode.indexedColor", "Indexed Color…");
        {
            let d = h.state_mut().ui.dialog_mut(parent).unwrap();
            d.fields.insert("palette".into(), json!("custom"));
            d.fields.insert("__palette_before".into(), json!("adaptive"));
            d.fields.insert("__open_table".into(), json!(true));
        }
        h.run_steps(3);
        let editor = top(&h);
        let closed = h.state_mut().ui.close_dialog(editor).unwrap();
        cancelled(h.state_mut(), &closed.fields);
        assert_eq!(h.state().ui.dialogs.iter().find(|d| d.id == parent).unwrap().fields["palette"], "adaptive");
    }

    #[test]
    fn color_table_dialog_applies_edits_to_the_document() {
        let mut h = app();
        h.state_mut().run("image.mode.indexedColor", json!({"palette": "uniform", "colors": 8, "dither": "none", "forced": "none"})).unwrap();
        let id = crate::dialogs::open_command_dialog(h.state_mut(), "image.mode.colorTable", "Color Table…");
        h.run_steps(2);
        assert!(owns(&h.state().ui.dialogs.iter().find(|d| d.id == id).unwrap().fields));
        let d = h.state_mut().ui.dialog_mut(id).unwrap();
        let mut c = colors(&d.fields);
        c[0] = [9, 8, 7];
        d.fields.insert("colors".into(), hexes(&c));
        d.fields.insert("transparent".into(), json!(1));
        crate::dialogs::confirm(h.state_mut(), id).unwrap();
        let t = h.state().session.active().unwrap().doc.color_table.clone().unwrap();
        assert_eq!((t.colors[0], t.transparent), ([9, 8, 7], Some(1)));
    }
}
