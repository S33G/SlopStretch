//! Interaction regressions exercise the actual shared editor with pointer/key input.
use plugin_core::{DemoPlugin, FREEZE, REGION_END, REGION_START, SLOP};
use rvst_core::{EditorContext, EditorHost, Error, Plugin, PluginDescriptor};
use rvst_ui::egui::{self, Pos2, Rect, Vec2, pos2};

struct Host {
    plugin: DemoPlugin,
    gestures: Vec<(bool, u32)>,
    groups: Vec<Option<String>>,
    undo_calls: usize,
    redo_calls: usize,
}
impl Host {
    fn new() -> Self {
        Self {
            plugin: DemoPlugin::new(),
            gestures: Vec::new(),
            groups: Vec::new(),
            undo_calls: 0,
            redo_calls: 0,
        }
    }
    fn assert_balanced(&self) {
        let mut active = [0_i32; plugin_core::PARAM_COUNT];
        for &(begin, id) in &self.gestures {
            active[id as usize - 1] += if begin { 1 } else { -1 };
            assert!(active[id as usize - 1] >= 0, "end without begin: {id}");
        }
        assert_eq!(active, [0; plugin_core::PARAM_COUNT]);
        let mut depth = 0;
        for group in &self.groups {
            depth += if group.is_some() { 1 } else { -1 };
            assert!(depth >= 0, "group ended without beginning");
        }
        assert_eq!(depth, 0);
    }
}
impl EditorHost for Host {
    fn descriptor(&self) -> &'static PluginDescriptor {
        DemoPlugin::descriptor()
    }
    fn parameter(&self, id: u32) -> Option<f64> {
        self.plugin.parameter(id)
    }
    fn editor_context(&self) -> Option<EditorContext> {
        self.plugin.editor_context()
    }
    fn begin_edit(&mut self, id: u32) {
        self.gestures.push((true, id));
    }
    fn set_parameter(&mut self, id: u32, value: f64) {
        self.plugin.set_parameter(id, value).unwrap();
    }
    fn end_edit(&mut self, id: u32) {
        self.gestures.push((false, id));
    }
    fn begin_edit_group(&mut self, label: &str) {
        self.groups.push(Some(label.into()));
    }
    fn end_edit_group(&mut self) {
        self.groups.push(None);
    }
    fn save_state(&mut self) -> Result<Vec<u8>, Error> {
        self.plugin.save_state()
    }
    fn load_state(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.plugin.load_state(bytes)
    }
    fn undo(&mut self) -> Result<bool, Error> {
        self.undo_calls += 1;
        Ok(true)
    }
    fn redo(&mut self) -> Result<bool, Error> {
        self.redo_calls += 1;
        Ok(true)
    }
}

struct Harness {
    ctx: egui::Context,
    host: Host,
    time: f64,
    viewport: Option<Vec2>,
    shapes: Vec<egui::epaint::ClippedShape>,
}
impl Harness {
    fn new() -> Self {
        Self::with_viewport(Some(Vec2::new(1180., 820.)))
    }
    fn with_viewport(viewport: Option<Vec2>) -> Self {
        let mut harness = Self {
            ctx: egui::Context::default(),
            host: Host::new(),
            time: 0.,
            viewport,
            shapes: Vec::new(),
        };
        harness.frame(Vec::new());
        // Egui applies new font definitions at the next frame boundary.
        harness.frame(Vec::new());
        harness
    }
    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 1. / 60.;
        let input = egui::RawInput {
            screen_rect: self
                .viewport
                .map(|size| Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut output = self.ctx.run_ui(input, |ui| {
            rvst_ui::editor_frame(ui.ctx(), &mut self.host, plugin_ui::editor);
        });
        output.textures_delta.clear();
        // The native runner tessellates these shapes after the editor returns.
        // Finite widget values alone do not protect custom paths from NaNs.
        let primitives = self
            .ctx
            .tessellate(output.shapes.clone(), output.pixels_per_point);
        for primitive in primitives {
            if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                assert!(
                    mesh.vertices
                        .iter()
                        .all(|v| v.pos.is_finite() && v.uv.is_finite())
                );
            }
        }
        self.shapes = output.shapes;
    }
    fn pointer(&mut self, at: Pos2, pressed: bool) {
        self.frame(vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    fn click(&mut self, at: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(at)]);
        self.pointer(at, true);
        self.pointer(at, false);
    }
    fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        let event = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        self.frame(vec![event(true)]);
        self.frame(vec![event(false)]);
    }
    fn text_pos(&self, needle: &str) -> Pos2 {
        fn find(shape: &egui::Shape, needle: &str) -> Option<Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text().contains(needle) => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, needle)),
                _ => None,
            }
        }
        self.shapes
            .iter()
            .find_map(|shape| find(&shape.shape, needle))
            .unwrap_or_else(|| panic!("visible text missing: {needle}"))
    }
    fn skip_intro(&mut self) {
        self.click(self.text_pos("Skip the bit"));
        self.frame(Vec::new());
    }
}

#[test]
fn loader_and_normal_editor_tessellate_at_native_and_minimum_canvas_sizes() {
    for viewport in [
        Some(Vec2::new(1180., 820.)),
        Some(Vec2::new(980., 798.)),
        None, // Egui/native initial RawInput can arrive before a screen rectangle.
    ] {
        let mut h = Harness::with_viewport(viewport);
        for elapsed in [0.5, 1.6, 2.5, 3.5, 4.2] {
            h.time = elapsed;
            h.frame(Vec::new());
        }
        h.host.set_parameter(SLOP, 3600.);
        h.host.set_parameter(plugin_core::HARMONICS, 1.);
        h.host.set_parameter(plugin_core::SPREAD, 1.);
        h.frame(vec![egui::Event::PointerMoved(pos2(400., 500.))]);
        h.host.set_parameter(FREEZE, 1.);
        h.frame(Vec::new());
    }
}

#[test]
fn loader_skip_and_transport_shortcuts_operate_the_real_controls() {
    let mut h = Harness::new();
    h.key(egui::Key::Space, egui::Modifiers::NONE);
    assert!(
        !h.host.plugin.controls().snapshot().playing,
        "intro gates shortcuts"
    );
    h.skip_intro();
    h.key(egui::Key::Space, egui::Modifiers::NONE);
    assert!(h.host.plugin.controls().snapshot().playing);
    h.key(egui::Key::C, egui::Modifiers::NONE);
    let snapshot = h.host.plugin.controls().snapshot();
    assert!(snapshot.capturing);
    assert!(!snapshot.playing);
    h.key(egui::Key::Space, egui::Modifiers::NONE);
    let snapshot = h.host.plugin.controls().snapshot();
    assert!(snapshot.playing);
    assert!(!snapshot.capturing);
    h.key(egui::Key::F, egui::Modifiers::NONE);
    assert_eq!(h.host.parameter(FREEZE), Some(1.));
    h.key(egui::Key::F, egui::Modifiers::NONE);
    assert_eq!(h.host.parameter(FREEZE), Some(0.));
    h.host.assert_balanced();
}

#[test]
fn first_factory_preset_applies_from_initial_selection_as_one_group() {
    let mut h = Harness::new();
    h.skip_intro();
    h.click(h.text_pos("Choose your emotional baggage"));
    h.frame(Vec::new());
    h.click(h.text_pos("Cathedral But Sad"));
    assert_eq!(h.host.parameter(SLOP), Some(32.));
    assert_eq!(h.host.groups, vec![Some("Cathedral But Sad".into()), None]);
    assert_eq!(
        h.host.gestures.len(),
        18,
        "nine parameters each begin and end"
    );
    h.host.assert_balanced();
}

#[test]
fn knob_keyboard_bounds_and_drag_close_the_host_gesture() {
    let mut h = Harness::new();
    h.skip_intro();
    h.click(pos2(175., 565.));
    let original = h.host.parameter(SLOP).unwrap();
    h.key(egui::Key::ArrowUp, egui::Modifiers::NONE);
    assert!(h.host.parameter(SLOP).unwrap() > original);
    h.host.set_parameter(SLOP, 3599.);
    h.key(egui::Key::ArrowUp, egui::Modifiers::NONE);
    assert_eq!(h.host.parameter(SLOP), Some(3600.));
    h.host.set_parameter(SLOP, 1.);
    h.key(egui::Key::ArrowDown, egui::Modifiers::NONE);
    assert_eq!(h.host.parameter(SLOP), Some(1.));
    h.pointer(pos2(175., 565.), true);
    h.frame(vec![egui::Event::PointerMoved(pos2(175., 515.))]);
    h.pointer(pos2(175., 515.), false);
    assert!(h.host.parameter(SLOP).unwrap() > 1.);
    h.host.assert_balanced();
}

#[test]
fn waveform_drag_makes_an_ordered_region_and_balanced_pair() {
    let mut h = Harness::new();
    h.skip_intro();
    h.pointer(pos2(500., 245.), true);
    h.frame(vec![egui::Event::PointerMoved(pos2(260., 245.))]);
    h.pointer(pos2(260., 245.), false);
    let start = h.host.parameter(REGION_START).unwrap();
    let end = h.host.parameter(REGION_END).unwrap();
    assert!((start - (260. - 46.) / 804.).abs() < 0.01, "start={start}");
    assert!((end - (500. - 46.) / 804.).abs() < 0.01, "end={end}");
    assert_eq!(
        h.host.groups,
        vec![Some("Select source region".into()), None]
    );
    h.host.assert_balanced();
}

#[test]
fn reversed_host_region_remains_editable_at_its_visible_left_edge() {
    let mut h = Harness::new();
    h.skip_intro();
    h.host.set_parameter(REGION_START, 0.8);
    h.host.set_parameter(REGION_END, 0.2);
    h.frame(Vec::new());
    h.pointer(pos2(46. + 804. * 0.2, 245.), true);
    h.frame(vec![egui::Event::PointerMoved(pos2(
        46. + 804. * 0.3,
        245.,
    ))]);
    h.pointer(pos2(46. + 804. * 0.3, 245.), false);
    assert_eq!(h.host.parameter(REGION_START), Some(0.8));
    assert!((h.host.parameter(REGION_END).unwrap() - 0.3).abs() < 0.001);
    h.host.assert_balanced();
}

#[test]
fn host_history_shortcuts_remain_available_after_knob_focus() {
    let mut h = Harness::new();
    h.skip_intro();
    h.click(pos2(175., 565.));
    h.key(egui::Key::Z, egui::Modifiers::COMMAND);
    h.key(
        egui::Key::Z,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    );
    assert_eq!(h.host.undo_calls, 1);
    assert_eq!(h.host.redo_calls, 1);
    assert_eq!(h.host.parameter(SLOP), Some(16.));
}
