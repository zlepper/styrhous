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

pub(super) fn ansi_text_format(
    style: Style,
    default: &egui::TextFormat,
    theme: &Theme,
) -> egui::TextFormat {
    let ansi_color = |color| ansi_color(color, &theme.ansi);
    let default_background = theme.code.surface;
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

// colour-guard: allow-begin (xterm 256 / truecolor: the 6×6×6 cube, the grey ramp and 24-bit
// escapes carry their colour in the log text itself)
pub(super) fn ansi_color(color: Color, palette: &AnsiRoles) -> egui::Color32 {
    match color {
        Color::Ansi(color) => ansi_palette_color(color, palette),
        Color::Ansi256(Ansi256Color(index)) => ansi_256_color(index, palette),
        Color::Rgb(RgbColor(red, green, blue)) => egui::Color32::from_rgb(red, green, blue),
    }
}

/// An xterm 256-colour index. Indices 0–15 are the theme's ANSI roles; the
/// 6×6×6 cube and the grey ramp are fixed by the xterm specification.
pub(super) fn ansi_256_color(index: u8, palette: &AnsiRoles) -> egui::Color32 {
    if index < 16 {
        return ansi_palette_color(ansi_color_from_index(index), palette);
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
// colour-guard: allow-end

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

pub(super) fn ansi_palette_color(color: AnsiColor, palette: &AnsiRoles) -> egui::Color32 {
    match color {
        AnsiColor::Black => palette.black,
        AnsiColor::Red => palette.red,
        AnsiColor::Green => palette.green,
        AnsiColor::Yellow => palette.yellow,
        AnsiColor::Blue => palette.blue,
        AnsiColor::Magenta => palette.magenta,
        AnsiColor::Cyan => palette.cyan,
        AnsiColor::White => palette.white,
        AnsiColor::BrightBlack => palette.bright_black,
        AnsiColor::BrightRed => palette.bright_red,
        AnsiColor::BrightGreen => palette.bright_green,
        AnsiColor::BrightYellow => palette.bright_yellow,
        AnsiColor::BrightBlue => palette.bright_blue,
        AnsiColor::BrightMagenta => palette.bright_magenta,
        AnsiColor::BrightCyan => palette.bright_cyan,
        AnsiColor::BrightWhite => palette.bright_white,
    }
}
