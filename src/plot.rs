use std::borrow::Borrow;

use plotters::coord::ranged1d::{BoldPoints, DefaultFormatting, KeyPointHint, Ranged};
use plotters::style::text_anchor::{HPos, Pos, VPos};
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
fn face_index(f: Face) -> usize {
    (f as u8).trailing_zeros() as usize
}

#[inline]
pub fn face_color(f: Face) -> RGBColor {
    PALETTE[face_index(f)]
}

/// x-axis with explicit tick points every 30s
struct XTicks {
    inner: RangedCoordf64,
    ticks: Vec<f64>,
    minor: bool,
}

impl Ranged for XTicks {
    type FormatOption = DefaultFormatting;
    type ValueType = f64;

    #[inline]
    fn map(&self, v: &f64, limit: (i32, i32)) -> i32 {
        self.inner.map(v, limit)
    }

    #[inline]
    fn key_points<H: KeyPointHint>(&self, hint: H) -> Vec<f64> {
        if self.minor && hint.weight().allow_light_points() {
            self.ticks
                .iter()
                .map(|&t| t + 15.0)
                .filter(|&x| self.inner.range().contains(&x))
                .collect()
        } else {
            self.ticks.clone()
        }
    }

    #[inline]
    fn range(&self) -> std::ops::Range<f64> {
        self.inner.range()
    }
}

/// y-axis with unlabeled minor lines halfway between the auto ticks
struct YTicks {
    inner: RangedCoordf64,
    minor: bool,
}

impl Ranged for YTicks {
    type FormatOption = DefaultFormatting;
    type ValueType = f64;

    #[inline]
    fn map(&self, v: &f64, limit: (i32, i32)) -> i32 {
        self.inner.map(v, limit)
    }

    #[inline]
    fn key_points<H: KeyPointHint>(&self, hint: H) -> Vec<f64> {
        let bold = self.inner.key_points(BoldPoints(hint.bold_points()));
        if self.minor && hint.weight().allow_light_points() {
            bold.windows(2).map(|w| 0.5 * (w[0] + w[1])).collect()
        } else {
            bold
        }
    }

    #[inline]
    fn range(&self) -> std::ops::Range<f64> {
        self.inner.range()
    }
}

fn y_ticks(y_max: f64, minor: bool) -> YTicks {
    YTicks {
        inner: (0.0..y_max).into(),
        minor,
    }
}

fn x_ticks(x_max: f64, x_min: f64, minor: bool) -> XTicks {
    XTicks {
        inner: (x_min..x_max).into(),
        ticks: (0..=(x_max / 30.0) as usize).map(|i| i as f64 * 30.0).collect(),
        minor,
    }
}


fn make_chart_context<'a, DB: DrawingBackend>(
    root: &'a DrawingArea<DB, Shift>,
    caption: &str,
    theme: &Theme,
    xlab: &str,
    ylab: &str,
    (x_start, x_max, y_max): (f64, f64, f64),
    minor: bool,
) -> Result<ChartContext<'a, DB, Cartesian2d<XTicks, YTicks>>, DrawingAreaErrorKind<DB::ErrorType>>
where
    DB::ErrorType: 'static,
{
    let mut ctx = ChartBuilder::on(root)
        .margin(10)
        .margin_right(22)
        .set_label_area_size(LabelAreaPosition::Left, 90)
        .set_label_area_size(LabelAreaPosition::Bottom, 40)
        .caption(caption, text_style(CAPFONT, theme))
        .build_cartesian_2d(x_ticks(x_max, x_start, minor), y_ticks(y_max, minor))?;

    {
        let mut mesh = ctx.configure_mesh();
        mesh.x_desc(xlab)
            .y_desc(ylab)
            .y_max_light_lines(1)
            .x_label_formatter(&|v| format!("{v:.0}"))
            .y_label_formatter(&|v| format!("{v:.0}"))
            .axis_style(theme.grid_bold)
            .bold_line_style(theme.grid_bold.stroke_width(if minor { 2 } else { 1 }))
            .light_line_style(theme.grid_light)
            .label_style(text_style(LABFONT, theme))
            .axis_desc_style(text_style(LABFONT, theme));
        if minor {
            mesh.x_max_light_lines(1);
        }
        mesh.draw()?;
    }

    if minor {
        let xstyle = text_style(LABFONT, theme).pos(Pos::new(HPos::Center, VPos::Top));
        let ystyle = text_style(LABFONT, theme).pos(Pos::new(HPos::Right, VPos::Center));
        ctx.draw_series(
            (0..)
                .map(|k| 15.0 + 30.0 * k as f64)
                .take_while(|&x| x < x_max)
                .map(|x| {
                    EmptyElement::at((x, 0.0)) + Text::new(format!("{x:.0}"), (0, 11), xstyle.clone())
                }),
        )?;

        let y_bold = RangedCoordf64::from(0.0..y_max).key_points(BoldPoints(11));
        ctx.draw_series(y_bold.windows(2).map(|w| {
            let y = 0.5 * (w[0] + w[1]);
            EmptyElement::at((0.0, y)) + Text::new(format!("{y:.0}"), (-10, 0), ystyle.clone())
        }))?;
    }
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
    let mut by_face: [Vec<(f64, f64)>; 6] = Default::default();
    for e in &sig.events {
        let t = (e.time / dt).round() * dt;
        let impulses = &mut by_face[face_index(e.face)];
        match impulses.last_mut() {
            Some(last) if last.0 == t => last.1 += e.damage,
            _ => impulses.push((t, e.damage)),
        }
    }

    root.fill(&theme.bg)?;
    let mut ctx =
        make_chart_context(root, caption, theme, "time (s)", "damage", (-10.0, x_max, PLOT_Y_MAX), false)?;

    for &face in &sig.rotation {
        let impulses = &by_face[face_index(face)];
        if impulses.is_empty() {
            continue;
        }
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
        make_chart_context(&chart_area, caption, theme, "time (s)", "cumulative damage", (0.0, x_max, y_max), true)?;

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
                name.as_str(),
                (x + SWATCH_W + LABEL_GAP, y - 7),
                text_style(LABFONT, theme),
            ))?;
            x += entry_width(name);
        }
    }
    Ok(())
}

