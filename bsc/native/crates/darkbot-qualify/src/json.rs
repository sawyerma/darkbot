//! A small JSON value and its pretty-printed form. Enough for evidence;
//! no parsing.

use std::fmt::Write as _;

/// A JSON value. Object members keep their order.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// An unsigned integer.
    U64(u64),
    /// A signed integer.
    I64(i64),
    /// A number with a fraction; written with six decimals, `null` if not
    /// finite.
    F64(f64),
    /// A string.
    Str(String),
    /// An array.
    Array(Vec<Json>),
    /// An object.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// An object from `(key, value)` pairs.
    pub fn object<const N: usize>(members: [(&str, Json); N]) -> Self {
        Self::Object(
            members
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    /// A string value.
    pub fn str(value: impl Into<String>) -> Self {
        Self::Str(value.into())
    }

    /// `null` for `None`, the mapped value otherwise.
    pub fn opt<T>(value: Option<T>, map: impl FnOnce(T) -> Json) -> Self {
        value.map_or(Self::Null, map)
    }

    /// The pretty-printed text, two spaces per level, ending in a newline.
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::U64(value) => {
                let _ = write!(out, "{value}");
            }
            Self::I64(value) => {
                let _ = write!(out, "{value}");
            }
            Self::F64(value) if value.is_finite() => {
                let _ = write!(out, "{value:.6}");
            }
            Self::F64(_) => out.push_str("null"),
            Self::Str(value) => escape(value, out),
            Self::Array(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    out.push_str(if index == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    item.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push(']');
            }
            Self::Object(members) => {
                if members.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (index, (key, value)) in members.iter().enumerate() {
                    out.push_str(if index == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    escape(key, out);
                    out.push_str(": ");
                    value.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push('}');
            }
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// Writes `text` as a JSON string.
fn escape(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if u32::from(ch) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(ch));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}
