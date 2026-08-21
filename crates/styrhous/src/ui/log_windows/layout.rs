use super::*;

pub(super) fn byte_offset_at_response_x(
    text: &str,
    pointer_x: f32,
    text_left: f32,
    text_start_x: f32,
    character_width: f32,
) -> usize {
    byte_offset_at_x(text, pointer_x - text_left + text_start_x, character_width)
}

#[derive(Debug, Clone)]
pub(super) struct VisibleTextFragment {
    pub(super) byte_range: std::ops::Range<usize>,
    pub(super) start_x: f32,
}

pub(super) fn visible_text_fragment(
    text: &str,
    horizontal_offset: f32,
    viewport_width: f32,
    character_width: f32,
) -> VisibleTextFragment {
    let overscan_columns = (HORIZONTAL_OVERSCAN_POINTS / character_width).ceil() as usize;
    let first_column = (horizontal_offset / character_width).floor().max(0.0) as usize;
    let visible_columns = (viewport_width / character_width).ceil() as usize;
    let start_column = first_column.saturating_sub(overscan_columns);
    let end_column = first_column
        .saturating_add(visible_columns)
        .saturating_add(overscan_columns);
    let byte_range = character_column_range(text, start_column, end_column);
    VisibleTextFragment {
        byte_range,
        start_x: start_column as f32 * character_width,
    }
}

pub(super) fn character_column_range(
    text: &str,
    start_column: usize,
    end_column: usize,
) -> std::ops::Range<usize> {
    if text.is_ascii() {
        return start_column.min(text.len())..end_column.min(text.len());
    }

    let start = text
        .char_indices()
        .nth(start_column)
        .map_or(text.len(), |(byte_index, _)| byte_index);
    let end = text
        .char_indices()
        .nth(end_column)
        .map_or(text.len(), |(byte_index, _)| byte_index);
    start..end
}

pub(super) fn log_line_prefix(
    line_index: usize,
    timestamp: Option<&str>,
    display_options: LogDisplayOptions,
) -> String {
    let mut prefix = String::new();
    if display_options.show_line_numbers {
        prefix.push_str(&format!("{line_index:>6}  "));
    }
    if display_options.show_timestamps
        && let Some(timestamp) = timestamp
    {
        prefix.push_str(timestamp);
        prefix.push_str("  ");
    }
    prefix
}

pub(super) fn show_loading_row(
    ui: &mut egui::Ui,
    row_height: f32,
    display_row: usize,
    display_row_is_line_index: bool,
    display_options: LogDisplayOptions,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            if display_row_is_line_index && display_options.show_line_numbers {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(log_line_prefix(display_row, None, display_options))
                            .font(egui::FontId::monospace(LOG_FONT_SIZE))
                            .color(egui::Color32::from_rgb(156, 163, 175)),
                    )
                    .selectable(false),
                );
            }
            ui.add(
                egui::Label::new(
                    egui::RichText::new("Loading…")
                        .font(egui::FontId::monospace(LOG_FONT_SIZE))
                        .color(gray::_500),
                )
                .selectable(false),
            );
        },
    );
}

pub(super) fn clipped_style_spans(
    style_spans: &[AnsiStyleSpan],
    byte_range: std::ops::Range<usize>,
) -> Vec<AnsiStyleSpan> {
    style_spans
        .iter()
        .filter_map(|span| {
            let start = span.range.0.max(byte_range.start);
            let end = span.range.1.min(byte_range.end);
            (start < end).then_some(AnsiStyleSpan {
                range: (start - byte_range.start, end - byte_range.start),
                style: span.style,
            })
        })
        .collect()
}

pub(super) fn clipped_ranges(
    ranges: &[(usize, usize)],
    byte_range: std::ops::Range<usize>,
) -> Vec<(usize, usize)> {
    ranges
        .iter()
        .filter_map(|&(range_start, range_end)| {
            let start = range_start.max(byte_range.start);
            let end = range_end.min(byte_range.end);
            (start < end).then_some((start - byte_range.start, end - byte_range.start))
        })
        .collect()
}

pub(super) fn log_line_layout_job(
    line_index: usize,
    timestamp: Option<&str>,
    line: &str,
    style_spans: &[AnsiStyleSpan],
    ranges: &[(usize, usize)],
    display_options: LogDisplayOptions,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob {
        wrap: egui::text::TextWrapping::no_max_width(),
        ..Default::default()
    };
    let number = egui::TextFormat {
        font_id: egui::FontId::monospace(LOG_FONT_SIZE),
        color: egui::Color32::from_rgb(156, 163, 175),
        ..Default::default()
    };
    if display_options.show_line_numbers {
        job.append(&format!("{line_index:>6}  "), 0.0, number.clone());
    }
    if display_options.show_timestamps
        && let Some(timestamp) = timestamp
    {
        job.append(&format!("{timestamp}  "), 0.0, number);
    }
    append_log_line_text(&mut job, line, style_spans, ranges, display_options);
    job
}

pub(super) fn log_line_text_layout_job(
    line: &str,
    style_spans: Vec<AnsiStyleSpan>,
    ranges: Vec<(usize, usize)>,
    display_options: LogDisplayOptions,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob {
        wrap: egui::text::TextWrapping::no_max_width(),
        ..Default::default()
    };
    append_log_line_text(&mut job, line, &style_spans, &ranges, display_options);
    job
}

pub(super) fn append_log_line_text(
    job: &mut egui::text::LayoutJob,
    line: &str,
    style_spans: &[AnsiStyleSpan],
    ranges: &[(usize, usize)],
    display_options: LogDisplayOptions,
) {
    let text = egui::TextFormat {
        font_id: egui::FontId::monospace(LOG_FONT_SIZE),
        color: egui::Color32::from_rgb(229, 231, 235),
        ..Default::default()
    };
    let mut boundaries = Vec::with_capacity(2 + style_spans.len() * 2 + ranges.len() * 2);
    boundaries.extend([0, line.len()]);
    boundaries.extend(
        style_spans
            .iter()
            .flat_map(|span| [span.range.0, span.range.1]),
    );
    boundaries.extend(ranges.iter().flat_map(|&(start, end)| [start, end]));
    boundaries.sort_unstable();
    boundaries.dedup();
    for boundary_pair in boundaries.windows(2) {
        let start = boundary_pair[0];
        let end = boundary_pair[1];
        if start == end {
            continue;
        }
        let style = display_options
            .render_ansi
            .then(|| {
                style_spans
                    .iter()
                    .find(|span| span.range.0 <= start && start < span.range.1)
                    .map(|span| span.style)
            })
            .flatten();
        let mut format = style.map_or_else(|| text.clone(), |style| ansi_text_format(style, &text));
        if ranges
            .iter()
            .any(|&(match_start, match_end)| match_start <= start && end <= match_end)
        {
            if style.is_none() {
                format.color = egui::Color32::from_rgb(254, 243, 199);
            }
            format.background = search::MATCH_BACKGROUND;
        }
        job.append(&line[start..end], 0.0, format);
    }
}

pub(super) fn ansi_text_format(style: Style, default: &egui::TextFormat) -> egui::TextFormat {
    let default_background = egui::Color32::from_rgb(10, 10, 11);
    let mut format = default.clone();
    let foreground = style.get_fg_color().map(ansi_color).unwrap_or(format.color);
    let background = style
        .get_bg_color()
        .map(ansi_color)
        .unwrap_or(default_background);
    if style.get_effects().contains(Effects::INVERT) {
        format.color = background;
        format.background = foreground;
    } else {
        format.color = foreground;
        if style.get_bg_color().is_some() {
            format.background = background;
        }
    }
    let effects = style.get_effects();
    if effects.contains(Effects::DIMMED) {
        format.color = format.color.gamma_multiply(0.65);
    }
    if effects.contains(Effects::HIDDEN) {
        format.color = egui::Color32::TRANSPARENT;
    }
    format.italics = effects.contains(Effects::ITALIC);
    if effects.contains(Effects::UNDERLINE)
        || effects.contains(Effects::DOUBLE_UNDERLINE)
        || effects.contains(Effects::CURLY_UNDERLINE)
        || effects.contains(Effects::DOTTED_UNDERLINE)
        || effects.contains(Effects::DASHED_UNDERLINE)
    {
        format.underline = egui::Stroke::new(
            1.0,
            style
                .get_underline_color()
                .map(ansi_color)
                .unwrap_or(format.color),
        );
    }
    if effects.contains(Effects::STRIKETHROUGH) {
        format.strikethrough = egui::Stroke::new(1.0, format.color);
    }
    format
}

pub(super) fn ansi_color(color: Color) -> egui::Color32 {
    match color {
        Color::Ansi(color) => ansi_palette_color(color),
        Color::Ansi256(Ansi256Color(index)) => ansi_256_color(index),
        Color::Rgb(RgbColor(red, green, blue)) => egui::Color32::from_rgb(red, green, blue),
    }
}

pub(super) fn ansi_256_color(index: u8) -> egui::Color32 {
    if index < 16 {
        return ansi_palette_color(ansi_color_from_index(index));
    }
    if index >= 232 {
        let gray = 8 + (index - 232) * 10;
        return egui::Color32::from_gray(gray);
    }
    let color_index = index - 16;
    let component = |value| if value == 0 { 0 } else { 55 + value * 40 };
    egui::Color32::from_rgb(
        component(color_index / 36),
        component((color_index / 6) % 6),
        component(color_index % 6),
    )
}

pub(super) fn ansi_color_from_index(index: u8) -> AnsiColor {
    match index {
        0 => AnsiColor::Black,
        1 => AnsiColor::Red,
        2 => AnsiColor::Green,
        3 => AnsiColor::Yellow,
        4 => AnsiColor::Blue,
        5 => AnsiColor::Magenta,
        6 => AnsiColor::Cyan,
        7 => AnsiColor::White,
        8 => AnsiColor::BrightBlack,
        9 => AnsiColor::BrightRed,
        10 => AnsiColor::BrightGreen,
        11 => AnsiColor::BrightYellow,
        12 => AnsiColor::BrightBlue,
        13 => AnsiColor::BrightMagenta,
        14 => AnsiColor::BrightCyan,
        15 => AnsiColor::BrightWhite,
        _ => unreachable!("ANSI 16-color palette index must be below 16"),
    }
}

pub(super) fn ansi_palette_color(color: AnsiColor) -> egui::Color32 {
    match color {
        AnsiColor::Black => egui::Color32::from_rgb(0, 0, 0),
        AnsiColor::Red => egui::Color32::from_rgb(239, 68, 68),
        AnsiColor::Green => egui::Color32::from_rgb(34, 197, 94),
        AnsiColor::Yellow => egui::Color32::from_rgb(234, 179, 8),
        AnsiColor::Blue => egui::Color32::from_rgb(59, 130, 246),
        AnsiColor::Magenta => egui::Color32::from_rgb(217, 70, 239),
        AnsiColor::Cyan => egui::Color32::from_rgb(6, 182, 212),
        AnsiColor::White => egui::Color32::from_rgb(229, 231, 235),
        AnsiColor::BrightBlack => egui::Color32::from_rgb(107, 114, 128),
        AnsiColor::BrightRed => egui::Color32::from_rgb(248, 113, 113),
        AnsiColor::BrightGreen => egui::Color32::from_rgb(74, 222, 128),
        AnsiColor::BrightYellow => egui::Color32::from_rgb(250, 204, 21),
        AnsiColor::BrightBlue => egui::Color32::from_rgb(96, 165, 250),
        AnsiColor::BrightMagenta => egui::Color32::from_rgb(232, 121, 249),
        AnsiColor::BrightCyan => egui::Color32::from_rgb(34, 211, 238),
        AnsiColor::BrightWhite => egui::Color32::from_rgb(255, 255, 255),
    }
}
