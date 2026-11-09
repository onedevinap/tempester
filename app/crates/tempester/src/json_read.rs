//! A small JSON reader (no serde), the counterpart of `json.rs`: enough to read back `library.json`, which the app
//! writes itself. Integers only (dates are whole seconds); anything else is a refusal, so a damaged or foreign file
//! is noticed instead of half-read. Nesting is limited so that a hostile file cannot exhaust the stack.

/// A parsed JSON value. Object keys keep their file order.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

const MAX_DEPTH: usize = 32;

impl Value {
    /// The member `key` of an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Arr(items) => Some(items),
            _ => None,
        }
    }
}

/// Parses one JSON document; trailing characters are an error.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    let value = reader.value(0)?;
    reader.skip_space();
    if reader.at != reader.bytes.len() {
        return Err(reader.fail("trailing characters"));
    }
    Ok(value)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn fail(&self, why: &str) -> String {
        format!("{why} at byte {}", self.at)
    }

    fn skip_space(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.skip_space();
        let found = self.bytes.get(self.at) == Some(&byte);
        if found {
            self.at += 1;
        }
        found
    }

    fn word(&mut self, word: &str, value: Value) -> Result<Value, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.fail("unexpected character"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(self.fail("nested too deep"));
        }
        self.skip_space();
        match self.bytes.get(self.at) {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Value::Str),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'n') => self.word("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.fail("a value was expected")),
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.at;
        if self.bytes.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        while matches!(self.bytes.get(self.at), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        if matches!(self.bytes.get(self.at), Some(b'.' | b'e' | b'E')) {
            return Err(self.fail("only whole numbers are expected"));
        }
        std::str::from_utf8(&self.bytes[start..self.at])
            .ok()
            .and_then(|digits| digits.parse().ok())
            .map(Value::Int)
            .ok_or_else(|| self.fail("bad number"))
    }

    fn array(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1;
        let mut items = Vec::new();
        if self.eat(b']') {
            return Ok(Value::Arr(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            if self.eat(b']') {
                return Ok(Value::Arr(items));
            }
            if !self.eat(b',') {
                return Err(self.fail("',' or ']' was expected"));
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1;
        let mut fields = Vec::new();
        if self.eat(b'}') {
            return Ok(Value::Obj(fields));
        }
        loop {
            self.skip_space();
            let key = self.string()?;
            if !self.eat(b':') {
                return Err(self.fail("':' was expected"));
            }
            fields.push((key, self.value(depth + 1)?));
            if self.eat(b'}') {
                return Ok(Value::Obj(fields));
            }
            if !self.eat(b',') {
                return Err(self.fail("',' or '}' was expected"));
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        if self.bytes.get(self.at) != Some(&b'"') {
            return Err(self.fail("a string was expected"));
        }
        self.at += 1;
        let mut out = Vec::new();
        loop {
            let byte = *self
                .bytes
                .get(self.at)
                .ok_or_else(|| self.fail("unterminated string"))?;
            self.at += 1;
            match byte {
                b'"' => break,
                b'\\' => self.escape(&mut out)?,
                0..=0x1F => return Err(self.fail("control character in a string")),
                other => out.push(other),
            }
        }
        String::from_utf8(out).map_err(|_| self.fail("invalid UTF-8"))
    }

    fn escape(&mut self, out: &mut Vec<u8>) -> Result<(), String> {
        let code = *self
            .bytes
            .get(self.at)
            .ok_or_else(|| self.fail("unterminated escape"))?;
        self.at += 1;
        let plain = match code {
            b'"' | b'\\' | b'/' => code,
            b'b' => 8,
            b'f' => 12,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'u' => {
                let c = self.unicode()?;
                let mut buffer = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
                return Ok(());
            }
            _ => return Err(self.fail("unknown escape")),
        };
        out.push(plain);
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .and_then(|d| std::str::from_utf8(d).ok())
            .and_then(|d| u32::from_str_radix(d, 16).ok())
            .ok_or_else(|| self.fail("bad \\u escape"))?;
        self.at += 4;
        Ok(digits)
    }

    fn unicode(&mut self) -> Result<char, String> {
        let first = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&first) {
            if !self.bytes[self.at..].starts_with(b"\\u") {
                return Err(self.fail("lone surrogate"));
            }
            self.at += 2;
            let second = self.hex4()?;
            if !(0xDC00..0xE000).contains(&second) {
                return Err(self.fail("lone surrogate"));
            }
            0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
        } else {
            first
        };
        char::from_u32(code).ok_or_else(|| self.fail("bad code point"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::{obj, Json};

    #[test]
    fn test_parse_when_written_by_json_returns_the_same_values() {
        let written = obj([
            ("a", 1usize.into()),
            ("b", "say \"hi\"\\\n\u{2028}é".into()),
            ("c", vec![1u8, 2].into()),
            ("d", Json::Null),
            ("e", true.into()),
            ("f", (-5i64).into()),
        ])
        .render();

        let value = parse(&written).unwrap();

        assert_eq!(value.get("a").and_then(Value::as_int), Some(1));
        assert_eq!(
            value.get("b").and_then(Value::as_str),
            Some("say \"hi\"\\\n\u{2028}é")
        );
        assert_eq!(
            value.get("c").and_then(Value::as_array).map(<[_]>::len),
            Some(2)
        );
        assert_eq!(value.get("d"), Some(&Value::Null));
        assert_eq!(value.get("e").and_then(Value::as_bool), Some(true));
        assert_eq!(value.get("f").and_then(Value::as_int), Some(-5));
    }

    #[test]
    fn test_parse_when_unicode_escapes_returns_the_characters_including_surrogate_pairs() {
        let value = parse(r#""\u00e9\ud83c\udfb5""#).unwrap();

        assert_eq!(value.as_str(), Some("é\u{1F3B5}"));
    }

    #[test]
    fn test_parse_when_damaged_returns_an_error() {
        for text in [
            "",
            "{",
            "{\"a\":}",
            "[1,]",
            "{\"a\":1} x",
            "\"open",
            "1.5",
            "{\"a\" 1}",
            "\"\\ud800\"",
            "tru",
            "{\"a\":\"\u{1}\"}",
        ] {
            assert!(parse(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn test_parse_when_nested_too_deep_returns_an_error_instead_of_overflowing() {
        let deep = "[".repeat(10_000);

        assert!(parse(&deep).is_err());
    }

    #[test]
    fn test_parse_when_empty_containers_and_spaces_returns_them() {
        let value = parse(" { \"a\" : [ ] , \"b\" : { } } ").unwrap();

        assert_eq!(value.get("a"), Some(&Value::Arr(vec![])));
        assert_eq!(value.get("b"), Some(&Value::Obj(vec![])));
    }
}
