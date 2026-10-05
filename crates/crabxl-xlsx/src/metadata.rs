//! Shared bounded XML attribute decoding for worksheet metadata codecs.
use crabxl_core::{Error, ErrorKind, Result};
use quick_xml::{encoding::Decoder, events::BytesStart};
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
pub(crate) fn boolean(value: &str) -> Result<bool> {
    match value {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => Err(invalid("Invalid worksheet metadata boolean")),
    }
}
pub(crate) fn integer(value: &str) -> Result<i64> {
    value
        .parse()
        .map_err(|_| invalid("Invalid worksheet metadata integer"))
}
pub(crate) fn attributes(
    e: &BytesStart<'_>,
    decoder: Decoder,
    mut apply: impl FnMut(&[u8], &str) -> Result<()>,
) -> Result<()> {
    for attribute in e.attributes() {
        let attribute = attribute.map_err(|error| {
            Error::caused_by(
                ErrorKind::Xml,
                "Invalid worksheet metadata attribute",
                error,
            )
        })?;
        let name = attribute.key.as_ref();
        if name == b"xmlns" || name.starts_with(b"xmlns:") {
            continue;
        }
        let value = attribute
            .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
            .map_err(|error| {
                Error::caused_by(
                    ErrorKind::Xml,
                    "Cannot decode worksheet metadata attribute",
                    error,
                )
            })?;
        apply(name, &value)?;
    }
    Ok(())
}
