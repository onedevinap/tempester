//! The page, embedded in the binary and served through the custom protocol: no file is read at run time, no
//! network is used. `/params.js` is generated from the parameter table of `tempest-proto`.

use std::borrow::Cow;

use tempest_proto::sound::params;

const HTML: &str = "text/html; charset=utf-8";
const JS: &str = "text/javascript; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";

struct Asset {
    path: &'static str,
    mime: &'static str,
    body: &'static str,
}

macro_rules! asset {
    ($path:literal, $mime:expr, $file:literal) => {
        Asset {
            path: $path,
            mime: $mime,
            body: include_str!(concat!("../../ui/", $file)),
        }
    };
}

static ASSETS: &[Asset] = &[
    asset!("/index.html", HTML, "index.html"),
    asset!("/app.css", CSS, "app.css"),
    asset!("/compact.css", CSS, "compact.css"),
    asset!("/library.css", CSS, "library.css"),
    asset!("/panel.css", CSS, "panel.css"),
    asset!("/panel-face.css", CSS, "panel-face.css"),
    asset!("/edit-tools.css", CSS, "edit-tools.css"),
    asset!("/monitor.css", CSS, "monitor.css"),
    asset!("/bridge.js", JS, "bridge.js"),
    asset!("/icons.js", JS, "icons.js"),
    asset!("/kit.js", JS, "kit.js"),
    asset!("/pad-notes.js", JS, "pad-notes.js"),
    asset!("/conn-bar.js", JS, "conn-bar.js"),
    asset!("/project-ui.js", JS, "project-ui.js"),
    asset!("/panel-labels.js", JS, "panel-labels.js"),
    asset!("/panel-keys.js", JS, "panel-keys.js"),
    asset!("/panel-remote.js", JS, "panel-remote.js"),
    asset!("/panel-view.js", JS, "panel-view.js"),
    asset!("/panel-mode.js", JS, "panel-mode.js"),
    asset!("/panel-link.js", JS, "panel-link.js"),
    asset!("/panel.js", JS, "panel.js"),
    asset!("/panel-face.js", JS, "panel-face.js"),
    asset!("/panel-machine.js", JS, "panel-machine.js"),
    asset!("/edit-learn.js", JS, "edit-learn.js"),
    asset!("/edit-tools.js", JS, "edit-tools.js"),
    asset!("/panel-perform.js", JS, "panel-perform.js"),
    asset!("/monitor-ui.js", JS, "monitor-ui.js"),
    asset!("/identity-panel.js", JS, "identity-panel.js"),
    asset!("/pad-notes-panel.js", JS, "pad-notes-panel.js"),
    asset!("/os-hash.js", JS, "os-hash.js"),
    asset!("/os-versions.js", JS, "os-versions.js"),
    asset!("/os-verify.js", JS, "os-verify.js"),
    asset!("/page-machine.js", JS, "page-machine.js"),
    asset!("/page-panel.js", JS, "page-panel.js"),
    asset!("/lib-model.js", JS, "lib-model.js"),
    asset!("/lib-dialogs.js", JS, "lib-dialogs.js"),
    asset!("/lib-flow.js", JS, "lib-flow.js"),
    asset!("/lib-kit.js", JS, "lib-kit.js"),
    asset!("/lib-browser.js", JS, "lib-browser.js"),
    asset!("/page-library.js", JS, "page-library.js"),
    asset!("/page-backup.js", JS, "page-backup.js"),
    asset!("/page-os.js", JS, "page-os.js"),
    asset!("/page-play.js", JS, "page-play.js"),
    asset!("/app.js", JS, "app.js"),
];

/// `window.TEMPEST_PARAMS = [[index, max, default], ...]` for the panel.
pub fn params_js() -> String {
    let rows: Vec<String> = params()
        .iter()
        .map(|p| format!("[{},{},{}]", p.index, p.max, p.default))
        .collect();
    format!("window.TEMPEST_PARAMS = [{}];\n", rows.join(","))
}

/// The asset for a URL path (`/` is the page): its media type and bytes.
pub fn lookup(path: &str) -> Option<(&'static str, Cow<'static, [u8]>)> {
    let path = if path == "/" { "/index.html" } else { path };
    if path == "/params.js" {
        return Some((JS, Cow::Owned(params_js().into_bytes())));
    }
    ASSETS
        .iter()
        .find(|asset| asset.path == path)
        .map(|asset| (asset.mime, Cow::Borrowed(asset.body.as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    fn ui_dir() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/ui"))
    }

    fn text(path: &str) -> String {
        lookup(path)
            .map(|(_, body)| String::from_utf8_lossy(&body).into_owned())
            .unwrap_or_default()
    }

    #[test]
    fn test_lookup_when_root_returns_the_page() {
        let (mime, body) = lookup("/").unwrap();

        assert_eq!(mime, HTML);
        assert!(String::from_utf8_lossy(&body).contains("<title>Tempester</title>"));
    }

    #[test]
    fn test_lookup_when_unknown_path_returns_none() {
        assert!(lookup("/nope.js").is_none());
        assert!(lookup("/../Cargo.toml").is_none());
    }

    #[test]
    fn test_assets_when_listed_return_every_file_of_the_ui_folder() {
        let on_disk: BTreeSet<String> = fs::read_dir(ui_dir())
            .unwrap()
            .map(|e| format!("/{}", e.unwrap().file_name().to_string_lossy()))
            .collect();
        let embedded: BTreeSet<String> = ASSETS.iter().map(|a| a.path.to_string()).collect();

        assert_eq!(
            embedded, on_disk,
            "a file of ui/ is not embedded, or the reverse"
        );
    }

    #[test]
    fn test_index_when_read_returns_only_embedded_files_in_its_tags() {
        let html = text("/index.html");
        let referenced: Vec<&str> = html
            .split(['"', '\''])
            .filter(|s| s.ends_with(".js") || s.ends_with(".css"))
            .collect();

        assert!(referenced.len() >= 10, "{referenced:?}");
        for file in referenced {
            assert!(
                lookup(&format!("/{file}")).is_some(),
                "{file} is not embedded"
            );
        }
    }

    #[test]
    fn test_index_when_read_returns_every_page_script_loaded_before_app_js() {
        let html = text("/index.html");
        let app = html.find("app.js").unwrap();

        for page in [
            "page-machine",
            "page-panel",
            "page-library",
            "page-backup",
            "page-os",
            "page-play",
        ] {
            let at = html.find(page).unwrap();
            assert!(at < app, "{page} must load before app.js");
        }
    }

    /// The page scripts of the old Sounds and Beats tabs that `html` still loads.
    fn old_bank_tabs_in(html: &str) -> Vec<&'static str> {
        ["page-sounds.js", "page-beats.js"]
            .into_iter()
            .filter(|script| html.contains(script))
            .collect()
    }

    #[test]
    fn test_index_when_read_returns_one_library_tab_and_no_sounds_or_beats_tab() {
        let html = text("/index.html");

        assert!(html.contains("page-library.js"));
        assert_eq!(old_bank_tabs_in(&html), Vec::<&str>::new());
        assert!(lookup("/page-sounds.js").is_none());
        assert!(lookup("/page-beats.js").is_none());
    }

    #[test]
    fn test_index_when_an_old_tab_is_put_back_returns_it_as_left() {
        let html = text("/index.html").replace(
            "<script src=\"page-library.js\"></script>",
            "<script src=\"page-sounds.js\"></script>",
        );

        assert_eq!(old_bank_tabs_in(&html), vec!["page-sounds.js"]);
    }

    #[test]
    fn test_assets_when_scanned_return_no_network_address() {
        for asset in ASSETS {
            // The SVG namespace is an identifier, not a request.
            let rest = asset.body.replace("http://www.w3.org/2000/svg", "");
            assert!(
                !rest.contains("http://") && !rest.contains("https://"),
                "{} mentions a URL",
                asset.path
            );
        }
    }

    #[test]
    fn test_params_js_when_generated_returns_all_127_rows_matching_the_table() {
        let js = params_js();

        assert_eq!(js.matches('[').count(), 128);
        assert!(
            js.starts_with("window.TEMPEST_PARAMS = [[0,120,36],[1,100,50]"),
            "{js}"
        );
        assert!(js.contains("[34,164,164]"));
    }

    #[test]
    fn test_panel_labels_when_read_returns_a_name_for_each_of_the_127_parameters() {
        let labels = text("/panel-labels.js");

        for index in 0..127 {
            assert!(
                labels.contains(&format!("\n    {index}: \"")),
                "no label for parameter {index}"
            );
        }
        assert!(!labels.contains("\n    127: \""));
    }

    const KEYS_TSV: &str = include_str!("../../../../../refs/panel_keys.tsv");

    /// One key line of `panel-keys.js`: `["name", code, [led, ...]],`.
    struct KeyLine {
        name: String,
        code: u8,
        leds: Vec<u8>,
    }

    fn key_lines(js: &str) -> Vec<KeyLine> {
        js.lines()
            .filter_map(|line| {
                let row = line.trim().strip_prefix("[\"")?.strip_suffix("]],")?;
                let (name, rest) = row.split_once("\", ")?;
                let (code, leds) = rest.split_once(", [")?;
                let leds = leds
                    .split(',')
                    .filter(|led| !led.trim().is_empty())
                    .map(|led| u8::from_str_radix(led.trim().trim_start_matches("0x"), 16))
                    .collect::<Result<_, _>>()
                    .ok()?;
                Some(KeyLine {
                    name: name.to_string(),
                    code: code.parse().ok()?,
                    leds,
                })
            })
            .collect()
    }

    /// The LEDs of key `code` in `refs/panel_keys.tsv`: `none`, `0x26+0x2B` or a single index.
    fn tsv_leds(tsv: &str, code: u8) -> Option<Vec<u8>> {
        let row = tsv
            .lines()
            .skip(1)
            .map(|line| line.split('\t').collect::<Vec<_>>())
            .find(|cols| cols[1].parse() == Ok(code))?;
        Some(
            row[4]
                .split('+')
                .filter_map(|led| u8::from_str_radix(led.trim_start_matches("0x"), 16).ok())
                .collect(),
        )
    }

    /// What is wrong in the key table of the page compared with the firmware table.
    fn key_table_problems(js: &str, tsv: &str) -> Vec<String> {
        let lines = key_lines(js);
        let mut problems = Vec::new();
        if lines.is_empty() {
            problems.push("no key line found".to_string());
        }
        for (at, line) in lines.iter().enumerate() {
            let twice = lines[..at]
                .iter()
                .any(|l| l.code == line.code || l.name == line.name);
            if twice {
                problems.push(format!("{}: name or code used twice", line.name));
            }
            match tsv_leds(tsv, line.code) {
                None => problems.push(format!(
                    "{}: key code {} is not in the table",
                    line.name, line.code
                )),
                Some(leds) if leds != line.leds => problems.push(format!(
                    "{}: LEDs {:?} but the firmware table says {leds:?}",
                    line.name, line.leds
                )),
                Some(_) => {}
            }
        }
        problems
    }

    #[test]
    fn test_panel_keys_when_compared_with_the_firmware_table_return_no_problem() {
        let js = text("/panel-keys.js");

        assert_eq!(key_table_problems(&js, KEYS_TSV), Vec::<String>::new());
        // Every key of the firmware (codes 0-48) has a line: the Full view draws 36, the Machine view the rest.
        assert_eq!(key_lines(&js).len(), 49);
    }

    #[test]
    fn test_panel_keys_when_a_led_is_wrong_returns_a_problem() {
        let js =
            text("/panel-keys.js").replace("[\"Click\", 22, [0x15]]", "[\"Click\", 22, [0x16]]");

        assert_ne!(js, text("/panel-keys.js"), "the variant must differ");
        assert_eq!(key_table_problems(&js, KEYS_TSV).len(), 1);
    }

    #[test]
    fn test_panel_keys_when_a_code_is_out_of_range_or_repeated_returns_problems() {
        let js = text("/panel-keys.js");

        let out_of_range = js.replace("[\"Copy\", 31,", "[\"Copy\", 49,");
        let repeated = js.replace("[\"Copy\", 31, [0x1f]]", "[\"Copy\", 30, [0x1E]]");
        assert!(
            js != out_of_range && js != repeated,
            "the variants must differ"
        );
        assert!(!key_table_problems(&out_of_range, KEYS_TSV).is_empty());
        assert!(!key_table_problems(&repeated, KEYS_TSV).is_empty());
    }

    #[test]
    fn test_panel_when_drawn_names_every_key_of_the_table_so_that_no_key_is_left_unbound() {
        let panel = text("/panel.js") + &text("/panel-machine.js") + &text("/panel-face.js");

        for line in key_lines(&text("/panel-keys.js")) {
            let literal = format!("\"{}\"", line.name);
            let templated = line.name.trim_end_matches(|c: char| c.is_ascii_digit());
            assert!(
                panel.contains(&literal) || panel.contains(&format!("{templated}${{n}}")),
                "{} is not drawn by panel.js",
                line.name
            );
        }
    }
}
