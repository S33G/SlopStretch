//! SlopStretch's shared, native-first editor. Decoration stays on the UI thread.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use plugin_core::{RenderParams, SlopControls};
use rvst_core::EditorHost;
use rvst_ui::egui::{
    self, Align2, Color32, FontFamily, FontId, Id, Pos2, Rect, Response, Sense, Stroke, StrokeKind,
    pos2, vec2,
};

mod art;
use art::*;

const WHITE: Color32 = Color32::from_rgb(255, 242, 255);
const MUTED: Color32 = Color32::from_rgb(190, 174, 213);
const PINK: Color32 = Color32::from_rgb(255, 155, 218);
const CYAN: Color32 = Color32::from_rgb(129, 241, 233);
const TAGLINES: [&str; 5] = [
    "Turn 3 seconds of audio into 3 hours of unresolved feelings.",
    "For producers who want ambience, but with consequences.",
    "Stretch responsibly.",
    "Your sample, but spiritually unwell.",
    "From field recording to field-of-view recording.",
];
const STATUSES: [&str; 7] = [
    "Analysing emotional residue…",
    "Adding unnecessary reverb…",
    "Finding the beautiful part…",
    "It’s not a bug, it’s sound design…",
    "Stretching beyond the reach of tempo…",
    "Your neighbours have been notified.",
    "This preset is suitable for ambient music and unresolved arguments.",
];
const PRESETS: [(&str, [f64; 6]); 9] = [
    ("Cathedral But Sad", [32., 2., 0.2, 0.75, 0.62, -12.]),
    (
        "Elevator to the Subconscious",
        [12., 2., 0.1, 0.6, 0.28, -9.],
    ),
    ("Wet Carpet Ambient", [90., 3., 0.35, 0.45, 0.5, -12.]),
    (
        "AI Thinks This Is Cinematic",
        [48., 2., 0.7, 0.95, 0.58, -12.],
    ),
    (
        "Frozen Sainsbury’s Freezer Aisle",
        [320., 3., 0.06, 0.8, 0.32, -12.],
    ),
    (
        "Podcast, But It Has Seen Things",
        [18., 1., 0.4, 0.65, 0.24, -9.],
    ),
    (
        "Kick Drum Having an Existential Crisis",
        [160., 3., 0.6, 0.9, 0.5, -12.],
    ),
    (
        "Ambient For People Who Own Too Many Reverbs",
        [64., 3., 0.22, 1., 0.86, -15.],
    ),
    (
        "Vocal Chop, But It Ascended",
        [24., 2., 0.75, 0.85, 0.42, -12.],
    ),
];

#[derive(Clone, Default)]
struct WorkState {
    busy: bool,
    message: Option<String>,
    source_name: Option<String>,
    error: bool,
}
#[derive(Clone)]
struct UiState {
    installed: bool,
    dark: bool,
    reduced_motion: bool,
    intro_start: Option<f64>,
    intro_done: bool,
    preset: usize,
    preset_applied: bool,
    export_seconds: f32,
    work: Arc<Mutex<WorkState>>,
    source_name: String,
    notice: Option<(String, f64)>,
    region_anchor: f32,
    region_mode: u8,
    freeze_time: f64,
    frozen: bool,
    frozen_playhead: f32,
    cursor: Vec<(Pos2, f64)>,
    cheer_until: f64,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            installed: false,
            dark: false,
            reduced_motion: false,
            intro_start: None,
            intro_done: false,
            preset: 0,
            preset_applied: false,
            export_seconds: 30.,
            work: Arc::new(Mutex::new(WorkState::default())),
            source_name: "A small, perfectly innocent sound".into(),
            notice: None,
            region_anchor: 0.,
            region_mode: 0,
            freeze_time: 0.,
            frozen: false,
            frozen_playhead: 0.,
            cursor: Vec::new(),
            cheer_until: 0.,
        }
    }
}

fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        (
            "Outfit",
            include_bytes!("../assets/fonts/Outfit-Regular.ttf").as_slice(),
        ),
        (
            "Lore",
            include_bytes!("../assets/fonts/DMSerifDisplay.ttf").as_slice(),
        ),
        (
            "Credibility",
            include_bytes!("../assets/fonts/SpaceMono.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
        fonts
            .families
            .insert(FontFamily::Name(name.into()), vec![name.into()]);
    }
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "Outfit".into());
    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .unwrap()
        .insert(0, "Credibility".into());
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::TRANSPARENT;
    style.visuals.window_fill = Color32::from_rgb(37, 22, 58);
    style.visuals.extreme_bg_color = Color32::from_rgb(34, 22, 52);
    style.visuals.selection.bg_fill = Color32::from_rgb(122, 63, 146);
    style.visuals.selection.stroke = Stroke::new(1., PINK);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1., WHITE);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(64, 40, 87);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(64, 40, 87);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1., WHITE);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1., Color32::from_white_alpha(30));
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(96, 56, 119);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(96, 56, 119);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1., WHITE);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1., PINK);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(118, 65, 139);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = egui::CornerRadius::same(24);
    }
    style.spacing.button_padding = vec2(15., 10.);
    style.spacing.item_spacing = vec2(10., 10.);
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(15.));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(15.));
    ctx.set_style_of(egui::Theme::Dark, style);
}

fn value(host: &dyn EditorHost, id: u32) -> f64 {
    host.parameter(id)
        .unwrap_or_else(|| host.descriptor().parameter(id).map_or(0., |s| s.default))
}
fn edit(host: &mut dyn EditorHost, id: u32, val: f64) {
    host.begin_edit(id);
    host.set_parameter(id, val);
    host.end_edit(id);
}
fn preset(host: &mut dyn EditorHost, index: usize) {
    host.begin_edit_group(PRESETS[index].0);
    for (id, val) in [1, 3, 4, 5, 6, 7].into_iter().zip(PRESETS[index].1) {
        edit(host, id, val);
    }
    edit(host, 2, 0.);
    edit(host, 10, 1.);
    edit(host, 11, 0.);
    host.end_edit_group();
}
fn duration(seconds: f64) -> String {
    if seconds < 60. {
        format!("{seconds:.1}s")
    } else if seconds < 3600. {
        format!("{}m {:02}s", seconds as u64 / 60, seconds as u64 % 60)
    } else {
        format!(
            "{}h {:02}m",
            seconds as u64 / 3600,
            seconds as u64 % 3600 / 60
        )
    }
}
fn button(ui: &mut egui::Ui, rect: Rect, label: &str, tip: &str, kind: u8) -> Response {
    let reduced = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(Id::new("slop-reduced-motion")))
        .unwrap_or(false);
    let lift = if reduced {
        0.
    } else {
        ui.ctx().animate_bool_with_time(
            Id::new(("gummy-lift", rect.min.x.to_bits(), rect.min.y.to_bits())),
            ui.rect_contains_pointer(rect),
            0.14,
        ) * 2.
    };
    let rect = rect.translate(vec2(0., -lift));
    let (fill, text, stroke) = match kind {
        1 => (
            PINK,
            Color32::from_rgb(57, 24, 62),
            Color32::from_rgb(255, 194, 233),
        ),
        2 => (
            Color32::from_rgb(61, 96, 103),
            CYAN,
            Color32::from_rgb(115, 177, 177),
        ),
        _ => (
            Color32::from_rgba_unmultiplied(102, 71, 123, 120),
            WHITE,
            Color32::from_white_alpha(32),
        ),
    };
    ui.painter().rect_filled(
        rect.translate(vec2(0., 4.)),
        24.,
        Color32::from_black_alpha(40),
    );
    let text_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(14.), text)
        .size()
        .x;
    let font_size = (14. * (rect.width() - 28.) / text_width.max(1.)).clamp(10.5, 14.);
    let response = ui.put(
        rect,
        egui::Button::new(egui::RichText::new(label).size(font_size).color(text))
            .wrap_mode(egui::TextWrapMode::Extend)
            .fill(fill)
            .stroke(Stroke::new(1., stroke))
            .corner_radius(24),
    );
    if response.hovered() {
        ui.painter().rect_stroke(
            rect.shrink(2.),
            22.,
            Stroke::new(1., Color32::from_white_alpha(70)),
            StrokeKind::Inside,
        );
    }
    response.on_hover_text(tip)
}
fn badge(p: &egui::Painter, rect: Rect, label: &str, color: Color32) {
    p.rect_filled(rect, 12., color.gamma_multiply(0.1));
    p.rect_stroke(
        rect,
        12.,
        Stroke::new(1., color.gamma_multiply(0.3)),
        StrokeKind::Inside,
    );
    text(
        p,
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        10.,
        color,
        true,
    );
}

pub fn editor(ctx: &egui::Context, host: &mut dyn EditorHost) {
    let id = Id::new("slopstretch-ui-v1");
    let mut state = ctx
        .data_mut(|d| d.remove_temp::<UiState>(id))
        .unwrap_or_default();
    if !state.installed {
        install(ctx);
        state.installed = true;
        // Egui applies font definitions at the next pass. Named faces are not
        // available yet, so paint the editor only after that pass has begun.
        ctx.data_mut(|d| d.insert_temp(id, state));
        ctx.request_repaint();
        return;
    }
    ctx.data_mut(|d| d.insert_temp(Id::new("slop-reduced-motion"), state.reduced_motion));
    let time = ctx.input(|i| i.time);
    state.intro_start.get_or_insert(time);
    let controls = host
        .editor_context()
        .and_then(|c| c.downcast::<SlopControls>().ok());
    if let Ok(mut work) = state.work.lock() {
        if let Some(message) = work.message.take() {
            state.notice = Some((message, time + if work.error { 24. } else { 10. }));
        }
        if let Some(name) = work.source_name.take() {
            state.source_name = name;
        }
    }
    let busy = state.work.lock().is_ok_and(|s| s.busy);
    if let Some(controls) = &controls {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped.into_iter().take(1) {
            if !busy {
                load_drop(controls.clone(), &state, file);
            }
        }
    }
    let snapshot = controls.as_ref().map(|c| c.snapshot());
    let playing = snapshot.as_ref().is_some_and(|s| s.playing);
    let capturing = snapshot.as_ref().is_some_and(|s| s.capturing);
    let source_seconds = snapshot
        .as_ref()
        .map_or(0., |s| s.source_frames as f64 / s.sample_rate.max(1) as f64);
    let mut freeze = value(host, 2) >= 0.5;
    if freeze && !state.frozen {
        state.freeze_time = time;
        state.frozen_playhead = snapshot.as_ref().map_or(0., |s| s.playhead);
    }
    state.frozen = freeze;
    let art_time = if state.reduced_motion {
        0.
    } else if freeze {
        state.freeze_time
    } else {
        time
    };
    let slop = value(host, 1);
    if !ctx.text_edit_focused()
        && state.intro_done
        && !ctx.input(|i| i.modifiers.command || i.modifiers.ctrl || i.modifiers.alt)
    {
        if let Some(controls) = &controls {
            if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                controls.set_playing(!playing);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::C)) {
                controls.set_capture(!capturing);
                if !capturing {
                    state.source_name = "Live capture · stereo input".into();
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Home)) {
                controls.rewind();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::L)) && !busy {
                load_dialog(controls.clone(), &state);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::B)) && !busy {
                render_dialog(controls.clone(), &state, render_params(host));
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            freeze = !freeze;
            edit(host, 2, if freeze { 1. } else { 0. });
        }
    }
    let mut root = egui::Ui::new(
        ctx.clone(),
        Id::new("slop-root"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.content_rect()),
    );
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(&mut root, |ui| {
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            let size = vec2(ui.available_width().max(980.), ui.available_height().max(798.));
            let (canvas, _) = ui.allocate_exact_size(size, Sense::hover());
            let o = canvas.min;
            let at = |x: f32, y: f32| o + vec2(x, y);
            let rr = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x,y),vec2(w,h));
            let p = ui.painter().clone();
            background(&p, canvas, state.dark, art_time);
            let right = size.x - 284.;
            let main_w = right - 48.;
            let nebula_x = 350.;
            let nebula_w = right - 374.;
            // Brand: a small, suspiciously friendly piece of studio hardware.
            let breath = if state.reduced_motion { 1. } else { 1. + 0.035 * (time as f32 * 1.4).sin() };
            blob(&p, at(47., 48.), 23. * breath, PINK, 0.);
            text(&p, at(82., 27.), Align2::LEFT_TOP, "SlopStretch", 35., WHITE, false);
            text(&p, at(84., 68.), Align2::LEFT_TOP, "A LITTLE AUDIO. AN UNREASONABLE AMOUNT OF FEELING.", 9., MUTED, true);
            let tag = TAGLINES[(time as usize / 13) % TAGLINES.len()];
            p.text(at(25., 98.), Align2::LEFT_CENTER, tag, FontId::new(15., FontFamily::Name("Lore".into())), Color32::from_rgb(226, 195, 233));
            // Presets have full names in the menu and on hover, even in a narrow host window.
            text(&p, at(right - 338., 24.), Align2::LEFT_TOP, "EMOTIONAL STARTING POINT", 9., MUTED, true);
            let preset_rect = rr(right - 340., 45., 340., 38.);
            let mut chosen = state.preset;
            let mut preset_clicked = false;
            ui.scope_builder(egui::UiBuilder::new().max_rect(preset_rect), |ui| {
                egui::ComboBox::from_id_salt("lore-presets").width(318.).height(340.)
                    .selected_text(if state.preset_applied { PRESETS[state.preset].0 } else { "Choose your emotional baggage…" })
                    .show_ui(ui, |ui| { for (i,(name,_)) in PRESETS.iter().enumerate() { preset_clicked |= ui.selectable_value(&mut chosen, i, *name).clicked(); } })
                    .response.on_hover_text("Nine useful starting points. Preset changes are one undoable host edit.");
            });
            if preset_clicked {
                state.preset = chosen; state.preset_applied = true; preset(host, chosen); state.cheer_until = time + 2.;
            }
            // Theme controls deliberately make only a modest difference.
            if button(ui, rr(right+22., 37., 108., 38.), if state.dark { "Darker-ish" } else { "Dark-ish" }, "Dark mode: the same feelings, slightly less electricity. Changes the gradients.", 0).clicked() { state.dark = !state.dark; }
            if button(ui, rr(right+140., 37., 120., 38.), if state.reduced_motion { "Motion: off" } else { "Motion: on" }, "Reduce motion: stop floating blobs, cursor trails, and decorative animation.", 0).clicked() { state.reduced_motion = !state.reduced_motion; }
            // Waveform and transport.
            let wave_card = rr(24.,126.,main_w,258.);
            glass(&p, wave_card, state.dark);
            text(&p, at(45., 145.), Align2::LEFT_TOP, "01", 11., PINK, true);
            text(&p, at(76., 140.), Align2::LEFT_TOP, "Feed the feeling", 22., WHITE, false);
            badge(&p, rr(right-177.,145.,130.,23.), if capturing { "● CONSUMING" } else if playing { "● SLOP IN PROGRESS" } else { "● READY TO RUIN" }, if capturing { PINK } else { CYAN });
            text(&p, at(46., 177.), Align2::LEFT_TOP, &truncate(&state.source_name, ((main_w - 260.) / 7.) as usize), 13., MUTED, false);
            text(&p, at(right-45., 177.), Align2::RIGHT_TOP, &format!("{}  →  {}", duration(source_seconds), duration(source_seconds * slop * (value(host,9)-value(host,8)).abs())), 11., CYAN, true);
            let wave_rect = rr(46., 204., main_w-44., 91.);
            let wave = controls.as_ref().map_or([0.;256], |c| c.waveform());
            let playhead = if freeze {state.frozen_playhead} else {snapshot.as_ref().map_or(0., |s| s.playhead)};
            waveform(&p, wave_rect, &wave, slop, [value(host,8).min(value(host,9)) as f32, value(host,8).max(value(host,9)) as f32], (playhead, freeze), art_time);
            region(ui, host, wave_rect, &mut state);
            text(&p, at(48.,304.), Align2::LEFT_CENTER, if freeze { "It’s okay to stay here." } else { "DRAG TO SELECT A REGION  ·  DOUBLE-CLICK TO RESET  ·  DROP A WAV" }, if freeze {13.} else {9.}, if freeze {PINK} else {MUTED}, !freeze);
            let gap = 8.; let bw = (main_w-44.-gap*3.) /4.;
            if button(ui, rr(46.,324.,bw,35.), "Load WAV", "Load a WAV sample [L], or drag one onto the plugin. Sources are limited to 30 seconds. Opens a native file dialog.", 0).clicked() && !busy && let Some(c)=&controls {load_dialog(c.clone(), &state);}
            if button(ui, rr(46.+(bw+gap),324.,bw,35.), if capturing {"Enough, thanks"} else {"Consume Audio"}, "Record the incoming stereo bus [C]. Click again to stop. The 30-second buffer stops automatically when full.", if capturing {2} else {0}).clicked() { if let Some(c)=&controls {c.set_capture(!capturing);}
                if !capturing {state.source_name="Live capture · stereo input".into();} state.cheer_until=time+2.; }
            if button(ui, rr(46.+2.*(bw+gap),324.,bw,35.), if playing {"Contain the Slop"} else {"Release the Slop"}, "Play or pause the stretched sample [Space]. Audio is processed by the native plugin, never by the decorative loader.", 1).clicked() { if let Some(c)=&controls {c.set_playing(!playing);} state.cheer_until=time+2.; }
            if button(ui, rr(46.+3.*(bw+gap),324.,bw,35.), "♡  Emotional Support", "Freeze the current spectral moment [F]. Click again to resume moving through the source.", if freeze {2} else {0}).clicked() {edit(host,2,if freeze {0.} else {1.}); state.cheer_until=time+2.;}
            let notice = state.notice.as_ref().filter(|(_,until)| *until>time).map(|(s,_)|s.as_str());
            let status = if busy { "Baking / loading in the background. Please remain emotionally available." } else {notice.unwrap_or(STATUSES[(time as usize/9)%STATUSES.len()])};
            text(&p, at(46.,373.), Align2::LEFT_CENTER, &truncate(status, ((main_w-44.)/6.) as usize), 10., if notice.is_some() {CYAN} else {MUTED}, false);
            ui.interact(rr(46.,362.,main_w-44.,20.), Id::new("slop-status-detail"), Sense::hover()).on_hover_text(status);
            // The large control is the centre of the interaction, not a hidden number field.
            glass(&p, rr(24.,404.,302.,324.), state.dark);
            text(&p, at(45.,424.), Align2::LEFT_TOP, "02", 11., PINK, true);
            text(&p, at(76.,418.), Align2::LEFT_TOP, "Make it a lot", 22., WHITE, false);
            badge(&p, rr(232.,424.,69.,22.), "SLOP", PINK);
            let knob_r = rr(74.,470.,202.,199.);
            let slop_response = knob(ui, host, 1, knob_r, true, true, "Slop stretch factor. Drag up/right for more; Shift for fine control. Arrow keys adjust. Double-click resets to 16×.");
            if slop_response.changed() {state.cheer_until=time+1.5;}
            text(&p, at(58.,678.), Align2::CENTER_CENTER,"Normal",11.,MUTED,false);
            text(&p, at(175.,683.), Align2::CENTER_CENTER,"Concerning",11.,PINK,false);
            text(&p, at(289.,678.), Align2::CENTER_CENTER,"Eternal",11.,MUTED,false);
            text(&p, at(175.,709.), Align2::CENTER_CENTER,"1× — 3,600×  ·  SHIFT = GENTLE",9.,MUTED,true);
            // A nebula driven by spectral data; no decorative representation is claimed to be measurement.
            glass(&p, rr(nebula_x,404.,nebula_w,324.), state.dark);
            text(&p, at(nebula_x+22.,424.), Align2::LEFT_TOP, "03",11.,PINK,true);
            text(&p, at(nebula_x+53.,418.), Align2::LEFT_TOP,"Become atmosphere",22.,WHITE,false);
            let spectrum=controls.as_ref().map_or([0.;64],|c|c.spectrum());
            let spectral_rect=rr(nebula_x+22.,462.,nebula_w-44.,164.);
            nebula(&p,spectral_rect,&spectrum,value(host,4) as f32,value(host,5) as f32,slop as f32,art_time);
            text(&p,at(nebula_x+24.,639.),Align2::LEFT_CENTER,"SPECTRAL WEATHER",9.,MUTED,true);
            text(&p,at(right-46.,639.),Align2::RIGHT_CENTER,if freeze {"STATIONARY FEELINGS"} else {"CHANCE OF FEELINGS: 100%"},9.,PINK,true);
            let render_width=(nebula_w-136.).min(254.);
            let rendering = snapshot.as_ref().is_some_and(|s| s.rendering);
            let render_label = if rendering { format!("Cancel bake · {:.0}%", snapshot.as_ref().map_or(0., |s| s.render_progress) * 100.) } else { "Bake Ambient Lore".into() };
            if button(ui,rr(nebula_x+22.,662.,render_width,38.),&render_label,"Render the selected region and current settings to a 48 kHz float stereo WAV [B]. Choose 1 second to 3 hours. Rendering streams to disk in the background; click again to cancel an active bake.",1).clicked() && let Some(c)=&controls { if rendering { c.cancel_render(); } else if !busy {render_dialog(c.clone(),&state,render_params(host));} }
            ui.scope_builder(egui::UiBuilder::new().max_rect(rr(nebula_x+render_width+34.,662.,80.,38.)), |ui| {
                ui.add(egui::DragValue::new(&mut state.export_seconds).range(1.0..=10800.0).speed(5.0).suffix(" sec")).on_hover_text("Export duration: 1 second to 10,800 seconds (3 hours). At 48 kHz, stereo float WAV uses approximately 23 MB per minute. Double-click to type an exact duration.");
            });
            text(&p,at(nebula_x+24.,714.),Align2::LEFT_CENTER,&format!("48 kHz WAV  ·  {}  ·  ≈{:.1} MB  ·  max 3h", duration(state.export_seconds as f64), state.export_seconds * 0.384),9.,MUTED,true);
            // Right-hand shaping strip.
            glass(&p,rr(right,126.,260.,602.),state.dark);
            text(&p,at(right+22.,148.),Align2::LEFT_TOP,"FINESSE THE MESS",11.,PINK,true);
            text(&p,at(right+22.,178.),Align2::LEFT_TOP,"Vibe Resolution",18.,WHITE,false);
            text(&p,at(right+238.,184.),Align2::RIGHT_CENTER,"FFT",9.,MUTED,true);
            let vibe=value(host,3).round() as usize;
            for (i,name) in ["Crispy","Soup","Fog","Non-Euclidean"].iter().enumerate() {
                if button(ui,rr(right+22.+(i%2) as f32*112.,208.+(i/2) as f32*40.,104.,33.),name,
                    ["1,024-sample window: the sharpest transients.","2,048-sample window: a little soup, as a treat.","4,096-sample window: soft, diffuse detail.","8,192-sample window: time loses its administrative powers."][i],if vibe==i {2} else {0}).clicked() {edit(host,3,i as f64);}
            }
            for (i,(param,name,caption)) in [(4,"Harmonic nonsense","A little extra existential shimmer"),(5,"Spread","Give the feelings some room"),(6,"Unnecessary reverb","Necessary, actually"),(7,"Output","Your neighbours remain a factor"),(10,"Wet / dry","Choose your level of commitment")].iter().enumerate() {
                let y=300.+i as f32*79.;
                if i>0 {p.line_segment([at(right+22.,y-9.),at(right+238.,y-9.)],Stroke::new(1.,Color32::from_white_alpha(13)));}
                knob(ui,host,*param,rr(right+16.,y,64.,64.),false,false,&format!("{name}: drag up/down, Shift-drag for fine control, arrow keys to adjust, double-click to reset."));
                text(&p,at(right+90.,y+11.),Align2::LEFT_TOP,name,15.,WHITE,false);
                let v=value(host,*param);
                text(&p,at(right+90.,y+36.),Align2::LEFT_TOP,&if *param==7 {format!("{v:.1} dB")} else {format!("{:.0}%",v*100.)},11.,CYAN,true);
                ui.interact(rr(right+88.,y+4.,155.,59.),Id::new(("knob-caption",param)),Sense::hover()).on_hover_text(*caption);
            }
            // Practical controls, deliberately quiet.
            if button(ui,rr(24.,750.,92.,31.),"Undo","Undo the last parameter edit [Command/Ctrl+Z]. Presets undo as one group.",0).clicked() {let _=host.undo();}
            if button(ui,rr(124.,750.,92.,31.),"Redo","Redo the last parameter edit [Command/Ctrl+Shift+Z].",0).clicked() {let _=host.redo();}
            if button(ui,rr(226.,750.,105.,31.),"|◀  Rewind","Return to the beginning of the selected region [Home].",0).clicked() && let Some(c)=&controls {c.rewind();}
            if button(ui,rr(341.,750.,110.,31.),"Fresh snack","Load the built-in three-second demo sound. No microphone or external file required.",0).clicked() && let Some(c)=&controls {match c.load_demo() {Ok(())=>state.source_name="A small, perfectly innocent sound".into(),Err(e)=>state.notice=Some((e,time+15.))}}
            let bypass=value(host,11)>=0.5;
            if button(ui,rr(461.,750.,101.,31.),if bypass {"Bypassed"} else {"Bypass"},"Bypass the effect and pass the input through unchanged.",if bypass {2} else {0}).clicked() {edit(host,11,if bypass {0.} else {1.});}
            let peak=snapshot.as_ref().map_or(0.,|s|s.peak);
            let peak_db=20.*peak.max(0.00001).log10();
            text(&p,at(584.,766.),Align2::LEFT_CENTER,&format!("OUT  {} dBFS",if peak_db < -90. {"−∞".into()} else {format!("{peak_db:.1}")}),10.,if peak>0.98 {PINK} else {MUTED},true);
            #[cfg(target_arch="wasm32")]
            text(&p,at(size.x-24.,786.),Align2::RIGHT_CENTER,"SHARED UI PREVIEW · AUDIO RUNS IN THE NATIVE PLUGIN",8.,MUTED,true);
            #[cfg(not(target_arch="wasm32"))]
            text(&p,at(size.x-24.,786.),Align2::RIGHT_CENTER,"SLOPSTRETCH 0.1  /  POWERED BY RVST  /  NO ACTUAL AI",8.,MUTED,true);
            if !state.reduced_motion && state.intro_done {mascot(&p,canvas,&mut state,ctx,time);}
            if !state.intro_done {intro(ui,canvas,&mut state,time);}
        });
    });
    ctx.request_repaint_after(Duration::from_millis(
        if state.reduced_motion && state.intro_done && !playing && !busy {
            500
        } else {
            33
        },
    ));
    ctx.data_mut(|d| d.insert_temp(id, state));
}

fn render_params(host: &dyn EditorHost) -> RenderParams {
    let mut params = RenderParams::default();
    for (i, v) in params.values.iter_mut().enumerate() {
        *v = value(host, i as u32 + 1);
    }
    params
}
fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() > limit {
        format!(
            "{}…",
            text.chars()
                .take(limit.saturating_sub(1))
                .collect::<String>()
        )
    } else {
        text.into()
    }
}

fn region(ui: &mut egui::Ui, host: &mut dyn EditorHost, rect: Rect, state: &mut UiState) {
    let response=ui.interact(rect,Id::new("source-region"),Sense::click_and_drag()).on_hover_text("Drag a region to stretch. Drag either bright edge to trim. Double-click for the whole source.");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "Audio source region")
    });
    if response.double_clicked() {
        host.begin_edit_group("Select entire source");
        edit(host, 8, 0.);
        edit(host, 9, 1.);
        host.end_edit_group();
    }
    let (start_id, end_id) = if value(host, 8) <= value(host, 9) {
        (8, 9)
    } else {
        (9, 8)
    };
    if response.drag_started() {
        host.begin_edit_group("Select source region");
        host.begin_edit(8);
        host.begin_edit(9);
        state.region_anchor = ui
            .input(|i| i.pointer.press_origin())
            .map_or(0., |p| ((p.x - rect.left()) / rect.width()).clamp(0., 1.));
        state.region_mode =
            if (state.region_anchor - value(host, start_id) as f32).abs() * rect.width() < 14. {
                1
            } else if (state.region_anchor - value(host, end_id) as f32).abs() * rect.width() < 14.
            {
                2
            } else {
                0
            };
    }
    if response.dragged()
        && let Some(p) = response.interact_pointer_pos()
    {
        let at = ((p.x - rect.left()) / rect.width()).clamp(0., 1.);
        match state.region_mode {
            1 => host.set_parameter(
                start_id,
                (at as f64).min(value(host, end_id) - 0.005).max(0.),
            ),
            2 => host.set_parameter(
                end_id,
                (at as f64).max(value(host, start_id) + 0.005).min(1.),
            ),
            _ => {
                host.set_parameter(8, state.region_anchor.min(at).min(0.995) as f64);
                host.set_parameter(
                    9,
                    state
                        .region_anchor
                        .max(at)
                        .max(state.region_anchor.min(at) + 0.005)
                        .min(1.) as f64,
                );
            }
        }
    }
    if response.drag_stopped() {
        host.end_edit(8);
        host.end_edit(9);
        host.end_edit_group();
    }
}

fn knob(
    ui: &mut egui::Ui,
    host: &mut dyn EditorHost,
    id: u32,
    rect: Rect,
    logarithmic: bool,
    large: bool,
    tip: &str,
) -> Response {
    let Some(spec) = host.descriptor().parameter(id).copied() else {
        return ui.allocate_rect(rect, Sense::hover());
    };
    let old = value(host, id);
    let norm = |v: f64| {
        if logarithmic {
            (v.max(1.).ln() / 3600f64.ln()).clamp(0., 1.)
        } else {
            spec.normalize(v)
        }
    };
    let denorm = |v: f64| {
        if logarithmic {
            3600f64.powf(v).clamp(1., 3600.)
        } else {
            spec.denormalize(v)
        }
    };
    let mut n = norm(old);
    let mut response = ui.interact(rect, Id::new(("slop-knob", id)), Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::slider(true, old, spec.name));
    if response.clicked() {
        response.request_focus();
    }
    if response.drag_started() {
        host.begin_edit(id);
    }
    if response.dragged() {
        let delta = ui.input(|i| i.pointer.delta());
        let speed = ui.input(|i| if i.modifiers.shift { 0.0005 } else { 0.004 });
        n = (n + (delta.x - delta.y) as f64 * speed).clamp(0., 1.);
    }
    if response.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            )
        });
        let keys = ui.input(|i| {
            i.num_presses(egui::Key::ArrowUp) as i32 + i.num_presses(egui::Key::ArrowRight) as i32
                - i.num_presses(egui::Key::ArrowDown) as i32
                - i.num_presses(egui::Key::ArrowLeft) as i32
        });
        if keys != 0 {
            ui.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            n = (n + keys as f64 * ui.input(|i| if i.modifiers.shift { 0.001 } else { 0.01 }))
                .clamp(0., 1.);
        }
    }
    let mut v = denorm(n);
    if response.double_clicked() {
        v = spec.default;
        n = norm(v);
    }
    if (v - old).abs() > 1e-9 {
        if !response.dragged() {
            host.begin_edit(id);
        }
        host.set_parameter(id, v);
        response.mark_changed();
    }
    if response.drag_stopped() || ((v - old).abs() > 1e-9 && !response.dragged()) {
        host.end_edit(id);
    }
    draw_knob(
        ui.painter(),
        rect,
        n as f32,
        large,
        response.hovered() || response.has_focus(),
        old,
    );
    response.on_hover_text(tip)
}

fn begin_job(state: &UiState) -> bool {
    if let Ok(mut w) = state.work.lock() {
        if w.busy {
            return false;
        }
        w.busy = true;
        w.error = false;
        true
    } else {
        false
    }
}
fn finish_job(work: &Arc<Mutex<WorkState>>, result: Result<String, String>, source: bool) {
    if let Ok(mut w) = work.lock() {
        w.busy = false;
        match result {
            Ok(msg) => {
                if source {
                    w.source_name = Some(msg.clone());
                    w.message = Some(format!("Consumed {msg}. It had a good run."));
                } else {
                    w.message = Some(msg);
                }
                w.error = false;
            }
            Err(e) => {
                w.message = Some(e);
                w.error = true;
            }
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn load_dialog(controls: Arc<SlopControls>, state: &UiState) {
    if !begin_job(state) {
        return;
    }
    let work = state.work.clone();
    std::thread::spawn(move || {
        let file = rfd::FileDialog::new()
            .set_title("Feed SlopStretch a WAV · maximum 30 seconds")
            .add_filter("WAV audio", &["wav", "wave"])
            .pick_file();
        if let Some(path) = file {
            let result = read_wav(&path)
                .and_then(|bytes| controls.load_wav_bytes(&bytes))
                .map(|()| {
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                });
            finish_job(&work, result, true);
        } else if let Ok(mut w) = work.lock() {
            w.busy = false;
        }
    });
}
#[cfg(target_arch = "wasm32")]
fn load_dialog(_controls: Arc<SlopControls>, state: &UiState) {
    finish_job(&state.work,Err("Browser preview: drop a WAV to inspect it. Use the native plugin for audio and file dialogs.".into()),false);
}
#[cfg(not(target_arch = "wasm32"))]
fn read_wav(path: &std::path::Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > 256 * 1024 * 1024 {
        return Err("That WAV is over 256 MB. Please trim it to 30 seconds first.".into());
    }
    std::fs::read(path).map_err(|e| e.to_string())
}
fn load_drop(controls: Arc<SlopControls>, state: &UiState, file: egui::DroppedFileHandle) {
    if !begin_job(state) {
        return;
    }
    let work = state.work.clone();
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        let path = file.path();
        let result = read_wav(path)
            .and_then(|bytes| controls.load_wav_bytes(&bytes))
            .map(|()| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        finish_job(&work, result, true);
    });
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let result = file
            .bytes_async()
            .await
            .and_then(|bytes| controls.load_wav_bytes(&bytes))
            .map(|()| {
                file.path()
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        finish_job(&work, result, true);
    });
}
#[cfg(not(target_arch = "wasm32"))]
fn render_dialog(controls: Arc<SlopControls>, state: &UiState, params: RenderParams) {
    if !begin_job(state) {
        return;
    }
    let work = state.work.clone();
    let seconds = state.export_seconds.clamp(1., 10800.);
    std::thread::spawn(move || {
        let path = rfd::FileDialog::new()
            .set_title("Bake Ambient Lore · 48 kHz stereo WAV")
            .set_file_name("slopstretch-ambient-lore.wav")
            .add_filter("WAV audio", &["wav"])
            .save_file();
        if let Some(path) = path {
            let result = controls
                .render_wav_to_path(&path, seconds, params)
                .map(|()| format!("Baked {:.0} seconds of lore → {}", seconds, path.display()));
            finish_job(&work, result, false);
        } else if let Ok(mut w) = work.lock() {
            w.busy = false;
        }
    });
}
#[cfg(target_arch = "wasm32")]
fn render_dialog(_controls: Arc<SlopControls>, state: &UiState, _params: RenderParams) {
    finish_job(
        &state.work,
        Err(
            "WAV export lives in the native plugin. This browser is a shared-editor preview."
                .into(),
        ),
        false,
    );
}
