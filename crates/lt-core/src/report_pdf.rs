//! Moteur de mise en page PDF (A4) pour les rapports : bandeau d'accent,
//! badges, cartes KPI, tableaux paginés, graphiques et pied de page.
//!
//! Les polices iA Writer Quattro S (Unicode) sont embarquées ; à défaut, on
//! retombe sur Helvetica intégrée (Latin-1 uniquement).

use printpdf::{
    BuiltinFont, Color, FontId, Line, LinePoint, Mm, Op, PaintMode, ParsedFont, PdfDocument,
    PdfFontHandle, PdfPage, PdfSaveOptions, Point, Polygon, PolygonRing, Pt, Rgb, TextItem,
    WindingOrder,
};

static FONT_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/iAWriterQuattroS-Regular.ttf");
static FONT_BOLD: &[u8] = include_bytes!("../../../assets/fonts/iAWriterQuattroS-Bold.ttf");

pub(crate) type Rgb3 = (f32, f32, f32);

pub(crate) const ACCENT: Rgb3 = (0.357, 0.310, 0.878);
const TEXT: Rgb3 = (0.114, 0.137, 0.188);
const MUTED: Rgb3 = (0.404, 0.439, 0.522);
const HEAD_TEXT: Rgb3 = (0.30, 0.34, 0.42);
const CARD_BG: Rgb3 = (0.945, 0.953, 0.965);
const HEAD_BG: Rgb3 = (0.898, 0.914, 0.941);
const ZEBRA_BG: Rgb3 = (0.972, 0.976, 0.984);
const GRID: Rgb3 = (0.835, 0.858, 0.890);
const OK: Rgb3 = (0.122, 0.616, 0.333);
const OK_BG: Rgb3 = (0.898, 0.965, 0.925);
const KO: Rgb3 = (0.851, 0.220, 0.220);
const KO_BG: Rgb3 = (0.992, 0.910, 0.910);
const WARN: Rgb3 = (0.718, 0.475, 0.122);
const WARN_BG: Rgb3 = (0.992, 0.953, 0.882);

const PT_TO_MM: f32 = 0.352_778;
const MM_TO_PT: f32 = 2.834_645;

const PAGE_W: f32 = 210.0;
const PAGE_H: f32 = 297.0;
const MARGIN: f32 = 15.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;
const BAR_H: f32 = 3.5;
/// Limite basse du contenu (réserve la zone du pied de page).
const BOTTOM: f32 = 20.0;

const KPI_COLS: usize = 4;
const KPI_GAP: f32 = 3.0;
const KPI_H: f32 = 16.0;

const CHART_GAP: f32 = 4.0;
const CHART_PANEL_W: f32 = (CONTENT_W - CHART_GAP) / 2.0;
const CHART_H: f32 = 47.0;
/// Dimensions de la zone de tracé d'un graphique (mm), pour `scale_points`.
pub(crate) const CHART_PLOT_W: f32 = CHART_PANEL_W - 6.0;
pub(crate) const CHART_PLOT_H: f32 = 28.0;
pub(crate) const CHART_PAD_Y: f32 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tone {
    Neutral,
    Ok,
    Ko,
    Warn,
}

impl Tone {
    fn fg(self) -> Rgb3 {
        match self {
            Tone::Neutral => TEXT,
            Tone::Ok => OK,
            Tone::Ko => KO,
            Tone::Warn => WARN,
        }
    }

    fn bg(self) -> Rgb3 {
        match self {
            Tone::Neutral => CARD_BG,
            Tone::Ok => OK_BG,
            Tone::Ko => KO_BG,
            Tone::Warn => WARN_BG,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Right,
}

/// Cellule de tableau : texte simple ou pastille colorée.
pub(crate) struct Cell {
    text: String,
    badge: Option<Tone>,
}

impl Cell {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            badge: None,
        }
    }

    pub fn badge(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            badge: Some(tone),
        }
    }
}

pub(crate) struct Kpi {
    pub label: String,
    pub value: String,
    pub tone: Tone,
}

pub(crate) struct ChartSpec {
    pub title: String,
    pub last_label: String,
    pub max_label: String,
    pub min_label: String,
    pub duration_label: String,
    pub no_data: String,
    pub color: Rgb3,
    /// Points en mm dans la zone de tracé (origine en haut à gauche, y vers le
    /// bas). Vide = pas de données.
    pub coords: Vec<(f64, f64)>,
}

pub(crate) struct PdfBuilder {
    doc: PdfDocument,
    regular: PdfFontHandle,
    bold: PdfFontHandle,
    regular_metrics: Option<ParsedFont>,
    bold_metrics: Option<ParsedFont>,
    pages: Vec<Vec<Op>>,
    ops: Vec<Op>,
    y: f32,
    footer_left: String,
}

fn rgb(c: Rgb3) -> Color {
    Color::Rgb(Rgb {
        r: c.0,
        g: c.1,
        b: c.2,
        icc_profile: None,
    })
}

fn pt_point(x: f32, y: f32) -> Point {
    Point {
        x: Pt(x * MM_TO_PT),
        y: Pt(y * MM_TO_PT),
    }
}

fn line_point(x: f32, y: f32) -> LinePoint {
    LinePoint {
        p: pt_point(x, y),
        bezier: false,
    }
}

fn tint(color: Rgb3, amount: f32) -> Rgb3 {
    let mix = |c: f32| 1.0 - (1.0 - c) * amount;
    (mix(color.0), mix(color.1), mix(color.2))
}

/// Points d'un rectangle aux coins arrondis (approximé par des segments).
fn round_rect_points(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<(f32, f32)> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r <= 0.01 {
        return vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    }
    const STEPS: usize = 5;
    let corners = [
        (x + w - r, y + r, -90.0_f32),
        (x + w - r, y + h - r, 0.0),
        (x + r, y + h - r, 90.0),
        (x + r, y + r, 180.0),
    ];
    let mut points = Vec::with_capacity(4 * (STEPS + 1));
    for (cx, cy, start) in corners {
        for step in 0..=STEPS {
            let angle = (start + 90.0 * step as f32 / STEPS as f32).to_radians();
            points.push((cx + r * angle.cos(), cy + r * angle.sin()));
        }
    }
    points
}

impl PdfBuilder {
    pub fn new(title: &str, footer_left: &str) -> Self {
        let mut doc = PdfDocument::new(title);
        let (regular, regular_metrics) =
            Self::load_font(&mut doc, FONT_REGULAR, BuiltinFont::Helvetica);
        let (bold, bold_metrics) = Self::load_font(&mut doc, FONT_BOLD, BuiltinFont::HelveticaBold);
        let mut builder = Self {
            doc,
            regular,
            bold,
            regular_metrics,
            bold_metrics,
            pages: Vec::new(),
            ops: Vec::new(),
            y: 0.0,
            footer_left: footer_left.to_string(),
        };
        builder.begin_page();
        builder
    }

    fn load_font(
        doc: &mut PdfDocument,
        bytes: &[u8],
        fallback: BuiltinFont,
    ) -> (PdfFontHandle, Option<ParsedFont>) {
        let mut warnings = Vec::new();
        match ParsedFont::from_bytes(bytes, 0, &mut warnings) {
            Some(font) => {
                let id: FontId = doc.add_font(&font);
                (PdfFontHandle::External(id), Some(font))
            }
            None => (PdfFontHandle::Builtin(fallback), None),
        }
    }

    // ---------------------------------------------------------------- texte

    fn unicode(&self) -> bool {
        self.regular_metrics.is_some() && self.bold_metrics.is_some()
    }

    /// Nettoie les caractères de contrôle ; retombe sur Latin-1 si on utilise
    /// les polices intégrées.
    fn sanitize(&self, input: &str) -> String {
        let unicode = self.unicode();
        input
            .chars()
            .map(|c| {
                if c.is_control() {
                    return ' ';
                }
                if unicode {
                    return c;
                }
                match c {
                    '–' | '—' => '-',
                    '…' => '.',
                    '’' | '‘' | '`' => '\'',
                    '“' | '”' | '«' | '»' => '"',
                    '·' => '-',
                    '→' => '>',
                    c if (c as u32) <= 0xFF => c,
                    _ => '?',
                }
            })
            .collect()
    }

    /// Largeur du texte en mm.
    pub fn text_width(&self, text: &str, size: f32, bold: bool) -> f32 {
        let font = if bold {
            &self.bold_metrics
        } else {
            &self.regular_metrics
        };
        let text = self.sanitize(text);
        let em = match font {
            Some(font) => {
                let units = (font.pdf_font_metrics.units_per_em as f32).max(1.0);
                text.chars()
                    .map(|c| {
                        font.lookup_glyph_index(c as u32)
                            .map(|glyph| font.get_horizontal_advance(glyph) as f32)
                            .unwrap_or(units * 0.5)
                            / units
                    })
                    .sum::<f32>()
            }
            None => text.chars().count() as f32 * if bold { 0.58 } else { 0.55 },
        };
        em * size * PT_TO_MM
    }

    /// Tronque avec « … » pour tenir dans `max_w` mm.
    pub fn fit_text(&self, text: &str, size: f32, bold: bool, max_w: f32) -> String {
        let text = self.sanitize(text);
        if self.text_width(&text, size, bold) <= max_w {
            return text;
        }
        let ellipsis = if self.unicode() { '…' } else { '.' };
        let mut chars: Vec<char> = text.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + &ellipsis.to_string();
            if self.text_width(&candidate, size, bold) <= max_w {
                return candidate;
            }
        }
        ellipsis.to_string()
    }

    /// Retour à la ligne par mots (césure dure si un mot dépasse), `max_lines`
    /// lignes au plus (la dernière est tronquée).
    pub fn wrap_text(
        &self,
        text: &str,
        size: f32,
        bold: bool,
        max_w: f32,
        max_lines: usize,
    ) -> Vec<String> {
        let text = self.sanitize(text);
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        for word in text.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if self.text_width(&candidate, size, bold) <= max_w {
                current = candidate;
                continue;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            // Mot seul trop long : découpe par caractères.
            let mut piece = String::new();
            for c in word.chars() {
                let next = format!("{piece}{c}");
                if self.text_width(&next, size, bold) > max_w && !piece.is_empty() {
                    lines.push(std::mem::take(&mut piece));
                    piece.push(c);
                } else {
                    piece = next;
                }
            }
            current = piece;
        }
        if !current.is_empty() {
            lines.push(current);
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        if lines.len() > max_lines {
            lines.truncate(max_lines);
            let last = lines.pop().unwrap_or_default();
            lines.push(self.fit_text(&format!("{last}…"), size, bold, max_w));
        }
        lines
    }

    fn text_at(&mut self, x: f32, y: f32, text: &str, size: f32, bold: bool, color: Rgb3) {
        let safe = self.sanitize(text);
        if safe.trim().is_empty() {
            return;
        }
        let font = if bold {
            self.bold.clone()
        } else {
            self.regular.clone()
        };
        self.ops.extend([
            Op::SaveGraphicsState,
            Op::StartTextSection,
            Op::SetTextCursor {
                pos: Point::new(Mm(x), Mm(y)),
            },
            Op::SetFont {
                font,
                size: Pt(size),
            },
            Op::SetLineHeight { lh: Pt(size) },
            Op::SetFillColor { col: rgb(color) },
            Op::ShowText {
                items: vec![TextItem::Text(safe)],
            },
            Op::EndTextSection,
            Op::RestoreGraphicsState,
        ]);
    }

    fn text_right(&mut self, right: f32, y: f32, text: &str, size: f32, bold: bool, color: Rgb3) {
        let width = self.text_width(text, size, bold);
        self.text_at(right - width, y, text, size, bold, color);
    }

    // ------------------------------------------------------------- formes

    fn polygon(&mut self, points: &[(f32, f32)], fill: Option<Rgb3>, stroke: Option<(Rgb3, f32)>) {
        if points.len() < 3 {
            return;
        }
        let mode = match (fill.is_some(), stroke.is_some()) {
            (true, true) => PaintMode::FillStroke,
            (true, false) => PaintMode::Fill,
            (false, true) => PaintMode::Stroke,
            (false, false) => return,
        };
        self.ops.push(Op::SaveGraphicsState);
        if let Some(color) = fill {
            self.ops.push(Op::SetFillColor { col: rgb(color) });
        }
        if let Some((color, thickness)) = stroke {
            self.ops.push(Op::SetOutlineColor { col: rgb(color) });
            self.ops.push(Op::SetOutlineThickness {
                pt: Pt(thickness),
            });
        }
        self.ops.push(Op::DrawPolygon {
            polygon: Polygon {
                rings: vec![PolygonRing {
                    points: points.iter().map(|(x, y)| line_point(*x, *y)).collect(),
                }],
                mode,
                winding_order: WindingOrder::NonZero,
            },
        });
        self.ops.push(Op::RestoreGraphicsState);
    }

    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb3) {
        self.polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], Some(color), None);
    }

    fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, fill: Rgb3, stroke: Option<(Rgb3, f32)>) {
        self.polygon(&round_rect_points(x, y, w, h, r), Some(fill), stroke);
    }

    fn polyline(&mut self, points: &[(f32, f32)], color: Rgb3, thickness: f32) {
        if points.len() < 2 {
            return;
        }
        self.ops.push(Op::SaveGraphicsState);
        self.ops.push(Op::SetOutlineColor { col: rgb(color) });
        self.ops.push(Op::SetOutlineThickness {
            pt: Pt(thickness),
        });
        self.ops.push(Op::DrawLine {
            line: Line {
                points: points.iter().map(|(x, y)| line_point(*x, *y)).collect(),
                is_closed: false,
            },
        });
        self.ops.push(Op::RestoreGraphicsState);
    }

    /// Pastille colorée dont le texte démarre à `x` (rien n'est dessiné à
    /// gauche). Renvoie sa largeur.
    fn pill(&mut self, x: f32, baseline: f32, text: &str, tone: Tone, size: f32) -> f32 {
        let text_w = self.text_width(text, size, true);
        let width = text_w + 5.0;
        let height = size * PT_TO_MM + 2.8;
        self.round_rect(
            x,
            baseline - 1.5,
            width,
            height,
            height / 2.0,
            tone.bg(),
            None,
        );
        self.text_at(x + 2.5, baseline, text, size, true, tone.fg());
        width
    }

    // -------------------------------------------------------------- pages

    fn begin_page(&mut self) {
        self.fill_rect(0.0, PAGE_H - BAR_H, PAGE_W, BAR_H, ACCENT);
        self.y = PAGE_H - BAR_H - 13.0;
    }

    pub fn new_page(&mut self) {
        let ops = std::mem::take(&mut self.ops);
        self.pages.push(ops);
        self.begin_page();
    }

    /// Passe à la page suivante si `needed` mm ne tiennent pas. Renvoie `true`
    /// si une nouvelle page a été ouverte.
    pub fn ensure_space(&mut self, needed: f32) -> bool {
        if self.y - needed < BOTTOM {
            self.new_page();
            true
        } else {
            false
        }
    }

    // ------------------------------------------------------------- blocs

    /// Titre, pastille de statut et ligne de méta.
    pub fn header(&mut self, title: &str, badge: &str, tone: Tone, meta: &str) {
        let badge_size = 8.0;
        let badge_w = self.text_width(badge, badge_size, true) + 5.0;
        let title_size = 17.0;
        let title_w = CONTENT_W - badge_w - 4.0;
        let title_lines = self.wrap_text(title, title_size, true, title_w, 2);
        let baseline = self.y;
        for (index, line) in title_lines.iter().enumerate() {
            self.text_at(
                MARGIN,
                baseline - index as f32 * 7.5,
                line,
                title_size,
                true,
                TEXT,
            );
        }
        self.pill(
            MARGIN + CONTENT_W - badge_w,
            baseline,
            badge,
            tone,
            badge_size,
        );
        self.y -= 7.5 * title_lines.len() as f32 + 0.5;

        for line in self.wrap_text(meta, 8.5, false, CONTENT_W, 3) {
            self.text_at(MARGIN, self.y, &line, 8.5, false, MUTED);
            self.y -= 4.4;
        }
        self.y -= 3.0;
    }

    /// Titre de section ; `keep_with` = hauteur de contenu à garder avec lui.
    pub fn heading(&mut self, text: &str, keep_with: f32) {
        self.ensure_space(14.0 + keep_with);
        self.y -= 4.0;
        self.fill_rect(MARGIN, self.y - 1.2, 1.5, 5.2, ACCENT);
        self.text_at(MARGIN + 4.0, self.y, text, 12.0, true, TEXT);
        self.y -= 7.0;
    }

    pub fn muted_line(&mut self, text: &str) {
        self.ensure_space(8.0);
        self.text_at(MARGIN, self.y, text, 9.0, false, MUTED);
        self.y -= 6.0;
    }

    /// Ligne « libellé  valeur » avec libellé gris en colonne fixe.
    pub fn key_value(&mut self, label: &str, value: &str) {
        self.ensure_space(6.0);
        self.text_at(MARGIN, self.y, label, 8.5, false, MUTED);
        let value = self.fit_text(value, 9.0, false, CONTENT_W - 32.0);
        self.text_at(MARGIN + 32.0, self.y, &value, 9.0, false, TEXT);
        self.y -= 5.2;
    }

    /// Grande pastille de verdict + phrase de synthèse.
    pub fn verdict(&mut self, label: &str, tone: Tone, sentence: &str) {
        let height = 30.0;
        self.ensure_space(height + 4.0);
        let top = self.y;
        self.round_rect(
            MARGIN,
            top - height,
            CONTENT_W,
            height,
            3.0,
            tone.bg(),
            Some((tone.fg(), 0.8)),
        );
        self.fill_rect(MARGIN + 0.4, top - height + 4.0, 2.0, height - 8.0, tone.fg());
        let label = self.fit_text(label, 28.0, true, CONTENT_W - 16.0);
        self.text_at(MARGIN + 9.0, top - 13.0, &label, 28.0, true, tone.fg());
        let sentence = self.fit_text(sentence, 10.5, false, CONTENT_W - 16.0);
        self.text_at(MARGIN + 9.0, top - 23.5, &sentence, 10.5, false, TEXT);
        self.y = top - height - 3.0;
    }

    pub fn kpi_grid(&mut self, kpis: &[Kpi]) {
        let card_w = (CONTENT_W - KPI_GAP * (KPI_COLS as f32 - 1.0)) / KPI_COLS as f32;
        for row in kpis.chunks(KPI_COLS) {
            self.ensure_space(KPI_H + KPI_GAP);
            let top = self.y;
            for (index, kpi) in row.iter().enumerate() {
                let x = MARGIN + index as f32 * (card_w + KPI_GAP);
                self.round_rect(x, top - KPI_H, card_w, KPI_H, 2.2, CARD_BG, None);
                let label = self.fit_text(&kpi.label, 7.5, false, card_w - 6.0);
                self.text_at(x + 3.0, top - 5.3, &label, 7.5, false, MUTED);
                let mut size = 14.0;
                while size > 8.0 && self.text_width(&kpi.value, size, true) > card_w - 6.0 {
                    size -= 0.5;
                }
                let value = self.fit_text(&kpi.value, size, true, card_w - 6.0);
                self.text_at(x + 3.0, top - 12.2, &value, size, true, kpi.tone.fg());
            }
            self.y = top - KPI_H - KPI_GAP;
        }
        self.y -= 1.0;
    }

    /// Tableau à en-tête gris, lignes zébrées et en-tête répété sur chaque page.
    /// `weights` = largeurs relatives des colonnes.
    pub fn table(&mut self, headers: &[String], rows: &[Vec<Cell>], weights: &[f32], aligns: &[Align]) {
        const HEAD_H: f32 = 7.0;
        const FONT: f32 = 8.5;
        const HEAD_FONT: f32 = 7.5;
        const LINE_H: f32 = 4.0;
        const PAD: f32 = 2.2;
        const MAX_LINES: usize = 3;

        let total: f32 = weights.iter().sum::<f32>().max(0.001);
        let widths: Vec<f32> = weights.iter().map(|w| w / total * CONTENT_W).collect();
        let mut xs = Vec::with_capacity(widths.len());
        let mut cursor = MARGIN;
        for width in &widths {
            xs.push(cursor);
            cursor += width;
        }
        let align_of = |index: usize| aligns.get(index).copied().unwrap_or(Align::Left);

        // Pré-calcul des lignes (retour à la ligne des cellules texte à gauche).
        let layouts: Vec<(Vec<Vec<String>>, f32)> = rows
            .iter()
            .map(|row| {
                let cells: Vec<Vec<String>> = row
                    .iter()
                    .enumerate()
                    .map(|(index, cell)| {
                        let max_w = (widths.get(index).copied().unwrap_or(20.0) - 2.0 * PAD).max(4.0);
                        if cell.badge.is_some() || align_of(index) == Align::Right {
                            vec![self.fit_text(&cell.text, FONT, false, max_w)]
                        } else {
                            self.wrap_text(&cell.text, FONT, false, max_w, MAX_LINES)
                        }
                    })
                    .collect();
                let lines = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
                (cells, (lines as f32 * LINE_H + 3.2).max(7.0))
            })
            .collect();

        let first_h = layouts.first().map(|(_, h)| *h).unwrap_or(0.0);
        self.ensure_space(HEAD_H + first_h + 2.0);
        self.table_header(headers, &xs, &widths, &align_of_vec(aligns, headers.len()), HEAD_H, HEAD_FONT, PAD);

        for (index, (row, (cells, row_h))) in rows.iter().zip(layouts.iter()).enumerate() {
            if self.y - row_h < BOTTOM {
                self.new_page();
                self.table_header(headers, &xs, &widths, &align_of_vec(aligns, headers.len()), HEAD_H, HEAD_FONT, PAD);
            }
            let top = self.y;
            if index % 2 == 1 {
                self.fill_rect(MARGIN, top - row_h, CONTENT_W, *row_h, ZEBRA_BG);
            }
            for (col, cell) in row.iter().enumerate() {
                let Some(lines) = cells.get(col) else { continue };
                let x = xs.get(col).copied().unwrap_or(MARGIN);
                let width = widths.get(col).copied().unwrap_or(20.0);
                for (line_index, line) in lines.iter().enumerate() {
                    let baseline = top - 4.8 - line_index as f32 * LINE_H;
                    let line_w = self.text_width(line, FONT, cell.badge.is_some());
                    match (cell.badge, align_of(col)) {
                        (Some(tone), Align::Right) => {
                            self.pill(x + width - PAD - line_w - 5.0, baseline, line, tone, 7.5);
                        }
                        (Some(tone), Align::Left) => {
                            self.pill(x + PAD, baseline, line, tone, 7.5);
                        }
                        (None, Align::Right) => {
                            self.text_right(x + width - PAD, baseline, line, FONT, false, TEXT);
                        }
                        (None, Align::Left) => {
                            self.text_at(x + PAD, baseline, line, FONT, false, TEXT);
                        }
                    }
                }
            }
            self.y = top - row_h;
        }
        self.polyline(&[(MARGIN, self.y), (MARGIN + CONTENT_W, self.y)], GRID, 0.4);
        self.y -= 4.0;
    }

    #[allow(clippy::too_many_arguments)]
    fn table_header(
        &mut self,
        headers: &[String],
        xs: &[f32],
        widths: &[f32],
        aligns: &[Align],
        height: f32,
        font: f32,
        pad: f32,
    ) {
        let top = self.y;
        self.round_rect(MARGIN, top - height, CONTENT_W, height, 1.2, HEAD_BG, None);
        for (index, header) in headers.iter().enumerate() {
            let x = xs.get(index).copied().unwrap_or(MARGIN);
            let width = widths.get(index).copied().unwrap_or(20.0);
            let text = self.fit_text(header, font, true, (width - 2.0 * pad).max(4.0));
            let baseline = top - 4.7;
            match aligns.get(index).copied().unwrap_or(Align::Left) {
                Align::Left => self.text_at(x + pad, baseline, &text, font, true, HEAD_TEXT),
                Align::Right => self.text_right(x + width - pad, baseline, &text, font, true, HEAD_TEXT),
            }
        }
        self.y = top - height;
    }

    /// Grille de graphiques, 2 par ligne.
    pub fn charts_grid(&mut self, charts: &[ChartSpec]) {
        for row in charts.chunks(2) {
            self.ensure_space(CHART_H + CHART_GAP);
            let top = self.y;
            for (index, chart) in row.iter().enumerate() {
                let x = MARGIN + index as f32 * (CHART_PANEL_W + CHART_GAP);
                self.chart_panel(x, top, chart);
            }
            self.y = top - CHART_H - CHART_GAP;
        }
    }

    fn chart_panel(&mut self, x: f32, top: f32, chart: &ChartSpec) {
        self.round_rect(x, top - CHART_H, CHART_PANEL_W, CHART_H, 2.2, CARD_BG, None);
        let title = self.fit_text(&chart.title, 8.0, true, CHART_PANEL_W * 0.55);
        self.text_at(x + 3.0, top - 5.5, &title, 8.0, true, MUTED);

        let ox = x + 3.0;
        let oy = top - 10.0;
        let inner_h = CHART_PLOT_H - 2.0 * CHART_PAD_Y;
        let grid_y = |i: usize| oy - CHART_PAD_Y - inner_h * i as f32 / 2.0;

        if chart.coords.is_empty() {
            let message = self.fit_text(&chart.no_data, 8.5, false, CHART_PLOT_W);
            let width = self.text_width(&message, 8.5, false);
            self.text_at(
                ox + (CHART_PLOT_W - width) / 2.0,
                oy - CHART_PLOT_H / 2.0,
                &message,
                8.5,
                false,
                MUTED,
            );
            return;
        }

        let value = self.fit_text(&chart.last_label, 10.0, true, CHART_PANEL_W * 0.45);
        self.text_right(x + CHART_PANEL_W - 3.0, top - 5.5, &value, 10.0, true, TEXT);

        for i in 0..3 {
            let gy = grid_y(i);
            self.polyline(&[(ox, gy), (ox + CHART_PLOT_W, gy)], GRID, 0.35);
        }

        let coords: Vec<(f32, f32)> = chart
            .coords
            .iter()
            .map(|(cx, cy)| (ox + *cx as f32, oy - *cy as f32))
            .collect();
        let base = grid_y(2);
        if coords.len() >= 2 {
            let mut area = Vec::with_capacity(coords.len() + 2);
            area.push((coords[0].0, base));
            area.extend(coords.iter().copied());
            area.push((coords[coords.len() - 1].0, base));
            self.polygon(&area, Some(tint(chart.color, 0.14)), None);
            self.polyline(&coords, chart.color, 1.3);
        } else {
            let (px, py) = coords[0];
            self.fill_rect(px - 0.6, py - 0.6, 1.2, 1.2, chart.color);
        }

        self.text_at(ox + 1.0, grid_y(0) + 0.9, &chart.max_label, 6.5, false, MUTED);
        self.text_at(ox + 1.0, base - 2.7, &chart.min_label, 6.5, false, MUTED);

        let foot = oy - CHART_PLOT_H - 3.8;
        self.text_at(ox, foot, "0 s", 6.5, false, MUTED);
        self.text_right(ox + CHART_PLOT_W, foot, &chart.duration_label, 6.5, false, MUTED);
    }

    // ------------------------------------------------------------ sortie

    fn draw_footer(&mut self, page: usize, total: usize) {
        self.polyline(
            &[(MARGIN, 14.0), (MARGIN + CONTENT_W, 14.0)],
            GRID,
            0.4,
        );
        let page_label = rust_i18n::t!("report.pdf.page", n = page, total = total).to_string();
        let right_w = self.text_width(&page_label, 7.5, false);
        let left = self.fit_text(
            &self.footer_left.clone(),
            7.5,
            false,
            CONTENT_W - right_w - 6.0,
        );
        self.text_at(MARGIN, 10.0, &left, 7.5, false, MUTED);
        self.text_right(MARGIN + CONTENT_W, 10.0, &page_label, 7.5, false, MUTED);
    }

    pub fn finish(mut self) -> Vec<u8> {
        let last = std::mem::take(&mut self.ops);
        self.pages.push(last);
        let pages = std::mem::take(&mut self.pages);
        let total = pages.len();
        let mut final_pages = Vec::with_capacity(total);
        for (index, page_ops) in pages.into_iter().enumerate() {
            self.ops = page_ops;
            self.draw_footer(index + 1, total);
            let ops = std::mem::take(&mut self.ops);
            final_pages.push(PdfPage::new(Mm(PAGE_W), Mm(PAGE_H), ops));
        }
        let mut warnings = Vec::new();
        self.doc
            .with_pages(final_pages)
            .save(&PdfSaveOptions::default(), &mut warnings)
    }
}

fn align_of_vec(aligns: &[Align], len: usize) -> Vec<Align> {
    (0..len)
        .map(|index| aligns.get(index).copied().unwrap_or(Align::Left))
        .collect()
}
