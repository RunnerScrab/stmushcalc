use std::borrow::Borrow;

use fxhash::FxHashMap;
use plotters::coord::ranged1d::{DefaultFormatting, KeyPointHint, Ranged};
use plotters::coord::types::RangedCoordf64;
use plotters::coord::Shift;
use plotters::prelude::*;

use crate::{DamageSignal, Face};

static CAPFONT: (&str, u16) = ("sans-serif", 24);
static LABFONT: (&str, u16) = ("sans-serif", 14);

fn text_style(font: (&'static str, u16), theme: &Theme) -> TextStyle<'static> {
    font.into_font().color(&theme.fg)
}

/// We want the same y range for each ship, so that damage is
/// instantly comparable
pub const PLOT_Y_MAX: f64 = 22_000.0;

pub struct Theme {
    pub bg: RGBColor,
    pub fg: RGBColor,
    pub grid_light: RGBColor,
    pub grid_bold: RGBColor,
}

pub static LIGHT: Theme = Theme {
    bg: RGBColor(255, 255, 255),
    fg: RGBColor(0, 0, 0),
    grid_light: RGBColor(224, 224, 224),
    grid_bold: RGBColor(120, 120, 120),
};

pub static DARK: Theme = Theme {
    bg: RGBColor(29, 29, 32),
    fg: RGBColor(220, 220, 225),
    grid_light: RGBColor(50, 50, 55),
    grid_bold: RGBColor(90, 90, 95),
};

/// Shared palette, used for ship cumulative damage signals and weapon faces
pub static PALETTE: [RGBColor; 11] = [
    RGBColor(31, 119, 180),
    RGBColor(255, 127, 14),
    RGBColor(44, 160, 44),
    RGBColor(214, 39, 40),
    RGBColor(148, 103, 189),
    RGBColor(140, 86, 75),
    RGBColor(227, 119, 194),
    RGBColor(127, 127, 127),
    RGBColor(255, 0, 0),
    RGBColor(0, 255, 0),
    RGBColor(0, 0, 255),
];

#[inline]
fn series_color(i: usize) -> RGBColor {
    PALETTE[i % PALETTE.len()]
}

#[inline]
pub fn face_color(f: Face) -> RGBColor {
    let idx = match f {
        Face::Fore => 0,
        Face::Aft => 1,
        Face::Port => 2,
        Face::Starboard => 3,
        Face::Dorsal => 4,
        Face::Ventral => 5,
    };
    PALETTE[idx]
}

/// x-axis with explicit tick points every 30s
struct XTicks {
    inner: RangedCoordf64,
    ticks: Vec<f64>,
}

impl Ranged for XTicks {
    type FormatOption = DefaultFormatting;
    type ValueType = f64;

    #[inline]
    fn map(&self, v: &f64, limit: (i32, i32)) -> i32 {
        self.inner.map(v, limit)
    }

    #[inline]
    fn key_points<H: KeyPointHint>(&self, _hint: H) -> Vec<f64> {
        self.ticks.clone()
    }

    #[inline]
    fn range(&self) -> std::ops::Range<f64> {
        self.inner.range()
    }
}

fn x_ticks(x_max: f64, x_min: f64) -> XTicks {
    XTicks {
        inner: (x_min..(x_max + 10.0)).into(),
        ticks: (0..=(x_max / 30.0) as usize).map(|i| i as f64 * 30.0).collect(),
    }
}


fn make_chart_context<'a, DB: DrawingBackend>(
    root: &'a DrawingArea<DB, Shift>,
    caption: &str,
    theme: &Theme,
    xlab: &str,
    ylab: &str,
    (x_start, x_max, y_max): (f64, f64, f64),
) -> Result<ChartContext<'a, DB, Cartesian2d<XTicks, RangedCoordf64>>, DrawingAreaErrorKind<DB::ErrorType>>
where
    DB::ErrorType: 'static,
{
    let mut ctx = ChartBuilder::on(root)
        .margin(10)
        .set_label_area_size(LabelAreaPosition::Left, 90)
        .set_label_area_size(LabelAreaPosition::Bottom, 40)
        .caption(caption, text_style(CAPFONT, theme))
        .build_cartesian_2d(x_ticks(x_max, x_start), 0.0..y_max)?;

    ctx.configure_mesh()
        .x_desc(xlab)
        .y_desc(ylab)
        .y_max_light_lines(1)
        .axis_style(theme.grid_bold)
        .bold_line_style(theme.grid_bold)
        .light_line_style(theme.grid_light)
        .label_style(text_style(LABFONT, theme))
        .axis_desc_style(text_style(LABFONT, theme))
        .draw()?;
    Ok(ctx)
}

/// One ship's per-arc instantaneous damage, colored by face
pub fn render_instantaneous<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    sig: &DamageSignal,
    caption: &str,
    theme: &Theme,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>>
where
    DB::ErrorType: 'static,
{
    let dt = sig.dt;
    let x_max = sig.horizon;

    // events are time-sorted, so a face's same-bin shots are contiguous
    let mut by_face: FxHashMap<Face, Vec<(f64, f64)>> = FxHashMap::default();
    for e in &sig.events {
        let t = (e.time / dt).round() * dt;
        let impulses = by_face.entry(e.face).or_default();
        match impulses.last_mut() {
            Some(last) if last.0 == t => last.1 += e.damage,
            _ => impulses.push((t, e.damage)),
        }
    }

    root.fill(&theme.bg)?;
    let mut ctx =
        make_chart_context(root, caption, theme, "time (s)", "damage", (-10.0, x_max, PLOT_Y_MAX))?;

    for &face in &sig.rotation {
        let Some(impulses) = by_face.get(&face) else {
            continue;
        };
        let color = face_color(face);
        ctx.draw_series(impulses.iter().map(|&(t, v)| {
            PathElement::new(vec![(t, 0.0), (t, v)], color.stroke_width(2))
        }))?
        .label(face.label().to_string())
        .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color.stroke_width(3)));
    }

    ctx.configure_series_labels()
        .border_style(theme.grid_bold)
        .background_style(theme.bg.mix(0.85))
        .label_font(text_style(LABFONT, theme))
        .draw()?;
    Ok(())
}

#[inline]
fn text_width_estimate(text: &str) -> i32 {
    text.chars().count() as i32 * 8
}

pub fn render_cumulative<DB: DrawingBackend, S: Borrow<DamageSignal>>(
    root: &DrawingArea<DB, Shift>,
    sims: &[(String, S)],
    caption: &str,
    theme: &Theme,
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>>
where
    DB::ErrorType: 'static,
{
    let x_max = sims
        .iter()
        .map(|(_, s)| s.borrow().horizon)
        .fold(0.0_f64, f64::max);

    let y_max = sims
        .iter()
        .map(|(_, s)| s.borrow().total)
        .fold(0.0_f64, f64::max)
        .max(1.0)
        * 1.05;

    // Lay out legend entries into wrapped rows so we know how tall a strip to
    // reserve below the chart before splitting the drawing area
    const SWATCH_W: i32 = 20;
    const ENTRY_GAP: i32 = 24;
    const ROW_H: i32 = 22;
    const PAD: i32 = 10;
    const LABEL_GAP: i32 = 6;
    let entry_width = |name: &str| SWATCH_W + LABEL_GAP + text_width_estimate(name) + ENTRY_GAP;

    let (canvas_w, _) = root.dim_in_pixel();
    let mut rows: Vec<Vec<usize>> = vec![vec![]];
    let mut x = PAD;
    for (i, (name, _)) in sims.iter().enumerate() {
        let w = entry_width(name);
        if x + w > canvas_w as i32 - PAD && !rows.last().unwrap().is_empty() {
            rows.push(vec![]);
            x = PAD;
        }
        rows.last_mut().unwrap().push(i);
        x += w;
    }
    let legend_h = PAD * 2 + ROW_H * rows.len() as i32;

    let (chart_area, legend_area) = root.split_vertically(root.dim_in_pixel().1 as i32 - legend_h);

    chart_area.fill(&theme.bg)?;
    let mut ctx =
        make_chart_context(&chart_area, caption, theme, "time (s)", "cumulative damage", (0.0, x_max, y_max))?;

    for (i, (_, sig)) in sims.iter().enumerate() {
        let sig = sig.borrow();
        let color = series_color(i);
        ctx.draw_series(LineSeries::new(
            sig.cumulative.iter().copied(),
            color.stroke_width(1),
        ))?;
    }

    // Flat legend box below the chart
    legend_area.fill(&theme.bg)?;
    legend_area.draw(&Rectangle::new(
        [(0, 0), (canvas_w as i32 - 1, legend_h - 1)],
        theme.grid_bold.stroke_width(1),
    ))?;
    for (row_idx, row) in rows.iter().enumerate() {
        let y = PAD + row_idx as i32 * ROW_H + ROW_H / 2;
        let mut x = PAD;
        for &i in row {
            let (name, _) = &sims[i];
            let color = series_color(i);
            legend_area.draw(&PathElement::new(
                vec![(x, y), (x + SWATCH_W, y)],
                color.stroke_width(3),
            ))?;
            legend_area.draw(&Text::new(
                name.clone(),
                (x + SWATCH_W + LABEL_GAP, y - 7),
                text_style(LABFONT, theme),
            ))?;
            x += entry_width(name);
        }
    }
    Ok(())
}

