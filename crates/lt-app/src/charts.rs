//! Graphiques de série temporelle dessinés nativement (aucune dépendance web).

use gpui::{
    App, Background, Bounds, Hsla, IntoElement, Pixels, Point, Styled, Window, canvas, point, px,
};
use gpui::{Path, PathBuilder};

const CHART_HEIGHT: f32 = 160.0;

/// `paint_path` attend des coordonnées fenêtre : on dessine en local puis on
/// décale à l'origine du canvas.
fn build_at(mut builder: PathBuilder, origin: Point<Pixels>) -> Option<Path<Pixels>> {
    builder.translate(origin);
    builder.build().ok()
}

fn empty_path() -> Path<Pixels> {
    Path::new(point(px(0.), px(0.)))
}

/// Courbe + aire, échelle automatique sur la série fournie.
pub struct Sparkline {
    values: Vec<f64>,
    color: Hsla,
    grid: Hsla,
    height: f32,
    filled: bool,
    baseline_zero: bool,
    max_value: Option<f64>,
    show_range: bool,
}

impl Sparkline {
    pub fn new(values: Vec<f64>, color: Hsla, grid: Hsla) -> Self {
        Self {
            values,
            color,
            grid,
            height: CHART_HEIGHT,
            filled: true,
            baseline_zero: true,
            max_value: None,
            show_range: true,
        }
    }

    #[allow(dead_code)]
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    #[allow(dead_code)]
    pub fn baseline_zero(mut self, enabled: bool) -> Self {
        self.baseline_zero = enabled;
        self
    }
}

type SparkPaint = (
    Path<Pixels>,
    Option<Path<Pixels>>,
    [Path<Pixels>; 3],
    Option<Path<Pixels>>,
    Option<Path<Pixels>>,
    f64,
    f64,
);

impl IntoElement for Sparkline {
    type Element = gpui::Canvas<SparkPaint>;

    fn into_element(self) -> Self::Element {
        let values = self.values;
        let color = self.color;
        let grid = self.grid;
        let filled = self.filled;
        let baseline_zero = self.baseline_zero;
        let max_value = self.max_value;
        let show_range = self.show_range;

        canvas(
            move |bounds: Bounds<Pixels>, _window: &mut Window, _cx: &mut App| {
                let width = f32::from(bounds.size.width).max(1.0);
                let height = f32::from(bounds.size.height).max(1.0);
                let pad_x = 6.0;
                let pad_y = 10.0;
                let plot_w = (width - pad_x * 2.0).max(1.0);
                let plot_h = (height - pad_y * 2.0).max(1.0);

                let mut grid_paths = [
                    PathBuilder::stroke(px(1.)),
                    PathBuilder::stroke(px(1.)),
                    PathBuilder::stroke(px(1.)),
                ];
                for (index, builder) in grid_paths.iter_mut().enumerate() {
                    let y = pad_y + plot_h * (index as f32 / 2.0);
                    builder.move_to(point(px(pad_x), px(y)));
                    builder.line_to(point(px(pad_x + plot_w), px(y)));
                }
                let origin = bounds.origin;
                let grid_paths =
                    grid_paths.map(|builder| build_at(builder, origin).unwrap_or_else(empty_path));

                let empty = (
                    empty_path(),
                    None,
                    grid_paths.clone(),
                    None,
                    None,
                    0.0,
                    0.0,
                );

                if values.len() < 2 {
                    return empty;
                }

                let data_min = values.iter().copied().fold(f64::INFINITY, f64::min);
                let data_max = max_value
                    .unwrap_or_else(|| values.iter().copied().fold(f64::NEG_INFINITY, f64::max));
                let min = if baseline_zero {
                    0.0_f64.min(data_min)
                } else {
                    data_min
                };
                let max = data_max.max(min + 1e-6);
                let map_y = |value: f64| -> f32 {
                    let ratio = ((value - min) / (max - min)).clamp(0.0, 1.0);
                    pad_y + plot_h - (ratio as f32) * plot_h
                };
                let map_x = |index: usize| -> f32 {
                    pad_x + (index as f32 / (values.len() - 1) as f32) * plot_w
                };

                let mut line = PathBuilder::stroke(px(2.0));
                for (index, value) in values.iter().enumerate() {
                    let x = map_x(index);
                    let y = map_y(*value);
                    if index == 0 {
                        line.move_to(point(px(x), px(y)));
                    } else {
                        line.line_to(point(px(x), px(y)));
                    }
                }
                let line_path = build_at(line, origin).unwrap_or_else(empty_path);

                let area_path = if filled {
                    let mut area = PathBuilder::fill();
                    area.move_to(point(px(pad_x), px(pad_y + plot_h)));
                    for (index, value) in values.iter().enumerate() {
                        area.line_to(point(px(map_x(index)), px(map_y(*value))));
                    }
                    area.line_to(point(px(pad_x + plot_w), px(pad_y + plot_h)));
                    area.close();
                    build_at(area, origin)
                } else {
                    None
                };

                let last = values.last().copied().unwrap_or(0.0);
                let last_x = map_x(values.len() - 1);
                let last_y = map_y(last);
                let mut dot = PathBuilder::fill();
                let r = 3.5_f32;
                dot.move_to(point(px(last_x - r), px(last_y)));
                dot.line_to(point(px(last_x), px(last_y - r)));
                dot.line_to(point(px(last_x + r), px(last_y)));
                dot.line_to(point(px(last_x), px(last_y + r)));
                dot.close();
                let dot_path = build_at(dot, origin);

                let baseline = if show_range {
                    let mut base = PathBuilder::stroke(px(1.25));
                    base.move_to(point(px(pad_x), px(pad_y + plot_h)));
                    base.line_to(point(px(pad_x + plot_w), px(pad_y + plot_h)));
                    build_at(base, origin)
                } else {
                    None
                };

                (line_path, area_path, grid_paths, dot_path, baseline, min, max)
            },
            move |_bounds,
                  (line, area, grids, dot, baseline, _min, _max): SparkPaint,
                  window: &mut Window,
                  _cx: &mut App| {
                for grid_path in grids {
                    window.paint_path(grid_path, grid.opacity(0.4));
                }
                if let Some(baseline) = baseline {
                    window.paint_path(baseline, grid.opacity(0.6));
                }
                if let Some(area) = area {
                    let fill: Background = color.opacity(0.10).into();
                    window.paint_path(area, fill);
                }
                window.paint_path(line, color);
                if let Some(dot) = dot {
                    window.paint_path(dot, color);
                }
            },
        )
        .h(px(self.height))
        .w_full()
    }
}

/// Barres compactes (débit par seconde par exemple).
pub struct Bars {
    values: Vec<f64>,
    color: Hsla,
    height: f32,
}

impl Bars {
    pub fn new(values: Vec<f64>, color: Hsla) -> Self {
        Self {
            values,
            color,
            height: CHART_HEIGHT,
        }
    }

    #[allow(dead_code)]
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
}

impl IntoElement for Bars {
    type Element = gpui::Canvas<(Vec<(Path<Pixels>, bool)>, Option<Path<Pixels>>)>;

    fn into_element(self) -> Self::Element {
        let values = self.values;
        let color = self.color;

        canvas(
            move |bounds: Bounds<Pixels>, _window: &mut Window, _cx: &mut App| {
                let width = f32::from(bounds.size.width).max(1.0);
                let height = f32::from(bounds.size.height).max(1.0);
                if values.is_empty() {
                    return (Vec::new(), None);
                }
                let min = 0.0_f64;
                let max = values
                    .iter()
                    .copied()
                    .fold(f64::NEG_INFINITY, f64::max)
                    .max(1e-6);
                let slot = width / values.len() as f32;
                let bar_width = (slot * 0.62).max(1.5).min(slot * 0.9);
                let radius = (bar_width * 0.22).min(4.0);
                let mut paths = Vec::new();
                for (index, value) in values.iter().enumerate() {
                    let ratio = ((value - min) / (max - min)).clamp(0.0, 1.0) as f32;
                    let bar_height = (ratio * (height - 10.0)).max(1.5);
                    let x = index as f32 * slot + (slot - bar_width) / 2.0;
                    let y = height - bar_height - 5.0;
                    let is_max = (*value - max).abs() < 1e-9;
                    let mut builder = PathBuilder::fill();
                    builder.move_to(point(px(x), px(y + radius)));
                    builder.line_to(point(px(x + radius), px(y)));
                    builder.line_to(point(px(x + bar_width - radius), px(y)));
                    builder.line_to(point(px(x + bar_width), px(y + radius)));
                    builder.line_to(point(px(x + bar_width), px(height - 5.0)));
                    builder.line_to(point(px(x), px(height - 5.0)));
                    builder.close();
                    if let Some(path) = build_at(builder, bounds.origin) {
                        paths.push((path, is_max));
                    }
                }
                let mut baseline = PathBuilder::stroke(px(1.25));
                baseline.move_to(point(px(0.), px(height - 5.0)));
                baseline.line_to(point(px(width), px(height - 5.0)));
                let baseline = build_at(baseline, bounds.origin);
                (paths, baseline)
            },
            move |_bounds,
                  (paths, baseline): (Vec<(Path<Pixels>, bool)>, Option<Path<Pixels>>),
                  window: &mut Window,
                  _cx: &mut App| {
                if let Some(baseline) = baseline {
                    window.paint_path(baseline, color.opacity(0.3));
                }
                for (path, is_max) in paths {
                    let paint = if is_max {
                        color
                    } else {
                        color.opacity(0.72)
                    };
                    window.paint_path(path, paint);
                }
            },
        )
        .h(px(self.height))
        .w_full()
    }
}

/// Mini pictogramme de forme de profil (chemin SVG simplifié M/L).
pub struct ProfileShape {
    path: &'static str,
    color: Hsla,
    width: f32,
    height: f32,
}

impl ProfileShape {
    pub fn new(path: &'static str, color: Hsla) -> Self {
        Self {
            path,
            color,
            width: 44.0,
            height: 18.0,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
}

impl IntoElement for ProfileShape {
    type Element = gpui::Canvas<Option<Path<Pixels>>>;

    fn into_element(self) -> Self::Element {
        let d = self.path;
        let color = self.color;
        let view_w = 100.0_f32;
        let view_h = 28.0_f32;

        canvas(
            move |bounds: Bounds<Pixels>, _window: &mut Window, _cx: &mut App| {
                let width = f32::from(bounds.size.width).max(1.0);
                let height = f32::from(bounds.size.height).max(1.0);
                // Segments disjoints (ex. TPS : ligne + ticks) : un `M` repart à zéro.
                let mut stroke = PathBuilder::stroke(px(2.0));
                let mut started = false;
                let mut point_count = 0usize;
                for token in d.split_whitespace() {
                    let is_move = token.starts_with('M') || token.starts_with('m');
                    let bare = token.trim_start_matches(['M', 'L', 'm', 'l']);
                    let Some((x, y)) = bare.split_once(',') else {
                        continue;
                    };
                    let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) else {
                        continue;
                    };
                    point_count += 1;
                    let px_x = (x / view_w) * width;
                    let px_y = (y / view_h) * height;
                    if is_move || !started {
                        stroke.move_to(point(px(px_x), px(px_y)));
                        started = true;
                    } else {
                        stroke.line_to(point(px(px_x), px(px_y)));
                    }
                }
                if point_count < 2 {
                    return None;
                }
                build_at(stroke, bounds.origin)
            },
            move |_bounds, path: Option<Path<Pixels>>, window: &mut Window, _cx: &mut App| {
                if let Some(path) = path {
                    window.paint_path(path, color);
                }
            },
        )
        .w(px(self.width))
        .h(px(self.height))
    }
}

/// Aperçu de la courbe planifiée (VUs ou TPS dans le temps).
pub struct ProfilePreview {
    points: Vec<(f64, f64)>,
    color: Hsla,
    grid: Hsla,
    height: f32,
}

impl ProfilePreview {
    pub fn new(points: Vec<(f64, f64)>, color: Hsla, grid: Hsla) -> Self {
        Self {
            points,
            color,
            grid,
            height: 88.0,
        }
    }
}

impl IntoElement for ProfilePreview {
    type Element = gpui::Canvas<(
        Path<Pixels>,
        Option<Path<Pixels>>,
        [Path<Pixels>; 3],
        Option<Path<Pixels>>,
    )>;

    fn into_element(self) -> Self::Element {
        let points = self.points;
        let color = self.color;
        let grid = self.grid;

        canvas(
            move |bounds: Bounds<Pixels>, _window: &mut Window, _cx: &mut App| {
                let width = f32::from(bounds.size.width).max(1.0);
                let height = f32::from(bounds.size.height).max(1.0);
                let pad_x = 8.0;
                let pad_y = 10.0;
                let plot_w = (width - pad_x * 2.0).max(1.0);
                let plot_h = (height - pad_y * 2.0).max(1.0);

                let mut grid_paths = [
                    PathBuilder::stroke(px(1.)),
                    PathBuilder::stroke(px(1.)),
                    PathBuilder::stroke(px(1.)),
                ];
                for (index, builder) in grid_paths.iter_mut().enumerate() {
                    let y = pad_y + plot_h * (index as f32 / 2.0);
                    builder.move_to(point(px(pad_x), px(y)));
                    builder.line_to(point(px(pad_x + plot_w), px(y)));
                }
                let origin = bounds.origin;
                let grid_paths =
                    grid_paths.map(|builder| build_at(builder, origin).unwrap_or_else(empty_path));

                if points.len() < 2 {
                    return (
                        empty_path(),
                        None,
                        grid_paths,
                        None,
                    );
                }

                let t_max = points.last().map(|(t, _)| *t).unwrap_or(1.0).max(1.0);
                let v_max = points
                    .iter()
                    .map(|(_, v)| *v)
                    .fold(0.0_f64, f64::max)
                    .max(1.0);
                let map_x = |t: f64| pad_x + ((t / t_max) as f32) * plot_w;
                let map_y = |v: f64| {
                    let ratio = (v / v_max).clamp(0.0, 1.0) as f32;
                    pad_y + plot_h - ratio * plot_h
                };

                let mut line = PathBuilder::stroke(px(2.0));
                for (index, (t, v)) in points.iter().enumerate() {
                    let x = map_x(*t);
                    let y = map_y(*v);
                    if index == 0 {
                        line.move_to(point(px(x), px(y)));
                    } else {
                        line.line_to(point(px(x), px(y)));
                    }
                }
                let line_path = build_at(line, origin).unwrap_or_else(empty_path);

                let mut area = PathBuilder::fill();
                area.move_to(point(px(pad_x), px(pad_y + plot_h)));
                for (t, v) in &points {
                    area.line_to(point(px(map_x(*t)), px(map_y(*v))));
                }
                area.line_to(point(px(pad_x + plot_w), px(pad_y + plot_h)));
                area.close();
                let area_path = build_at(area, origin);

                let mut baseline = PathBuilder::stroke(px(1.25));
                baseline.move_to(point(px(pad_x), px(pad_y + plot_h)));
                baseline.line_to(point(px(pad_x + plot_w), px(pad_y + plot_h)));
                let baseline = build_at(baseline, origin);

                (line_path, area_path, grid_paths, baseline)
            },
            move |_bounds, (line, area, grids, baseline), window: &mut Window, _cx: &mut App| {
                for grid_path in grids {
                    window.paint_path(grid_path, grid.opacity(0.4));
                }
                if let Some(baseline) = baseline {
                    window.paint_path(baseline, grid.opacity(0.6));
                }
                if let Some(area) = area {
                    let fill: Background = color.opacity(0.16).into();
                    window.paint_path(area, fill);
                }
                window.paint_path(line, color);
            },
        )
        .h(px(self.height))
        .w_full()
    }
}
