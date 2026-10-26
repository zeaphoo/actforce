use crate::types::*;

/// Convert an ElementChart to an ECharts option JSON object.
pub fn build_echarts_option(el: &ElementChart) -> serde_json::Value {
    let cfg = el.config.as_ref();
    let title_text = el.title.as_deref().unwrap_or("");

    // If any series specifies its own chart_type, build a mixed chart
    let has_mixed = matches!(el.chart_type, ChartType::Bar | ChartType::Column | ChartType::Line | ChartType::Area)
        && el.data.series.iter().any(|s| s.chart_type.is_some());
    if has_mixed {
        return build_mixed_option(el, cfg, title_text);
    }

    match el.chart_type {
        ChartType::Bar => build_bar_option(el, cfg, title_text, false),
        ChartType::Column => build_bar_option(el, cfg, title_text, false),
        ChartType::Line => build_line_option(el, cfg, title_text, false),
        ChartType::Area => build_line_option(el, cfg, title_text, true),
        ChartType::Pie => build_pie_option(el, cfg, title_text, false),
        ChartType::Ring => build_pie_option(el, cfg, title_text, true),
        ChartType::Scatter => build_scatter_option(el, cfg, title_text),
        ChartType::Radar => build_radar_option(el, cfg, title_text),
    }
}

fn base_option(title_text: &str, cfg: Option<&ChartConfig>) -> serde_json::Value {
    let mut opt = serde_json::json!({
        "title": { "text": title_text, "left": "center", "textStyle": { "fontSize": 14 } },
        "tooltip": {},
        "legend": { "show": true, "bottom": 0 },
    });

    if let Some(cfg) = cfg {
        if let Some(show_legend) = cfg.show_legend {
            opt["legend"]["show"] = serde_json::Value::Bool(show_legend);
        }
        if let Some(unit) = &cfg.unit {
            opt["tooltip"] = serde_json::json!({
                "trigger": "axis",
                "valueFormatter": format!("(value) => value + ' {}'", unit),
            });
        }
    }

    opt
}

fn build_bar_option(
    el: &ElementChart,
    cfg: Option<&ChartConfig>,
    title_text: &str,
    horizontal: bool,
) -> serde_json::Value {
    let mut opt = base_option(title_text, cfg);

    opt["tooltip"]["trigger"] = serde_json::Value::String("axis".to_string());

    if horizontal {
        opt["yAxis"] = serde_json::json!({
            "type": "category",
            "data": el.data.categories,
            "axisLabel": { "interval": 0 },
        });
        opt["xAxis"] = serde_json::json!({ "type": "value" });
        if let Some(cfg) = cfg {
            if let Some(name) = &cfg.y_axis_name {
                opt["yAxis"]["name"] = serde_json::Value::String(name.clone());
            }
            if let Some(name) = &cfg.x_axis_name {
                opt["xAxis"]["name"] = serde_json::Value::String(name.clone());
            }
        }
    } else {
        opt["xAxis"] = serde_json::json!({
            "type": "category",
            "data": el.data.categories,
            "axisLabel": { "interval": 0, "rotate": el.data.categories.len().max(1) as f64 > 8.0_f64 },
        });
        opt["yAxis"] = serde_json::json!({ "type": "value" });
        if let Some(cfg) = cfg {
            if let Some(name) = &cfg.x_axis_name {
                opt["xAxis"]["name"] = serde_json::Value::String(name.clone());
            }
            if let Some(name) = &cfg.y_axis_name {
                opt["yAxis"]["name"] = serde_json::Value::String(name.clone());
            }
        }
    }

    opt["grid"] = serde_json::json!({ "left": "3%", "right": "4%", "bottom": "10%", "containLabel": true });

    let series: Vec<serde_json::Value> = el
        .data
        .series
        .iter()
        .map(|s| {
            let mut s_opt = serde_json::json!({
                "name": s.name,
                "type": "bar",
                "data": s.values,
                "barMaxWidth": "40%",
            });
            if let Some(cfg) = cfg {
                if let Some(label) = cfg.show_label {
                    s_opt["label"] = serde_json::json!({ "show": label, "position": if horizontal { "right" } else { "top" } });
                }
                if let Some(stack) = cfg.stack {
                    if stack {
                        s_opt["stack"] = serde_json::Value::String("total".to_string());
                    }
                }
            }
            s_opt
        })
        .collect();
    opt["series"] = serde_json::Value::Array(series);

    opt
}

fn build_line_option(
    el: &ElementChart,
    cfg: Option<&ChartConfig>,
    title_text: &str,
    filled: bool,
) -> serde_json::Value {
    let mut opt = base_option(title_text, cfg);

    opt["tooltip"]["trigger"] = serde_json::Value::String("axis".to_string());
    opt["xAxis"] = serde_json::json!({
        "type": "category",
        "data": el.data.categories,
        "boundaryGap": false,
    });
    opt["yAxis"] = serde_json::json!({ "type": "value" });
    opt["grid"] = serde_json::json!({ "left": "3%", "right": "4%", "bottom": "10%", "containLabel": true });

    if let Some(cfg) = cfg {
        if let Some(name) = &cfg.x_axis_name {
            opt["xAxis"]["name"] = serde_json::Value::String(name.clone());
        }
        if let Some(name) = &cfg.y_axis_name {
            opt["yAxis"]["name"] = serde_json::Value::String(name.clone());
        }
    }

    let series: Vec<serde_json::Value> = el
        .data
        .series
        .iter()
        .map(|s| {
            let mut s_opt = serde_json::json!({
                "name": s.name,
                "type": "line",
                "data": s.values,
            });
            if filled {
                s_opt["areaStyle"] = serde_json::json!({});
            }
            if s.smooth.unwrap_or(false) {
                s_opt["smooth"] = serde_json::Value::Bool(true);
            }
            if let Some(cfg) = cfg {
                if let Some(label) = cfg.show_label {
                    s_opt["label"] = serde_json::json!({ "show": label });
                }
                if let Some(stack) = cfg.stack {
                    if stack {
                        s_opt["stack"] = serde_json::Value::String("total".to_string());
                    }
                }
            }
            s_opt
        })
        .collect();
    opt["series"] = serde_json::Value::Array(series);

    opt
}

fn build_pie_option(
    el: &ElementChart,
    cfg: Option<&ChartConfig>,
    title_text: &str,
    is_ring: bool,
) -> serde_json::Value {
    let mut opt = serde_json::json!({
        "title": { "text": title_text, "left": "center", "textStyle": { "fontSize": 14 } },
        "tooltip": { "trigger": "item" },
        "legend": { "show": true, "bottom": 0 },
    });

    if let Some(cfg) = cfg {
        if let Some(show_legend) = cfg.show_legend {
            opt["legend"]["show"] = serde_json::Value::Bool(show_legend);
        }
    }

    // Pie data: pair categories with first series values
    let pie_data: Vec<serde_json::Value> = el
        .data
        .categories
        .iter()
        .zip(el.data.series.first().map(|s| &s.values).unwrap_or(&vec![]).iter())
        .map(|(cat, val)| {
            serde_json::json!({ "name": cat, "value": val })
        })
        .collect();

    let mut series_opt = serde_json::json!({
        "type": "pie",
        "data": pie_data,
        "center": ["50%", "50%"],
    });

    series_opt["radius"] = if is_ring {
        serde_json::json!(["35%", "55%"])
    } else {
        serde_json::json!("50%")
    };

    if let Some(cfg) = cfg {
        if let Some(label) = cfg.show_label {
            series_opt["label"] = serde_json::json!({ "show": label });
        }
    }

    opt["series"] = serde_json::json!([series_opt]);

    opt
}

fn build_mixed_option(
    el: &ElementChart,
    cfg: Option<&ChartConfig>,
    title_text: &str,
) -> serde_json::Value {
    let mut opt = base_option(title_text, cfg);
    opt["tooltip"]["trigger"] = serde_json::Value::String("axis".to_string());

    // xAxis: category
    opt["xAxis"] = serde_json::json!({
        "type": "category",
        "data": el.data.categories,
        "axisLabel": { "interval": 0 },
    });
    if let Some(cfg) = cfg {
        if let Some(name) = &cfg.x_axis_name {
            opt["xAxis"]["name"] = serde_json::Value::String(name.clone());
        }
    }

    // Collect unique yAxis definitions
    let mut y_axes: Vec<serde_json::Value> = Vec::new();
    let mut axis_index_map: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();

    for s in &el.data.series {
        let idx = s.y_axis_index.unwrap_or(0);
        if !axis_index_map.contains_key(&idx) {
            axis_index_map.insert(idx, y_axes.len());
            let mut y_axis = serde_json::json!({ "type": "value" });
            if let Some(name) = &s.y_axis_name {
                y_axis["name"] = serde_json::Value::String(name.clone());
            }
            y_axes.push(y_axis);
        }
    }

    // If only one yAxis, don't wrap in array for cleaner output
    if y_axes.len() == 1 {
        opt["yAxis"] = y_axes.into_iter().next().unwrap();
    } else {
        opt["yAxis"] = serde_json::Value::Array(y_axes);
    }

    opt["grid"] = serde_json::json!({ "left": "3%", "right": "4%", "bottom": "10%", "containLabel": true });

    // Build series
    let series: Vec<serde_json::Value> = el.data.series.iter().map(|s| {
        let s_type = match s.chart_type.as_ref().unwrap_or(&el.chart_type) {
            ChartType::Bar | ChartType::Column => "bar",
            ChartType::Line | ChartType::Area => "line",
            _ => "bar",
        };
        let mut s_opt = serde_json::json!({
            "name": s.name,
            "type": s_type,
            "data": s.values,
        });
        if s.chart_type.as_ref().map_or(false, |t| matches!(t, ChartType::Area)) {
            s_opt["areaStyle"] = serde_json::json!({});
        }
        if s_type == "bar" {
            s_opt["barMaxWidth"] = serde_json::json!("40%");
        }
        if s.smooth.unwrap_or(false) {
            s_opt["smooth"] = serde_json::Value::Bool(true);
        }
        if s.y_axis_index.unwrap_or(0) != 0 || axis_index_map.len() > 1 {
            s_opt["yAxisIndex"] = serde_json::json!(*axis_index_map.get(&s.y_axis_index.unwrap_or(0)).unwrap_or(&0));
        }
        if let Some(label) = cfg.and_then(|c| c.show_label) {
            s_opt["label"] = serde_json::json!({ "show": label, "position": "top" });
        }
        s_opt
    }).collect();
    opt["series"] = serde_json::Value::Array(series);

    opt
}

fn build_scatter_option(
    el: &ElementChart,
    cfg: Option<&ChartConfig>,
    title_text: &str,
) -> serde_json::Value {
    let mut opt = base_option(title_text, cfg);

    opt["tooltip"]["trigger"] = serde_json::Value::String("item".to_string());
    opt["xAxis"] = serde_json::json!({ "type": "value" });
    opt["yAxis"] = serde_json::json!({ "type": "value" });
    opt["grid"] = serde_json::json!({ "left": "3%", "right": "4%", "bottom": "10%", "containLabel": true });

    if let Some(cfg) = cfg {
        if let Some(name) = &cfg.x_axis_name {
            opt["xAxis"]["name"] = serde_json::Value::String(name.clone());
        }
        if let Some(name) = &cfg.y_axis_name {
            opt["yAxis"]["name"] = serde_json::Value::String(name.clone());
        }
    }

    // Scatter: one series per ChartSeries, data as [[cat, val], ...]
    // categories are used as x values (parsed as f64 if possible)
    let series: Vec<serde_json::Value> = el
        .data
        .series
        .iter()
        .map(|s| {
            let data: Vec<serde_json::Value> = el
                .data
                .categories
                .iter()
                .zip(s.values.iter())
                .map(|(cat, val)| {
                    let x: serde_json::Value = cat
                        .parse::<f64>()
                        .map(|n| serde_json::Value::Number(serde_json::Number::from_f64(n).unwrap_or_else(|| serde_json::Number::from(0))))
                        .unwrap_or_else(|_| {
                            serde_json::Value::String(cat.clone())
                        });
                    serde_json::json!([x, val])
                })
                .collect();

            let mut s_opt = serde_json::json!({
                "name": s.name,
                "type": "scatter",
                "data": data,
            });
            if let Some(cfg) = cfg {
                if let Some(label) = cfg.show_label {
                    s_opt["label"] = serde_json::json!({ "show": label });
                }
            }
            s_opt
        })
        .collect();
    opt["series"] = serde_json::Value::Array(series);

    opt
}

fn build_radar_option(
    el: &ElementChart,
    cfg: Option<&ChartConfig>,
    title_text: &str,
) -> serde_json::Value {
    let mut opt = base_option(title_text, cfg);

    // Calculate max value for radar indicator scale
    let max_val = el
        .data
        .series
        .iter()
        .flat_map(|s| s.values.iter())
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    let indicator: Vec<serde_json::Value> = el
        .data
        .categories
        .iter()
        .map(|cat| {
            serde_json::json!({ "name": cat, "max": max_val * 1.2 })
        })
        .collect();

    opt["radar"] = serde_json::json!({ "indicator": indicator });
    opt["tooltip"]["trigger"] = serde_json::Value::String("item".to_string());

    let series_data: Vec<serde_json::Value> = el
        .data
        .series
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "value": s.values,
            })
        })
        .collect();

    opt["series"] = serde_json::json!([{
        "type": "radar",
        "data": series_data,
    }]);

    opt
}
