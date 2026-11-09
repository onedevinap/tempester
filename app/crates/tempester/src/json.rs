//! A small JSON value written by hand (no serde): enough to send events to the page. Strings are escaped so that
//! the text is also safe to embed in a JavaScript expression (U+2028 and U+2029 included).

use std::fmt::Write;

/// A JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(&'static str, Json)>),
}

impl Json {
    /// The compact text of the value.
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Int(n) => out.push_str(&n.to_string()),
            Self::Str(s) => write_string(s, out),
            Self::Arr(items) => write_list(items.iter(), '[', ']', out, Json::write_to),
            Self::Obj(fields) => write_list(fields.iter(), '{', '}', out, write_field),
        }
    }
}

fn write_list<T>(
    items: impl Iterator<Item = T>,
    open: char,
    close: char,
    out: &mut String,
    write_item: fn(T, &mut String),
) {
    out.push(open);
    for (k, item) in items.enumerate() {
        if k > 0 {
            out.push(',');
        }
        write_item(item, out);
    }
    out.push(close);
}

fn write_field(field: &(&'static str, Json), out: &mut String) {
    write_string(field.0, out);
    out.push(':');
    field.1.write_to(out);
}

fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                // Writing to a String cannot fail.
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl From<&str> for Json {
    fn from(s: &str) -> Self {
        Self::Str(s.to_string())
    }
}

impl From<String> for Json {
    fn from(s: String) -> Self {
        Self::Str(s)
    }
}

impl From<bool> for Json {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<i64> for Json {
    fn from(n: i64) -> Self {
        Self::Int(n)
    }
}

impl From<usize> for Json {
    fn from(n: usize) -> Self {
        Self::Int(i64::try_from(n).unwrap_or(i64::MAX))
    }
}

impl From<u16> for Json {
    fn from(n: u16) -> Self {
        Self::Int(i64::from(n))
    }
}

impl From<u8> for Json {
    fn from(n: u8) -> Self {
        Self::Int(i64::from(n))
    }
}

impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(items: Vec<T>) -> Self {
        Self::Arr(items.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
    }
}

/// `obj([("a", 1.into()), ...])` without spelling out the enum.
pub fn obj<const N: usize>(fields: [(&'static str, Json); N]) -> Json {
    Json::Obj(fields.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_when_object_with_all_kinds_returns_compact_json() {
        let value = obj([
            ("a", 1usize.into()),
            ("b", "x".into()),
            ("c", vec![1u8, 2].into()),
            ("d", Json::Null),
            ("e", true.into()),
        ]);

        assert_eq!(
            value.render(),
            r#"{"a":1,"b":"x","c":[1,2],"d":null,"e":true}"#
        );
    }

    #[test]
    fn test_render_when_string_has_quote_backslash_and_newline_returns_escapes() {
        let value = Json::from("say \"hi\"\\\n\t\r");

        assert_eq!(value.render(), r#""say \"hi\"\\\n\t\r""#);
    }

    #[test]
    fn test_render_when_string_has_control_character_returns_unicode_escape() {
        assert_eq!(Json::from("a\u{1}b\u{1f}").render(), r#""a\u0001b\u001f""#);
    }

    #[test]
    fn test_render_when_string_has_line_separators_returns_escaped_for_javascript() {
        let text = Json::from("a\u{2028}b\u{2029}").render();

        assert_eq!(text, "\"a\\u2028b\\u2029\"");
        assert!(!text.contains('\u{2028}') && !text.contains('\u{2029}'));
    }

    #[test]
    fn test_render_when_non_ascii_returns_it_unescaped() {
        assert_eq!(Json::from("é♪").render(), "\"é♪\"");
    }

    #[test]
    fn test_render_when_script_closing_tag_returns_text_kept_inside_the_string() {
        // The page receives the text through evaluate_script, not inside an HTML element: `</script>` is inert.
        assert_eq!(Json::from("</script>").render(), "\"</script>\"");
    }

    #[test]
    fn test_render_when_option_none_and_negative_number_returns_null_and_minus() {
        let value = obj([("n", None::<u8>.into()), ("m", (-5i64).into())]);

        assert_eq!(value.render(), r#"{"n":null,"m":-5}"#);
    }

    #[test]
    fn test_render_when_empty_containers_returns_brackets() {
        assert_eq!(Json::Arr(vec![]).render(), "[]");
        assert_eq!(Json::Obj(vec![]).render(), "{}");
    }
}
