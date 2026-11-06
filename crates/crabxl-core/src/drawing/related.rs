/// Relationship owned by a drawing or chart part.
#[derive(Clone, Debug, PartialEq)]
pub struct PartRelationship {
    /// Relationship identifier, retained in the owning XML.
    pub id: Box<str>,
    /// Full relationship type URI.
    pub kind: Box<str>,
    /// Absolute package name for an internal target, or an external URI.
    pub target: Box<str>,
    /// Whether the target is external to the package. External URIs are never fetched.
    pub external: bool,
}

/// Immutable dependency payload. Copies share bytes and have independent relationships.
#[derive(Clone, Debug, PartialEq)]
pub struct RelatedPart {
    /// Source identity referenced by internal graph edges.
    pub name: Box<str>,
    /// OPC content type.
    pub content_type: Box<str>,
    /// Encoded part payload, without its relationships part.
    pub data: crate::BinaryPayload,
    /// Outgoing relationships, retaining their original identifiers.
    pub relationships: Vec<PartRelationship>,
}
impl RelatedPart {
    /// Retained payload size.
    pub fn heap_bytes(&self) -> usize {
        self.name.len()
            + self.content_type.len()
            + self.data.memory_bytes()
            + self
                .relationships
                .iter()
                .map(|rel| {
                    size_of::<PartRelationship>() + rel.id.len() + rel.kind.len() + rel.target.len()
                })
                .sum::<usize>()
    }
}

/// A relationship-resolved graph owned by a drawing object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartGraph {
    /// Original root identity, used for back references.
    pub source_part: Option<Box<str>>,
    /// Root relationships.
    pub relationships: Vec<PartRelationship>,
    /// Immutable dependent parts.
    pub parts: Vec<RelatedPart>,
}
impl PartGraph {
    /// Retained metadata and payload allocation.
    pub fn heap_bytes(&self) -> usize {
        self.source_part.as_ref().map_or(0, |part| part.len())
            + self
                .parts
                .iter()
                .map(RelatedPart::heap_bytes)
                .sum::<usize>()
            + self
                .relationships
                .iter()
                .map(|rel| {
                    size_of::<PartRelationship>() + rel.id.len() + rel.kind.len() + rel.target.len()
                })
                .sum::<usize>()
    }
}
