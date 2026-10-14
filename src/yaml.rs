//! YAML in, serde out: `saphyr` parses, this module is the serde bridge.
//!
//! The layout file and the keymap the GUI uploads are YAML. They were read
//! with `serde_yaml_ng`, which links `unsafe-libyaml` -- unmaintained,
//! RUSTSEC-2024-0449, refused by the audit gate. `saphyr` is pure Rust and
//! maintained (the `yaml-rust2` line; media_server moved to it for the same
//! advisory) but ships no serde support, so this is that support, kept to
//! what `serde_yaml_ng` did for this crate's types:
//!
//! * a scalar read where a STRING is wanted is its text as written
//!   (`buttons: [[1]]` binds the key `"1"`); anywhere else it resolves under
//!   the YAML 1.2 core schema (`rows: 1` is a number, `rows: '1'` is not);
//! * a duplicate key, a second document and an unknown alias are errors;
//! * aliases resolve; a merge key (`<<`) is an ordinary key, which serde's
//!   derived structs ignore -- `serde_yaml_ng`'s typed path never applied
//!   merges either (probed 2026-10-14);
//! * every error names its line and column.

use std::borrow::Cow;
use std::fmt;

use saphyr::{MarkedYaml, Scalar, ScalarStyle, Tag, YamlData, YamlLoader};
use saphyr_parser::Parser;
use serde::de::{self, DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};

/// A YAML parse or type error, located in the source.
#[derive(Debug)]
pub struct Error {
    msg: String,
    located: bool,
}

impl Error {
    fn new(msg: String) -> Self {
        Self {
            msg,
            located: false,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for Error {}

impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self::new(msg.to_string())
    }
}

/// Parse one YAML document into `T`. An empty stream is a null document.
///
/// # Errors
/// Malformed YAML, more than one document, a duplicate key, an alias to an
/// unknown anchor, or a document `T` does not accept; each with its line.
pub fn from_str<T: DeserializeOwned>(text: &str) -> Result<T, Error> {
    let mut loader: YamlLoader<'_, MarkedYaml<'_>> = YamlLoader::default();
    // Keep each scalar's text: a string field reads it as written, and every
    // other field resolves it here (`Node::scalar`).
    loader.early_parse(false);
    let scan = |e: &saphyr::ScanError| Error {
        msg: format!("YAML: {e}"),
        located: true,
    };
    Parser::new_from_str(text)
        .load(&mut loader, true)
        .map_err(|e| scan(&e))?;
    if let Some(e) = loader.error() {
        return Err(scan(e));
    }
    match loader.into_documents().as_slice() {
        [] => {
            let start = saphyr::Marker::new(0, 1, 0);
            T::deserialize(Node(&MarkedYaml {
                span: saphyr_parser::Span { start, end: start },
                data: YamlData::Value(Scalar::Null),
            }))
        }
        [doc] => T::deserialize(Node(doc)),
        more => Err(Error::new(format!(
            "YAML: {} documents in one stream; expected one",
            more.len()
        ))),
    }
}

/// One node of the parsed document, as a serde `Deserializer`.
struct Node<'a, 'i>(&'a MarkedYaml<'i>);

impl<'a, 'i> Node<'a, 'i> {
    /// The node's data with any user tag peeled off: a tag changes nothing
    /// this crate reads.
    fn data(&self) -> &'a YamlData<'i, MarkedYaml<'i>> {
        let mut node = self.0;
        while let YamlData::Tagged(_, inner) = &node.data {
            node = inner;
        }
        &node.data
    }

    /// The scalar this node holds, resolved under the core schema; `None`
    /// for a collection.
    fn scalar(&self) -> Result<Option<Scalar<'i>>, Error> {
        match self.data() {
            YamlData::Representation(text, style, tag) => resolve(text, *style, tag.as_ref())
                .map(Some)
                .map_err(|e| self.locate(e)),
            YamlData::Value(s) => Ok(Some(s.clone())),
            YamlData::Alias(_) | YamlData::BadValue => {
                Err(self.locate(Error::new("YAML: unresolved node".to_owned())))
            }
            YamlData::Sequence(_) | YamlData::Mapping(_) | YamlData::Tagged(..) => Ok(None),
        }
    }

    /// The text of a scalar, as written.
    fn text(&self) -> Option<&'a str> {
        match self.data() {
            YamlData::Representation(text, ..) => Some(text),
            _ => None,
        }
    }

    /// Name this node's position on an error that has none yet; the
    /// innermost node an error passes through is the one that caused it.
    fn locate(&self, e: Error) -> Error {
        if e.located {
            return e;
        }
        let at = self.0.span.start;
        Error {
            msg: format!("{} at line {} column {}", e.msg, at.line(), at.col() + 1),
            located: true,
        }
    }
}

fn resolve<'i>(
    text: &Cow<'i, str>,
    style: ScalarStyle,
    tag: Option<&Cow<'i, Tag>>,
) -> Result<Scalar<'i>, Error> {
    Scalar::parse_from_cow_and_metadata(text.clone(), style, tag)
        .ok_or_else(|| Error::new(format!("YAML: {text:?} is not what its tag says")))
}

impl<'de> de::Deserializer<'de> for Node<'_, '_> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let out = match self.scalar()? {
            Some(Scalar::Null) => visitor.visit_unit(),
            Some(Scalar::Boolean(b)) => visitor.visit_bool(b),
            Some(Scalar::Integer(i)) => visitor.visit_i64(i),
            Some(Scalar::FloatingPoint(f)) => visitor.visit_f64(f.into_inner()),
            Some(Scalar::String(s)) => visitor.visit_str(&s),
            None => match self.data() {
                YamlData::Sequence(items) => visitor.visit_seq(Seq(items.iter())),
                YamlData::Mapping(map) => visitor.visit_map(Map {
                    entries: map.iter(),
                    value: None,
                }),
                _ => Err(Error::new("YAML: unresolved node".to_owned())),
            },
        };
        out.map_err(|e| self.locate(e))
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.text() {
            Some(text) => visitor.visit_str(text).map_err(|e| self.locate(e)),
            None => self.deserialize_any(visitor),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if matches!(self.scalar()?, Some(Scalar::Null)) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        let out = if let Some(text) = self.text() {
            visitor.visit_enum(de::value::StrDeserializer::<Error>::new(text))
        } else {
            match self.data() {
                YamlData::Mapping(map) if map.len() == 1 => visitor.visit_enum(Map {
                    entries: map.iter(),
                    value: None,
                }),
                _ => Err(Error::new(
                    "YAML: an enum is a name or a one-key mapping".to_owned(),
                )),
            }
        };
        out.map_err(|e| self.locate(e))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct ignored_any
    }
}

struct Seq<I>(I);

impl<'de, 'a, 'i: 'a, I: Iterator<Item = &'a MarkedYaml<'i>>> SeqAccess<'de> for Seq<I> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        self.0.next().map(|n| seed.deserialize(Node(n))).transpose()
    }
}

struct Map<'a, 'i, I> {
    entries: I,
    value: Option<&'a MarkedYaml<'i>>,
}

impl<'de, 'a, 'i: 'a, I> MapAccess<'de> for Map<'a, 'i, I>
where
    I: Iterator<Item = (&'a MarkedYaml<'i>, &'a MarkedYaml<'i>)>,
{
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        self.value = Some(value);
        seed.deserialize(Node(key)).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("YAML: a value asked for before its key".to_owned()))?;
        seed.deserialize(Node(value))
    }
}

impl<'de, 'a, 'i: 'a, I> de::EnumAccess<'de> for Map<'a, 'i, I>
where
    I: Iterator<Item = (&'a MarkedYaml<'i>, &'a MarkedYaml<'i>)>,
{
    type Error = Error;
    type Variant = Node<'a, 'i>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        mut self,
        seed: V,
    ) -> Result<(V::Value, Node<'a, 'i>), Error> {
        let (key, value) = self
            .entries
            .next()
            .ok_or_else(|| Error::new("YAML: an empty enum mapping".to_owned()))?;
        Ok((seed.deserialize(Node(key))?, Node(value)))
    }
}

impl<'de> de::VariantAccess<'de> for Node<'_, '_> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        de::Deserialize::deserialize(self)
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(self)
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_seq(self, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_map(self, visitor)
    }
}

#[cfg(test)]
#[path = "yaml_tests.rs"]
mod tests;
