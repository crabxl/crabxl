//! Canonical style hashing; equal positive/negative zero values hash equally.
use crate::{Alignment, Color, GradientFill, GradientStop, RunFont};
use std::hash::{Hash, Hasher};
fn bits(value: f64) -> u64 {
    if value == 0.0 { 0 } else { value.to_bits() }
}
fn float<H: Hasher>(value: Option<f64>, state: &mut H) {
    value.map(bits).hash(state);
}
impl Hash for Color {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        float(self.tint, state);
    }
}
impl Hash for RunFont {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        float(self.size, state);
        self.bold.hash(state);
        self.italic.hash(state);
        self.strike.hash(state);
        self.outline.hash(state);
        self.shadow.hash(state);
        self.condense.hash(state);
        self.extend.hash(state);
        self.underline.hash(state);
        self.vertical.hash(state);
        self.charset.hash(state);
        float(self.family, state);
        self.scheme.hash(state);
        self.color.hash(state);
    }
}
impl Hash for Alignment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.horizontal.hash(state);
        self.vertical.hash(state);
        self.rotation.hash(state);
        self.wrap_text.hash(state);
        self.shrink_to_fit.hash(state);
        float(self.indent, state);
        float(self.relative_indent, state);
        self.justify_last_line.hash(state);
        float(self.reading_order, state);
        self.merge_cell.hash(state);
    }
}
impl Hash for GradientStop {
    fn hash<H: Hasher>(&self, state: &mut H) {
        bits(self.position).hash(state);
        self.color.hash(state);
    }
}
impl Hash for GradientFill {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        float(self.degree, state);
        for value in self.edges {
            float(value, state);
        }
        self.stops.hash(state);
    }
}
