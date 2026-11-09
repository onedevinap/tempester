//! Events Rust sends to the page: `{"event": <name>, ...fields}` written by `json`.

use std::sync::Arc;

use crate::error::AppError;
use crate::json::Json;

/// One message to the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub name: &'static str,
    fields: Vec<(&'static str, Json)>,
}

impl Event {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            fields: Vec::new(),
        }
    }

    /// Adds a field (builder style).
    pub fn with(mut self, key: &'static str, value: impl Into<Json>) -> Self {
        self.fields.push((key, value.into()));
        self
    }

    /// The field `key`, if present.
    #[cfg(test)]
    pub fn field(&self, key: &str) -> Option<&Json> {
        self.fields
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value)
    }

    /// An `error` event for a failed operation; `during` names the operation for the page ("fetch_sound"...).
    pub fn error(during: &'static str, error: &AppError) -> Self {
        Self::new("error")
            .with("during", during)
            .with("code", error.code())
            .with("message", error.to_string())
    }

    /// The JSON text sent to the page.
    pub fn render(&self) -> String {
        let mut fields = vec![("event", Json::from(self.name))];
        fields.extend(self.fields.iter().cloned());
        Json::Obj(fields).render()
    }
}

/// Where events go: the web view in the app, a collector in tests. Called from several threads.
pub type Emitter = Arc<dyn Fn(Event) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_when_fields_added_returns_event_name_first() {
        let event = Event::new("sound").with("name", "Kick").with("n", 3usize);

        assert_eq!(event.render(), r#"{"event":"sound","name":"Kick","n":3}"#);
    }

    #[test]
    fn test_render_when_field_has_quotes_returns_escaped_text() {
        let event = Event::new("x").with("t", "a\"b");

        assert_eq!(event.render(), r#"{"event":"x","t":"a\"b"}"#);
    }

    #[test]
    fn test_error_when_timeout_returns_code_and_message() {
        let event = Event::error(
            "fetch_sound",
            &AppError::Midi(tempest_midi::MidiError::Timeout),
        );

        assert_eq!(event.field("code"), Some(&Json::from("timeout")));
        assert_eq!(event.field("during"), Some(&Json::from("fetch_sound")));
    }

    #[test]
    fn test_field_when_missing_returns_none() {
        assert_eq!(Event::new("x").field("nope"), None);
    }
}
