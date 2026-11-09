//! The JavaScript run in the page to deliver an event.

use crate::event::Event;

/// `window.Tempester.bridge.receive(<json>);`, or nothing harmful if the page has not loaded yet.
pub fn script_for(event: &Event) -> String {
    format!(
        "window.Tempester && window.Tempester.bridge && window.Tempester.bridge.receive({});",
        event.render()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_for_when_event_returns_a_call_with_the_json_as_literal() {
        let event = Event::new("nrpn").with("number", 34u16);

        assert_eq!(
            script_for(&event),
            r#"window.Tempester && window.Tempester.bridge && window.Tempester.bridge.receive({"event":"nrpn","number":34});"#
        );
    }

    #[test]
    fn test_script_for_when_text_tries_to_break_out_returns_it_inside_the_string() {
        let event = Event::new("saved").with("file", "\");alert(1);//\u{2028}");

        let js = script_for(&event);

        assert!(js.contains(r#"\");alert(1);//\u2028""#), "{js}");
        assert_eq!(
            js.matches(';').count(),
            2 + 1,
            "only the text's own semicolons and the final one: {js}"
        );
        assert!(!js.contains('\u{2028}'));
    }
}
