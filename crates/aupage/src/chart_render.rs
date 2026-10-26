use crate::types::*;
use anyhow::Result;
use charton::chart::Chart;
use charton::core::data::Dataset;
use charton::core::conversion::IntoLayered;
use charton::coordinate::CoordSystem;
use std::sync::OnceLock;

/// Register a CJK font for charton PNG rendering.
fn ensure_cjk_font() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        // Try common CJK font paths
        let font_paths = [
            "/usr/share/fonts/truetype/chinese/yahei.ttf",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/arphic/uming.ttc",
        ];
        for path in font_paths {
            if let Ok(data) = std::fs::read(path) {
                if charton::core::utils::register_raster_font("CJK", data).is_ok() {
                    return;
                }
            }
        }
    });
}

/// Render a chart element to PNG bytes using charton.
/// Returns PNG bytes or None if the chart type is not supported by charton.
pub fn render_chart(el: &ElementChart) -> Result<Option<Vec<u8>>> {
    match el.chart_type {
        ChartType::Line => render_line(el),
        ChartType::Area => render_area(el),
        ChartType::Bar => render_bar(el),
        ChartType::Column => render_column(el),
        ChartType::Pie => render_pie(el),
        ChartType::Ring => render_ring(el),
        ChartType::Scatter => render_scatter(el),
        ChartType::Radar => Ok(None), // charton doesn't support radar
    }
}

fn apply_labels(
    lc: charton::core::composite::LayeredChart,
    el: &ElementChart,
) -> charton::core::composite::LayeredChart {
    ensure_cjk_font();
    let mut lc = lc
        .with_size(800, 500)
        .configure_theme(|t| t
            .with_title_size(14.0)
            .with_title_family("CJK")
            .with_label_size(12.0)
            .with_label_family("CJK")
            .with_label_padding(8.0)
            .with_tick_label_size(11.0)
            .with_tick_label_family("CJK")
            .with_legend_label_size(11.0)
            .with_legend_label_family("CJK")
        );
    if let Some(title) = &el.title {
        lc = lc.with_title(title);
    }
    if let Some(config) = &el.config {
        if let Some(x_name) = &config.x_axis_name {
            lc = lc.with_x_label(x_name);
        }
        if let Some(y_name) = &config.y_axis_name {
            lc = lc.with_y_label(y_name);
        }
    }
    lc
}

fn render_line(el: &ElementChart) -> Result<Option<Vec<u8>>> {
    render_line_like(el, false)
}

fn render_area(el: &ElementChart) -> Result<Option<Vec<u8>>> {
    render_line_like(el, true)
}

fn render_line_like(el: &ElementChart, filled: bool) -> Result<Option<Vec<u8>>> {
    if el.data.categories.is_empty() || el.data.series.is_empty() {
        return Ok(None);
    }

    // Reshape to long format for multi-series
    let mut categories: Vec<String> = Vec::new();
    let mut series_names: Vec<String> = Vec::new();
    let mut values: Vec<f64> = Vec::new();

    for series in &el.data.series {
        for (ci, val) in series.values.iter().enumerate() {
            if let Some(cat) = el.data.categories.get(ci) {
                categories.push(cat.clone());
                series_names.push(series.name.clone());
                values.push(*val);
            }
        }
    }

    let ds = Dataset::new()
        .with_column("category", categories)?
        .with_column("series", series_names)?
        .with_column("value", values)?;

    let lc = if filled {
        let chart = Chart::build(ds)?.mark_area()?;
        let chart = chart.encode((
            charton::alt::x("category"),
            charton::alt::y("value"),
            charton::alt::color("series"),
        ))?;
        apply_labels(chart.into(), el)
    } else {
        let chart = Chart::build(ds)?.mark_line()?;
        let chart = chart.encode((
            charton::alt::x("category"),
            charton::alt::y("value"),
            charton::alt::color("series"),
        ))?;
        apply_labels(chart.into(), el)
    };

    let png = lc.to_png()?;
    Ok(Some(png))
}

fn render_bar(el: &ElementChart) -> Result<Option<Vec<u8>>> {
    render_bar_column_like(el)
}

fn render_column(el: &ElementChart) -> Result<Option<Vec<u8>>> {
    render_bar_column_like(el)
}

fn render_bar_column_like(el: &ElementChart) -> Result<Option<Vec<u8>>> {
    if el.data.categories.is_empty() || el.data.series.is_empty() {
        return Ok(None);
    }

    // Reshape to long format for multi-series
    let mut categories: Vec<String> = Vec::new();
    let mut series_names: Vec<String> = Vec::new();
    let mut values: Vec<f64> = Vec::new();

    for series in &el.data.series {
        for (ci, val) in series.values.iter().enumerate() {
            if let Some(cat) = el.data.categories.get(ci) {
                categories.push(cat.clone());
                series_names.push(series.name.clone());
                values.push(*val);
            }
        }
    }

    let ds = Dataset::new()
        .with_column("category", categories)?
        .with_column("series", series_names)?
        .with_column("value", values)?;

    let chart = Chart::build(ds)?.mark_bar()?.encode((
        charton::alt::x("category"),
        charton::alt::y("value"),
        charton::alt::color("series"),
    ))?;

    let lc = apply_labels(chart.into(), el);
    let png = lc.to_png()?;
    Ok(Some(png))
}

fn render_scatter(el: &ElementChart) -> Result<Option<Vec<u8>>> {
    if el.data.categories.is_empty() || el.data.series.is_empty() {
        return Ok(None);
    }

    let series = &el.data.series[0];

    let ds = Dataset::new()
        .with_column("x", el.data.categories.iter().map(|s| s.as_str()).collect::<Vec<_>>())?
        .with_column("y", series.values.clone())?;

    let chart = Chart::build(ds)?.mark_point()?.encode((
        charton::alt::x("x"),
        charton::alt::y("y"),
    ))?;

    let lc = apply_labels(chart.into(), el);
    let png = lc.to_png()?;
    Ok(Some(png))
}

fn render_pie(el: &ElementChart) -> Result<Option<Vec<u8>>> {
    render_pie_like(el, None)
}

fn render_ring(el: &ElementChart) -> Result<Option<Vec<u8>>> {
    render_pie_like(el, Some(0.5))
}

fn render_pie_like(el: &ElementChart, inner_radius: Option<f64>) -> Result<Option<Vec<u8>>> {
    if el.data.categories.is_empty() || el.data.series.is_empty() {
        return Ok(None);
    }

    let series = &el.data.series[0];
    let ds = Dataset::new()
        .with_column("category", el.data.categories.clone())?
        .with_column("value", series.values.clone())?;

    let chart = Chart::build(ds)?.mark_bar()?.encode((
        charton::alt::x(""),
        charton::alt::y("value"),
        charton::alt::color("category"),
    ))?;

    let mut lc: charton::core::composite::LayeredChart = chart.into();
    lc = lc.with_coord(CoordSystem::Polar);
    if let Some(r) = inner_radius {
        lc = lc.with_inner_radius(r);
    }
    let lc = apply_labels(lc, el);
    let png = lc.to_png()?;
    Ok(Some(png))
}
