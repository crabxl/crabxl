//! Worksheet header and footer.
//!
//! Modelled after openpyxl `worksheet/header_footer.py` (openpyxl, MIT). Each
//! header or footer keeps its raw Excel format string; [`HeaderFooterSections`]
//! splits it into left/center/right parts and builds it back.

/// Header or footer of one page kind, as raw Excel format strings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeaderFooter {
    /// `differentOddEven` attribute.
    pub different_odd_even: Option<bool>,
    /// `differentFirst` attribute.
    pub different_first: Option<bool>,
    /// `scaleWithDoc` attribute.
    pub scale_with_doc: Option<bool>,
    /// `alignWithMargins` attribute.
    pub align_with_margins: Option<bool>,
    /// `<oddHeader>` text, e.g. `&L&"Arial"left&Ccenter`.
    pub odd_header: Option<Box<str>>,
    /// `<oddFooter>` text.
    pub odd_footer: Option<Box<str>>,
    /// `<evenHeader>` text.
    pub even_header: Option<Box<str>>,
    /// `<evenFooter>` text.
    pub even_footer: Option<Box<str>>,
    /// `<firstHeader>` text.
    pub first_header: Option<Box<str>>,
    /// `<firstFooter>` text.
    pub first_footer: Option<Box<str>>,
}

impl HeaderFooter {
    /// True when nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        [
            &self.odd_header,
            &self.odd_footer,
            &self.even_header,
            &self.even_footer,
            &self.first_header,
            &self.first_footer,
        ]
        .into_iter()
        .map(|v| v.as_ref().map_or(0, |s| s.len()))
        .sum()
    }
}

/// One left, center or right part with its font formatting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeaderFooterPart {
    /// Text including Excel codes such as `&P`.
    pub text: Box<str>,
    /// Font name (`&"Arial,Bold"`).
    pub font: Option<Box<str>>,
    /// Font size in points (`&12`).
    pub size: Option<u32>,
    /// Six digit RGB hex color (`&KFF0000`).
    pub color: Option<Box<str>>,
}

impl HeaderFooterPart {
    /// Plain text part.
    pub fn text(text: &str) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// Parse one part (no `&L`/`&C`/`&R` marker), like openpyxl `from_str`.
    pub fn parse(source: &str) -> Self {
        let mut part = Self::default();
        let mut text = String::new();
        let bytes = source.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'&' {
                let len = source[i..].chars().next().map_or(1, char::len_utf8);
                text.push_str(&source[i..i + len]);
                i += len;
                continue;
            }
            match bytes.get(i + 1) {
                Some(b'&') => {
                    text.push_str("&&");
                    i += 2;
                }
                Some(b'"') => {
                    if let Some(close) = source[i + 2..].find('"').filter(|c| *c > 0) {
                        part.font = Some(source[i + 2..i + 2 + close].into());
                        i += close + 3;
                    } else {
                        text.push('&');
                        i += 1;
                    }
                }
                Some(b'K')
                    if bytes.len() >= i + 8
                        && bytes[i + 2..i + 8]
                            .iter()
                            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(b)) =>
                {
                    part.color = Some(source[i + 2..i + 8].into());
                    i += 8;
                }
                Some(d) if d.is_ascii_digit() => {
                    let mut j = i + 1;
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                    part.size = source[i + 1..j].parse().ok();
                    if bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
                        j += 1;
                    }
                    i = j;
                }
                _ => {
                    text.push('&');
                    i += 1;
                }
            }
        }
        part.text = text.into();
        part
    }

    /// Render as Excel format text without position marker.
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some(font) = self.font.as_deref().filter(|f| !f.is_empty()) {
            out.push_str(&format!("&\"{font}\""));
        }
        if let Some(size) = self.size.filter(|s| *s != 0) {
            out.push_str(&format!("&{size} "));
        }
        if let Some(color) = self.color.as_deref().filter(|c| !c.is_empty()) {
            out.push_str(&format!("&K{color}"));
        }
        out.push_str(&self.text);
        out
    }
}

/// A header or footer split into its three sections.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeaderFooterSections {
    /// Left section (`&L`).
    pub left: Option<HeaderFooterPart>,
    /// Center section (`&C`).
    pub center: Option<HeaderFooterPart>,
    /// Right section (`&R`).
    pub right: Option<HeaderFooterPart>,
}

impl HeaderFooterSections {
    /// Split an Excel header/footer format string. Text that does not start
    /// with a section marker yields no sections, like openpyxl.
    pub fn parse(text: &str) -> Self {
        let bytes = text.as_bytes();
        // (marker byte, content start) of every section marker in order.
        let mut marks: Vec<(u8, usize, usize)> = Vec::new();
        let mut i = 0;
        let mut order = 0u8;
        while i + 1 < bytes.len() {
            if bytes[i] == b'&' {
                let kind = bytes[i + 1];
                if kind == b'&' {
                    i += 2;
                    continue;
                }
                let rank = match kind {
                    b'L' => 1,
                    b'C' => 2,
                    b'R' => 3,
                    _ => 0,
                };
                // Markers must appear in L, C, R order and have content.
                if rank > order && i + 2 < bytes.len() && (marks.is_empty() == (i == 0)) {
                    marks.push((kind, i + 2, i));
                    order = rank;
                    i += 2;
                    continue;
                }
            }
            i += 1;
        }
        let mut out = Self::default();
        for (index, (kind, start, _)) in marks.iter().enumerate() {
            let end = marks.get(index + 1).map_or(text.len(), |m| m.2);
            let part = HeaderFooterPart::parse(&text[*start..end]);
            match kind {
                b'L' => out.left = Some(part),
                b'C' => out.center = Some(part),
                _ => out.right = Some(part),
            }
        }
        out
    }

    /// Build the Excel format string, abbreviating `&[Page]` style codes like
    /// openpyxl.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (key, part) in [('L', &self.left), ('C', &self.center), ('R', &self.right)] {
            if let Some(part) = part {
                out.push('&');
                out.push(key);
                out.push_str(&part.render());
            }
        }
        for (long, short) in [
            ("&[Tab]", "&A"),
            ("&[Pages]", "&N"),
            ("&[Date]", "&D"),
            ("&[Path]", "&Z"),
            ("&[Page]", "&P"),
            ("&[Time]", "&T"),
            ("&[File]", "&F"),
            ("&[Picture]", "&G"),
        ] {
            if out.contains(long) {
                out = out.replace(long, short);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn split_and_build() {
        let s = HeaderFooterSections::parse("&LL&C&\"Arial,Bold\"&12 &KFF0000C&RR&P");
        assert_eq!(s.left, Some(HeaderFooterPart::text("L")));
        let c = s.center.clone().unwrap_or_default();
        assert_eq!(&*c.text, "C");
        assert_eq!(c.font.as_deref(), Some("Arial,Bold"));
        assert_eq!(c.size, Some(12));
        assert_eq!(c.color.as_deref(), Some("FF0000"));
        assert_eq!(s.right, Some(HeaderFooterPart::text("R&P")));
        assert_eq!(s.render(), "&LL&C&\"Arial,Bold\"&12 &KFF0000C&RR&P");
    }

    #[test]
    fn no_marker_and_codes() {
        assert_eq!(
            HeaderFooterSections::parse("plain"),
            HeaderFooterSections::default()
        );
        let s = HeaderFooterSections {
            right: Some(HeaderFooterPart::text("&[Page] of &[Pages]")),
            ..Default::default()
        };
        assert_eq!(s.render(), "&R&P of &N");
    }

    #[test]
    fn ampersand_escape_is_kept() {
        let s = HeaderFooterSections::parse("&CA&&C");
        assert_eq!(s.center, Some(HeaderFooterPart::text("A&&C")));
    }
}
