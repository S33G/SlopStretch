use super::*;
use std::f32::consts::{PI, TAU};

pub(super) fn text(
    p: &egui::Painter,
    at: Pos2,
    align: Align2,
    s: &str,
    size: f32,
    color: Color32,
    mono: bool,
) {
    p.text(
        at,
        align,
        s,
        if mono {
            FontId::monospace(size)
        } else {
            FontId::proportional(size)
        },
        color,
    );
}
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0., 1.);
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 * (1. - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1. - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1. - t) + b.b() as f32 * t) as u8,
        (a.a() as f32 * (1. - t) + b.a() as f32 * t) as u8,
    )
}
fn haze(p: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    for i in (1..=20).rev() {
        let k = i as f32 / 20.;
        p.circle_filled(
            center,
            radius * k,
            Color32::from_rgba_unmultiplied(
                color.r(),
                color.g(),
                color.b(),
                ((1. - k) * color.a() as f32 / 5.) as u8,
            ),
        );
    }
}
pub(super) fn background(p: &egui::Painter, rect: Rect, dark: bool, time: f64) {
    let mut mesh = egui::Mesh::default();
    let dim = if dark { 0.74 } else { 1. };
    for (at, color) in [
        (rect.left_top(), Color32::from_rgb(48, 29, 82)),
        (rect.right_top(), Color32::from_rgb(40, 26, 64)),
        (rect.right_bottom(), Color32::from_rgb(111, 43, 106)),
        (rect.left_bottom(), Color32::from_rgb(34, 26, 66)),
    ] {
        mesh.colored_vertex(at, color.gamma_multiply(dim));
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(mesh);
    let t = time as f32;
    haze(
        p,
        rect.min
            + vec2(
                rect.width() * 0.29 + (t * 0.13).sin() * 35.,
                rect.height() * 0.1,
            ),
        360.,
        Color32::from_rgba_unmultiplied(141, 83, 224, 65),
    );
    haze(
        p,
        rect.min
            + vec2(
                rect.width() * 0.88,
                rect.height() * 0.73 + (t * 0.17).cos() * 28.,
            ),
        340.,
        Color32::from_rgba_unmultiplied(255, 76, 164, 48),
    );
    haze(
        p,
        rect.min + vec2(rect.width() * 0.08, rect.height() * 0.72),
        190.,
        Color32::from_rgba_unmultiplied(117, 100, 242, 35),
    );
    // Subtle grain keeps the backdrop from feeling like a flat dashboard.
    for i in 0..210 {
        let x = hash(i as f32 * 1.7);
        let y = hash(i as f32 * 9.2 + 3.);
        p.circle_filled(
            rect.min + vec2(x * rect.width(), y * rect.height()),
            0.55,
            Color32::from_white_alpha(13),
        );
    }
}
pub(super) fn glass(p: &egui::Painter, r: Rect, dark: bool) {
    for (offset, alpha) in [(10., 10), (7., 13), (4., 17)] {
        p.rect_filled(
            r.translate(vec2(0., offset)).expand(2.),
            30.,
            Color32::from_black_alpha(alpha),
        );
    }
    p.rect_filled(
        r,
        28.,
        Color32::from_rgba_unmultiplied(39, 25, 59, if dark { 218 } else { 194 }),
    );
    p.rect_stroke(
        r,
        28.,
        Stroke::new(1., Color32::from_rgba_unmultiplied(255, 216, 255, 37)),
        StrokeKind::Inside,
    );
    p.line_segment(
        [r.left_top() + vec2(30., 1.), r.right_top() + vec2(-30., 1.)],
        Stroke::new(1., Color32::from_white_alpha(16)),
    );
}
pub(super) fn blob(p: &egui::Painter, c: Pos2, r: f32, color: Color32, tilt: f32) {
    let points = (0..64)
        .map(|i| {
            let a = i as f32 / 64. * TAU;
            let rr = r * (1. + 0.085 * (a * 3. + tilt).sin());
            c + vec2(a.cos() * rr, a.sin() * rr)
        })
        .collect();
    p.add(egui::Shape::convex_polygon(
        points,
        color,
        Stroke::new(1., Color32::from_white_alpha(60)),
    ));
    p.circle_filled(
        c + vec2(-r * 0.28, -r * 0.08),
        r * 0.085,
        Color32::from_rgb(68, 31, 83),
    );
    p.circle_filled(
        c + vec2(r * 0.28, -r * 0.08),
        r * 0.085,
        Color32::from_rgb(68, 31, 83),
    );
    p.line(
        vec![
            c + vec2(-r * 0.13, r * 0.25),
            c + vec2(0., r * 0.3),
            c + vec2(r * 0.13, r * 0.25),
        ],
        Stroke::new(1.4, Color32::from_rgb(110, 41, 105)),
    );
    p.circle_filled(
        c + vec2(-r * 0.42, r * 0.15),
        r * 0.12,
        Color32::from_rgba_unmultiplied(250, 94, 172, 90),
    );
    p.circle_filled(
        c + vec2(r * 0.42, r * 0.15),
        r * 0.12,
        Color32::from_rgba_unmultiplied(250, 94, 172, 90),
    );
}
fn hash(n: f32) -> f32 {
    ((n * 127.1 + 311.7).sin() * 43758.547).fract().abs()
}

pub(super) fn waveform(
    p: &egui::Painter,
    r: Rect,
    data: &[f32; 256],
    slop: f64,
    region: [f32; 2],
    transport: (f32, bool),
    time: f64,
) {
    let [start, end] = region;
    let (playhead, freeze) = transport;
    let p = p.with_clip_rect(r.expand(2.));
    p.rect_filled(r, 14., Color32::from_rgba_unmultiplied(20, 15, 36, 140));
    let selected = Rect::from_min_max(
        pos2(r.left() + start * r.width(), r.top()),
        pos2(r.left() + end * r.width(), r.bottom()),
    );
    p.rect_filled(
        selected,
        0.,
        Color32::from_rgba_unmultiplied(141, 77, 146, 25),
    );
    for i in 1..8 {
        let x = r.left() + r.width() * i as f32 / 8.;
        p.line_segment(
            [pos2(x, r.top() + 8.), pos2(x, r.bottom() - 8.)],
            Stroke::new(1., Color32::from_white_alpha(8)),
        );
    }
    p.line_segment(
        [pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)],
        Stroke::new(1., Color32::from_white_alpha(13)),
    );
    let abuse = (slop.max(1.).ln() / 3600f64.ln()) as f32;
    let max = data.iter().copied().fold(0.0001f32, f32::max);
    let width = r.width() / 256.;
    for (i, sample) in data.iter().enumerate() {
        let x = r.left() + i as f32 / 255. * r.width();
        let a = (*sample / max).abs().sqrt() * r.height() * 0.37 + 0.5;
        let selected = i as f32 / 255. >= start && i as f32 / 255. <= end;
        let color = if selected {
            mix(Color32::from_rgb(163, 151, 241), PINK, i as f32 / 255.)
        } else {
            Color32::from_rgb(70, 52, 87)
        };
        if selected && abuse > 0.15 {
            p.line_segment(
                [
                    pos2(x, r.center().y - a * 1.2),
                    pos2(x, r.center().y + a * 1.2),
                ],
                Stroke::new(
                    width * (1. + abuse * 6.),
                    color.gamma_multiply(abuse * 0.08),
                ),
            );
            p.line_segment(
                [pos2(x, r.center().y - a), pos2(x, r.center().y + a)],
                Stroke::new(
                    width * (1. + abuse * 3.),
                    color.gamma_multiply(abuse * 0.15),
                ),
            );
        }
        p.line_segment(
            [pos2(x, r.center().y - a), pos2(x, r.center().y + a)],
            Stroke::new(
                width * 0.64,
                color.gamma_multiply(if selected { 0.85 } else { 0.4 }),
            ),
        );
    }
    for i in 0..(abuse * 55.) as usize {
        let x = r.left() + hash(i as f32 * 2.4) * r.width();
        let y = r.top() + hash(i as f32 * 6.8 + 2.) * r.height();
        let alpha = 70. + 30. * (time as f32 * 0.5 + i as f32).sin();
        p.circle_filled(
            pos2(x, y),
            0.8,
            Color32::from_rgba_unmultiplied(255, 215, 255, alpha as u8),
        );
    }
    for (fraction, is_start) in [(start, true), (end, false)] {
        let x = r.left() + fraction * r.width();
        p.line_segment(
            [pos2(x, r.top()), pos2(x, r.bottom())],
            Stroke::new(1.5, CYAN),
        );
        let tab = Rect::from_min_size(
            pos2(if is_start { x } else { x - 7. }, r.top()),
            vec2(7., 15.),
        );
        p.rect_filled(tab, 2., CYAN);
    }
    let x = r.left() + playhead.clamp(0., 1.) * r.width();
    p.line_segment(
        [pos2(x, r.top() + 6.), pos2(x, r.bottom() - 6.)],
        Stroke::new(1.5, if freeze { PINK } else { WHITE }),
    );
    p.circle_filled(pos2(x, r.top() + 6.), 3., if freeze { PINK } else { WHITE });
}

pub(super) fn draw_knob(
    p: &egui::Painter,
    r: Rect,
    n: f32,
    large: bool,
    hovered: bool,
    value: f64,
) {
    let c = r.center();
    let radius = if large { 82. } else { 23. };
    let ring = radius + if large { 12. } else { 5. };
    if large {
        haze(
            p,
            c,
            radius * 1.44,
            Color32::from_rgba_unmultiplied(236, 100, 207, 48),
        );
        for i in 0..41 {
            let a = (135. + 270. * i as f32 / 40.).to_radians();
            let v = vec2(a.cos(), a.sin());
            p.line_segment(
                [
                    c + v * (ring + 5.),
                    c + v * (ring + if i % 5 == 0 { 11. } else { 8. }),
                ],
                Stroke::new(
                    1.,
                    if i as f32 / 40. <= n {
                        PINK.gamma_multiply(0.6)
                    } else {
                        Color32::from_white_alpha(20)
                    },
                ),
            );
        }
    }
    let arc = |end: f32| {
        (0..=80)
            .map(|i| {
                let a = (135. + 270. * end * i as f32 / 80.).to_radians();
                c + vec2(a.cos(), a.sin()) * ring
            })
            .collect::<Vec<_>>()
    };
    p.line(
        arc(1.),
        Stroke::new(if large { 4. } else { 3. }, Color32::from_rgb(64, 46, 81)),
    );
    if n > 0.001 {
        let pts = arc(n);
        for pair in pts.windows(2).enumerate() {
            let (i, s) = pair;
            p.line_segment(
                [s[0], s[1]],
                Stroke::new(
                    if large { 5. } else { 3. },
                    mix(CYAN, PINK, i as f32 / 80. * n),
                ),
            );
        }
    }
    p.circle_filled(c + vec2(0., 5.), radius + 1., Color32::from_black_alpha(85));
    p.circle_filled(c, radius, Color32::from_rgb(60, 39, 76));
    for i in (1..=14).rev() {
        let k = i as f32 / 14.;
        p.circle_filled(
            c - vec2(radius * 0.08, radius * 0.12),
            radius * k * 0.96,
            Color32::from_rgba_unmultiplied(166, 114, 166, ((1. - k) * 6.) as u8),
        );
    }
    p.circle_stroke(
        c,
        radius,
        Stroke::new(
            1.2,
            if hovered {
                PINK.gamma_multiply(0.85)
            } else {
                Color32::from_rgba_unmultiplied(236, 186, 237, 50)
            },
        ),
    );
    p.circle_stroke(
        c - vec2(0., 1.),
        radius - 4.,
        Stroke::new(1., Color32::from_white_alpha(10)),
    );
    let a = (135. + 270. * n).to_radians();
    let v = vec2(a.cos(), a.sin());
    p.line_segment(
        [c + v * (radius * 0.76), c + v * (radius * 0.90)],
        Stroke::new(if large { 3.5 } else { 2.3 }, PINK),
    );
    if large {
        text(
            p,
            c - vec2(0., 25.),
            Align2::CENTER_CENTER,
            "S L O P",
            11.,
            PINK,
            true,
        );
        text(
            p,
            c + vec2(0., 4.),
            Align2::CENTER_CENTER,
            &format!(
                "{}×",
                if value < 10. {
                    format!("{value:.1}")
                } else {
                    format!("{value:.0}")
                }
            ),
            43.,
            WHITE,
            false,
        );
        text(
            p,
            c + vec2(0., 39.),
            Align2::CENTER_CENTER,
            if value < 4. {
                "MOSTLY FINE"
            } else if value < 100. {
                "CONCERNING"
            } else if value < 1000. {
                "DEEPLY CONCERNING"
            } else {
                "ETERNAL"
            },
            if (100.0..1000.0).contains(&value) {
                8.
            } else {
                10.
            },
            MUTED,
            true,
        );
    } else {
        p.circle_filled(c, 3., Color32::from_rgba_unmultiplied(220, 176, 223, 75));
    }
}

pub(super) fn nebula(
    p: &egui::Painter,
    r: Rect,
    spectrum: &[f32; 64],
    harmonics: f32,
    spread: f32,
    slop: f32,
    time: f64,
) {
    let p = p.with_clip_rect(r);
    p.rect_filled(r, 18., Color32::from_rgba_unmultiplied(16, 14, 36, 190));
    let t = time as f32 * 0.16;
    let drama = (slop.ln() / 3600f32.ln()).clamp(0., 1.) * 0.3 + harmonics * 0.4 + spread * 0.3;
    haze(
        &p,
        r.center() + vec2(r.width() * 0.08, -3.),
        r.height() * 0.76,
        Color32::from_rgba_unmultiplied(202, 68, 203, 40),
    );
    haze(
        &p,
        r.center() - vec2(r.width() * 0.19, 0.),
        r.height() * 0.56,
        Color32::from_rgba_unmultiplied(87, 126, 227, 38),
    );
    // Continuous nebular ribbons become more complex as the actual DSP controls rise.
    for layer in (0..29).rev() {
        let f = layer as f32 / 28.;
        let mut points = Vec::with_capacity(100);
        for i in 0..100 {
            let x = i as f32 / 99.;
            // f32 sin(PI) is slightly negative at the last sample. Clamp the
            // endpoint before a fractional power so all mesh vertices stay finite.
            let env = (x * PI).sin().max(0.).powf(0.6);
            let bin = (x * 63.) as usize;
            let spectral = spectrum[bin].abs().sqrt().min(1.);
            let wave = ((x * TAU * (1.1 + harmonics * 1.2) + t + f * 1.6).sin() * 0.58
                + (x * TAU * 2.7 - t * 0.8 + f * 2.).sin() * 0.24)
                * r.height()
                * (0.18 + drama * 0.2);
            let y = r.center().y
                + (f - 0.5) * r.height() * 0.48 * env
                + wave * env
                + spectral * (f - 0.5) * 18.;
            points.push(pos2(r.left() + x * r.width(), y));
        }
        let color = mix(
            Color32::from_rgb(117, 190, 242),
            Color32::from_rgb(255, 118, 215),
            f,
        );
        p.line(
            points.clone(),
            Stroke::new(4. + drama * 4., color.gamma_multiply(0.025)),
        );
        p.line(
            points,
            Stroke::new(0.65, color.gamma_multiply(0.16 + drama * 0.1)),
        );
    }
    for i in 0..80 {
        let x = hash(i as f32 * 2.7);
        let y = hash(i as f32 * 4.7 + 3.);
        let pulse = 0.6 + 0.25 * (t * 1.9 + i as f32).sin();
        let at = r.min + vec2(x * r.width(), y * r.height());
        let alpha = ((55. + hash(i as f32) * 100.) * pulse) as u8;
        p.circle_filled(
            at,
            if i % 17 == 0 { 1.5 } else { 0.65 },
            Color32::from_rgba_unmultiplied(227, 216, 255, alpha),
        );
        if i % 23 == 0 {
            p.line_segment(
                [at - vec2(3., 0.), at + vec2(3., 0.)],
                Stroke::new(0.5, Color32::from_white_alpha(alpha / 2)),
            );
        }
    }
    for (i, label) in ["20 Hz", "200", "2k", "20k"].iter().enumerate() {
        text(
            &p,
            r.min + vec2(12. + (r.width() - 24.) * i as f32 / 3., r.height() - 11.),
            if i == 3 {
                Align2::RIGHT_CENTER
            } else {
                Align2::LEFT_CENTER
            },
            label,
            8.,
            MUTED.gamma_multiply(0.7),
            true,
        );
    }
}

pub(super) fn mascot(
    p: &egui::Painter,
    canvas: Rect,
    state: &mut UiState,
    ctx: &egui::Context,
    time: f64,
) {
    if let Some(at) = ctx.pointer_hover_pos().filter(|p| canvas.contains(*p)) {
        if state
            .cursor
            .last()
            .is_none_or(|(last, _)| last.distance(at) > 4.)
        {
            state.cursor.push((at, time));
        }
        state.cursor.retain(|(_, t)| time - t < 0.35);
        if state.cursor.len() > 12 {
            state.cursor.remove(0);
        }
        for (at, t) in &state.cursor {
            let life = (1. - (time - t) / 0.35) as f32;
            p.circle_filled(
                *at,
                2.8 * life,
                Color32::from_rgba_unmultiplied(255, 163, 226, (life * 80.) as u8),
            );
        }
        let c = pos2(
            (at.x + 24.).min(canvas.right() - 14.),
            (at.y + 28.).min(canvas.bottom() - 24.),
        );
        blob(p, c, 8.5, PINK.gamma_multiply(0.86), time as f32);
        if time < state.cheer_until {
            let r = Rect::from_min_size(c + vec2(16., -13.), vec2(100., 26.));
            p.rect_filled(r, 13., Color32::from_rgb(87, 48, 100));
            text(
                p,
                r.center(),
                Align2::CENTER_CENTER,
                "Great choice!",
                11.,
                WHITE,
                false,
            );
        }
    }
}

pub(super) fn intro(ui: &mut egui::Ui, canvas: Rect, state: &mut UiState, time: f64) {
    let elapsed = time - state.intro_start.unwrap_or(time);
    let progress = if elapsed < 0.7 {
        elapsed / 0.7 * 0.16
    } else if elapsed < 1.8 {
        0.16 + (elapsed - 0.7) / 1.1 * 0.38
    } else if elapsed < 2.7 {
        0.54 + (elapsed - 1.8) / 0.9 * 0.13
    } else if elapsed < 3.6 {
        0.67 + (elapsed - 2.7) / 0.9 * 0.33
    } else {
        1.
    };
    if elapsed > 4.1 {
        state.intro_done = true;
        return;
    }
    ui.interact(
        canvas,
        Id::new("unnecessary-ceremony-blocker"),
        Sense::click_and_drag(),
    );
    let p = ui.painter().clone();
    p.rect_filled(canvas, 0., Color32::from_rgba_unmultiplied(23, 13, 41, 228));
    let r = Rect::from_center_size(canvas.center(), vec2(700., 382.));
    haze(
        &p,
        r.center(),
        400.,
        Color32::from_rgba_unmultiplied(236, 79, 196, 35),
    );
    glass(&p, r, false);
    let at = |x, y| r.min + vec2(x, y);
    blob(&p, at(49., 49.), 19., PINK, elapsed as f32);
    text(
        &p,
        at(80., 39.),
        Align2::LEFT_TOP,
        "SlopStretch",
        26.,
        WHITE,
        false,
    );
    text(
        &p,
        at(28., 87.),
        Align2::LEFT_TOP,
        "the world’s first AI-native ambient slop engine",
        22.,
        WHITE,
        false,
    );
    text(
        &p,
        at(29., 120.),
        Align2::LEFT_TOP,
        "ABSOLUTELY NO AI. JUST FOURIER AND FEELINGS.",
        10.,
        MUTED,
        true,
    );
    let percent = (progress * 100.).floor();
    let stage = if progress < 0.16 {
        "Locating a completely unnecessary wait…"
    } else if progress < 0.54 {
        "Stretching the concept of productivity…"
    } else if progress < 0.67 {
        "Asking the pixels to feel something…"
    } else if progress < 1. {
        "Dissolving into premium, artisanal slop…"
    } else {
        "100% stretched. Emotionally uncompressed."
    };
    text(&p, at(29., 156.), Align2::LEFT_TOP, stage, 15., PINK, false);
    text(
        &p,
        at(670., 206.),
        Align2::RIGHT_CENTER,
        &format!("{percent:.0}%"),
        47.,
        WHITE,
        true,
    );
    let bar = Rect::from_min_size(at(30., 202.), vec2(475., 60.));
    p.rect_filled(bar, 24., Color32::from_rgba_unmultiplied(13, 10, 24, 150));
    let pixel = if progress < 0.54 {
        5.
    } else if progress < 0.8 {
        8.
    } else {
        12.
    };
    let cols = (bar.width() / pixel) as usize;
    let rows = (bar.height() / pixel) as usize;
    for x in 0..cols {
        for y in 0..rows {
            if x as f64 / (cols as f64) > progress {
                continue;
            }
            let noise = hash(x as f32 * 3.7 + y as f32 * 11.3 + (elapsed * 8.).floor() as f32);
            if noise < 0.08 && progress > 0.6 {
                continue;
            }
            let center = (y as f32 + 0.5) / rows as f32;
            let edge = if progress < 0.55 {
                (center - 0.5).abs() < 0.32
            } else {
                true
            };
            if edge {
                p.rect_filled(
                    Rect::from_min_size(
                        bar.min + vec2(x as f32 * pixel + 1., y as f32 * pixel + 1.),
                        vec2(pixel - 2., pixel - 2.),
                    ),
                    if progress < 0.6 { 2. } else { 0. },
                    mix(CYAN, PINK, x as f32 / cols as f32).gamma_multiply(0.5 + noise * 0.5),
                );
            }
        }
    }
    // Letter spacing literally stretches, while the progress indicator dissolves into pixels.
    let spacing = 14. + progress as f32 * 15.;
    let label = "S T R E T C H";
    let count = label.chars().count() as f32;
    for (i, ch) in label.chars().enumerate() {
        text(
            &p,
            at(350. + (i as f32 - count / 2.) * spacing, 293.),
            Align2::CENTER_CENTER,
            &ch.to_string(),
            14.,
            PINK,
            true,
        );
    }
    text(
        &p,
        at(30., 351.),
        Align2::LEFT_CENTER,
        "Artificial wait. Authentic commitment to the bit.",
        11.,
        MUTED,
        false,
    );
    if button(
        ui,
        Rect::from_min_size(at(466., 333.), vec2(91., 32.)),
        "Less motion",
        "Skip this purely decorative loader and disable decorative motion.",
        0,
    )
    .clicked()
    {
        state.reduced_motion = true;
        state.intro_done = true;
    }
    if button(
        ui,
        Rect::from_min_size(at(569., 333.), vec2(103., 32.)),
        "Skip the bit",
        "Skip the artificial startup delay. Audio processing is never delayed.",
        1,
    )
    .clicked()
    {
        state.intro_done = true;
    }
}
