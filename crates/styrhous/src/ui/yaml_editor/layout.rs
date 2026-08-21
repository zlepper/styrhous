use super::*;

pub(super) fn yaml_editor_layout_job(
    ui: &egui::Ui,
    cache: &mut YamlEditorHighlightCache,
    theme: &CodeTheme,
    yaml: &str,
    search_query: &str,
    search_regex_mode: bool,
    active_match: Option<&Range<usize>>,
) -> egui::text::LayoutJob {
    let key = YamlEditorHighlightCacheKey::new(search_query, search_regex_mode, active_match);
    if let Some(job) = cache.layout_job(&key, yaml) {
        return (*job).clone();
    }

    let mut job = highlight(ui.ctx(), ui.style(), theme, yaml, "yaml");
    if let Ok(matches) = find_matches(yaml, search_query, search_regex_mode) {
        apply_search_highlights(&mut job, &matches, active_match);
    }
    cache.store(key, job.clone());
    job
}

pub(super) fn line_number_layout_job(
    line_count: usize,
    diagnostics: &[YamlDiagnostic],
) -> egui::text::LayoutJob {
    let diagnostic_lines: HashSet<_> = diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.line)
        .collect();
    let normal_format = egui::text::TextFormat::simple(typography::monospace(), gray::_500);
    let diagnostic_format = egui::text::TextFormat::simple(typography::monospace(), status::DANGER);
    let mut job = egui::text::LayoutJob::default();
    for line in 1..=line_count {
        let format = if diagnostic_lines.contains(&line) {
            &diagnostic_format
        } else {
            &normal_format
        };
        job.append(&line.to_string(), 0.0, format.clone());
        if line < line_count {
            job.append("\n", 0.0, format.clone());
        }
    }
    job
}

pub(super) fn show_diagnostic_underlines(
    ui: &mut egui::Ui,
    editor_id: u64,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    diagnostics: &[YamlDiagnostic],
) {
    for (diagnostic_index, diagnostic) in diagnostics.iter().enumerate() {
        let Some(range) = &diagnostic.range else {
            continue;
        };
        for (segment_index, rect) in diagnostic_rects(galley, galley_pos, range)
            .into_iter()
            .enumerate()
        {
            let response = ui.interact(
                rect.expand2(egui::vec2(0.0, 3.0)),
                egui::Id::new((
                    "yaml-editor-diagnostic",
                    editor_id,
                    diagnostic_index,
                    segment_index,
                )),
                egui::Sense::hover(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Label,
                    true,
                    format!("Validation error: {}", diagnostic.message),
                )
            });
            response.on_hover_text(&diagnostic.message);
            paint_diagnostic_squiggle(ui.painter(), rect);
        }
    }
}

pub(super) fn apply_search_highlights(
    layout_job: &mut egui::text::LayoutJob,
    matches: &[Range<usize>],
    active_match: Option<&Range<usize>>,
) {
    if matches.is_empty() {
        return;
    }
    let sections = std::mem::take(&mut layout_job.sections);
    for section in sections {
        let section_start = section.byte_range.start.0;
        let section_end = section.byte_range.end.0;
        let mut boundaries = vec![section_start, section_end];
        boundaries.extend(matches.iter().flat_map(|range| {
            [
                range.start.clamp(section_start, section_end),
                range.end.clamp(section_start, section_end),
            ]
        }));
        boundaries.sort_unstable();
        boundaries.dedup();

        for (index, pair) in boundaries.windows(2).enumerate() {
            let start = pair[0];
            let end = pair[1];
            if start == end {
                continue;
            }
            let mut format = section.format.clone();
            if let Some(matched) = matches
                .iter()
                .find(|range| range.start <= start && end <= range.end)
            {
                let is_active = active_match == Some(matched);
                format.background = if is_active {
                    search::ACTIVE_MATCH_BACKGROUND
                } else {
                    search::MATCH_BACKGROUND
                };
            }
            layout_job.sections.push(egui::text::LayoutSection {
                leading_space: if index == 0 {
                    section.leading_space
                } else {
                    0.0
                },
                byte_range: egui::text::ByteIndex(start)..egui::text::ByteIndex(end),
                format,
            });
        }
    }
}

pub(super) fn source_range_for_bytes(text: &str, range: Range<usize>) -> SourceRange {
    SourceRange {
        start: text[..range.start].chars().count(),
        end: text[..range.end].chars().count(),
    }
}

pub(super) fn diagnostic_rects(
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    range: &SourceRange,
) -> Vec<egui::Rect> {
    let mut character_index = 0;
    let mut rects = Vec::new();
    for row in &galley.rows {
        let row_character_count: usize = row.char_count_including_newline().into();
        let row_end = character_index + row_character_count;
        let first = range.start.max(character_index);
        let last = range.end.min(row_end);
        let row_text_length: usize = row.char_count_excluding_newline().into();
        if first < last && first - character_index < row_text_length {
            let start_column = first - character_index;
            let end_column = (last - character_index).min(row_text_length);
            let end_column = end_column.max((start_column + 1).min(row_text_length));
            let row_rect = row.rect().translate(galley_pos.to_vec2());
            rects.push(egui::Rect::from_min_max(
                egui::pos2(
                    row_rect.left() + row.x_offset(egui::text::CharIndex(start_column)),
                    row_rect.top(),
                ),
                egui::pos2(
                    row_rect.left() + row.x_offset(egui::text::CharIndex(end_column)),
                    row_rect.bottom(),
                ),
            ));
        }
        character_index = row_end;
    }
    rects
}

pub(super) fn paint_diagnostic_squiggle(painter: &egui::Painter, rect: egui::Rect) {
    let wavelength = 4.0;
    let amplitude = 1.5;
    let baseline = rect.bottom() - 2.0;
    let steps = (rect.width() / (wavelength / 2.0)).ceil() as usize;
    let points = (0..=steps)
        .map(|step| {
            let x = (rect.left() + step as f32 * (wavelength / 2.0)).min(rect.right());
            let y = baseline + if step % 2 == 0 { -amplitude } else { amplitude };
            egui::pos2(x, y)
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.5, status::DANGER),
    ));
}

pub(super) fn yaml_editor_text_edit_id(editor_id: u64) -> egui::Id {
    egui::Id::new(("yaml-editor-text", editor_id))
}
