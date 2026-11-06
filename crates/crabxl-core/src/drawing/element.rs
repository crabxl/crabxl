//! Lossless small DrawingML property trees. Cell data never uses this model.

/// A qualified DrawingML element with attributes and ordered mixed content.
#[derive(Clone, Debug, PartialEq)]
pub struct XmlElement {
    /// Qualified XML name, including its prefix when present.
    pub name: Box<str>,
    /// Attributes, including namespace declarations, in source order.
    pub attributes: Vec<(Box<str>, Box<str>)>,
    /// Children and character data in source order.
    pub children: Vec<XmlContent>,
}

/// Child content of a DrawingML property element.
#[derive(Clone, Debug, PartialEq)]
pub enum XmlContent {
    /// Nested element.
    Element(XmlElement),
    /// Decoded XML character data.
    Text(Box<str>),
    /// An XML comment retained in its original position.
    Comment(Box<str>),
    /// Processing instruction body, including its target.
    ProcessingInstruction(Box<str>),
}
impl XmlElement {
    /// Create an empty element.
    pub fn new(name: impl Into<Box<str>>) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }
    /// Local name without the optional namespace prefix.
    pub fn local_name(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }
    /// Attribute value by qualified name.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key.as_ref() == name)
            .map(|(_, value)| value.as_ref())
    }
    /// Replace an attribute, or append it when absent.
    pub fn set_attribute(&mut self, name: impl Into<Box<str>>, value: impl Into<Box<str>>) {
        let name = name.into();
        let value = value.into();
        if let Some((_, old)) = self.attributes.iter_mut().find(|(key, _)| *key == name) {
            *old = value;
        } else {
            self.attributes.push((name, value));
        }
    }
    /// First child with this local name.
    pub fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find_map(|child| match child {
            XmlContent::Element(child) if child.local_name() == name => Some(child),
            _ => None,
        })
    }
    /// First mutable child with this local name.
    pub fn child_mut(&mut self, name: &str) -> Option<&mut Self> {
        self.children.iter_mut().find_map(|child| match child {
            XmlContent::Element(child) if child.local_name() == name => Some(child),
            _ => None,
        })
    }
    /// Retained tree payload size.
    pub fn heap_bytes(&self) -> usize {
        self.name.len()
            + self
                .attributes
                .iter()
                .map(|(key, value)| 2 * size_of::<Box<str>>() + key.len() + value.len())
                .sum::<usize>()
            + self
                .children
                .iter()
                .map(|child| {
                    size_of::<XmlContent>()
                        + match child {
                            XmlContent::Element(value) => value.heap_bytes(),
                            XmlContent::Text(value)
                            | XmlContent::Comment(value)
                            | XmlContent::ProcessingInstruction(value) => value.len(),
                        }
                })
                .sum::<usize>()
    }
}
