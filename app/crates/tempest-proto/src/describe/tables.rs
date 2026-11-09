//! Names read from the firmware tables of the repository, embedded at build time: the remote-panel keys and the
//! remote-panel controls. Linear scans: the tables hold under 60 rows and a label is built once per message.

const KEYS_TSV: &str = include_str!("../../../../../refs/panel_keys.tsv");
const CONTROLS_TSV: &str = include_str!("../../../../../refs/panel_controls.tsv");

/// The rows of a tab-separated table, header skipped, split into columns.
fn rows(tsv: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    tsv.lines().skip(1).map(|line| line.split('\t').collect())
}

/// Name of remote-panel key `code` (`refs/panel_keys.tsv`).
pub(super) fn key_name(code: u8) -> Option<&'static str> {
    rows(KEYS_TSV)
        .find(|cols| cols.get(1).and_then(|c| c.parse::<u8>().ok()) == Some(code))
        .and_then(|cols| cols.get(2).copied())
}

/// `encoder 3: Shape (Sample)` for NRPN `number` of the remote panel (`refs/panel_controls.tsv`).
pub(super) fn remote_control(number: u16) -> Option<String> {
    rows(CONTROLS_TSV)
        .find(|cols| cols.get(2).and_then(|c| c.parse::<u16>().ok()) == Some(number))
        .and_then(|cols| {
            Some(format!(
                "{} {}: {}",
                cols.first()?,
                cols.get(1)?,
                cols.get(3)?
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_name_when_code_in_table_returns_its_name_and_when_not_returns_none() {
        assert_eq!(key_name(0), Some("Playback (FX1)"));
        assert_eq!(key_name(49), None);
    }

    #[test]
    fn test_remote_control_when_known_number_returns_kind_index_and_name() {
        assert_eq!(remote_control(228).as_deref(), Some("absolute 8: Volume"));
        assert_eq!(
            remote_control(203).as_deref(),
            Some("encoder 3: Shape (Sample)")
        );
        assert_eq!(remote_control(214), None);
    }
}
