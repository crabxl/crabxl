//! Runtime-independent validation shared by imported, registered and rich styles.
use crate::{Alignment, Border, CellStyle, Color, Error, ErrorKind, Fill, Font, Result};
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
impl Color {
    /// Validate numeric ranges and shared component invariants before model mutation.
    pub fn validate(&self) -> Result<()> {
        let color = self;

        if color
            .tint
            .is_some_and(|v| !v.is_finite() || !(-1.0..=1.0).contains(&v))
        {
            return Err(invalid("Invalid color tint"));
        }
        Ok(())
    }
}
impl Font {
    /// Validate numeric ranges and shared component invariants before model mutation.
    pub fn validate(&self) -> Result<()> {
        let font = self;

        if font.size.is_some_and(|s| !s.is_finite()) {
            return Err(invalid("Invalid font size"));
        }
        if font
            .family
            .is_some_and(|family| !family.is_finite() || !(0.0..=14.0).contains(&family))
        {
            return Err(invalid("Font family exceeds the public baseline range"));
        }
        if let Some(color) = font.color {
            color.validate()?;
        }
        Ok(())
    }
}
impl Fill {
    /// Validate numeric ranges and shared component invariants before model mutation.
    pub fn validate(&self) -> Result<()> {
        let fill = self;

        match fill {
            Fill::Pattern(v) => {
                for c in [v.foreground, v.background].into_iter().flatten() {
                    c.validate()?;
                }
            }
            Fill::Gradient(v) => {
                if v.degree.is_some_and(|n| !n.is_finite())
                    || v.edges.into_iter().flatten().any(|n| !n.is_finite())
                {
                    return Err(invalid("Invalid gradient geometry"));
                }
                for s in &v.stops {
                    if !s.position.is_finite() || !(0.0..=1.0).contains(&s.position) {
                        return Err(invalid("Invalid or duplicate gradient stop position"));
                    }
                    s.color.validate()?;
                }
                let mut positions = Vec::new();
                positions
                    .try_reserve_exact(v.stops.len())
                    .map_err(|error| {
                        Error::caused_by(
                            ErrorKind::MemoryBudgetExceeded,
                            "Cannot validate gradient stop positions",
                            error,
                        )
                    })?;
                positions.extend(v.stops.iter().map(|s| {
                    if s.position == 0.0 {
                        0
                    } else {
                        s.position.to_bits()
                    }
                }));
                positions.sort_unstable();
                if positions.windows(2).any(|pair| pair[0] == pair[1]) {
                    return Err(invalid("Duplicate gradient stop position"));
                }
            }
        }
        Ok(())
    }
}
impl Border {
    /// Validate numeric ranges and shared component invariants before model mutation.
    pub fn validate(&self) -> Result<()> {
        let v = self;

        for color in v.sides.iter().flatten().filter_map(|s| s.color) {
            color.validate()?;
        }
        Ok(())
    }
}
impl Alignment {
    /// Validate numeric ranges and shared component invariants before model mutation.
    pub fn validate(&self) -> Result<()> {
        let v = self;

        if v.rotation.is_some_and(|n| n > 180 && n != 255)
            || v.indent
                .is_some_and(|n| !n.is_finite() || !(0.0..=255.0).contains(&n))
            || v.relative_indent
                .is_some_and(|n| !n.is_finite() || !(-255.0..=255.0).contains(&n))
            || v.reading_order.is_some_and(|n| !n.is_finite() || n < 0.0)
        {
            return Err(invalid("Invalid cell alignment"));
        }
        Ok(())
    }
}
impl CellStyle {
    /// Validate common appearance semantics, independent of an output format.
    pub fn validate(&self) -> Result<()> {
        self.font.validate()?;
        self.fill.validate()?;
        self.borders.validate()?;
        self.alignment.validate()
    }
}
