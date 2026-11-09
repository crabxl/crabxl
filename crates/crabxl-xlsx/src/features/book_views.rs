//! Workbook views retain opaque metadata while active-tab normalization stays canonical.
use crabxl_core::{ActiveViewSelection, XmlContent, XmlElement};
use std::io::{self, Write};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const STRICT: &str = "http://purl.oclc.org/ooxml/spreadsheetml/main";

pub(crate) fn write(
    out: &mut impl Write,
    views: &XmlElement,
    uri: Option<&str>,
    active: Option<ActiveViewSelection>,
) -> io::Result<()> {
    if views.local_name() != "bookViews" {
        return Err(io::Error::other("Expected workbook views"));
    }
    let mut views = views.clone();
    let parent_attributes = views.attributes.clone();
    let mut first_view = true;
    for child in &mut views.children {
        let XmlContent::Element(child) = child else {
            continue;
        };
        if child.local_name() != "workbookView" {
            continue;
        }
        let declaration = child.name.split_once(':').map_or_else(
            || "xmlns".to_string(),
            |(prefix, _)| format!("xmlns:{prefix}"),
        );
        let namespace = child.attribute(&declaration).or_else(|| {
            parent_attributes
                .iter()
                .find(|(name, _)| name.as_ref() == declaration)
                .map(|(_, value)| value.as_ref())
        });
        if !matches!(namespace, Some(MAIN | STRICT)) {
            continue;
        }
        if first_view && let Some(active) = active {
            if let Some(index) = active.serialized_index {
                child.set_attribute("activeTab", index.to_string());
            } else {
                child
                    .attributes
                    .retain(|(name, _)| name.as_ref() != "activeTab");
            }
        }
        first_view = false;
        child.name = "workbookView".into();
        if child
            .attribute("xmlns")
            .is_some_and(|namespace| namespace != uri.unwrap_or(MAIN))
        {
            child.set_attribute("xmlns", uri.unwrap_or(MAIN));
        }
    }
    views.name = "bookViews".into();
    views.set_attribute("xmlns", uri.unwrap_or(MAIN));
    crate::drawing_xml::write(out, &views, 0)
}
