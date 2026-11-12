//! The subset of `atty`'s API that nih_log calls, on top of `std::io::IsTerminal`.

use std::io::IsTerminal;

/// A standard stream, as `atty` names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
    Stdin,
}

/// True when the stream is attached to a terminal.
pub fn is(stream: Stream) -> bool {
    match stream {
        Stream::Stdout => std::io::stdout().is_terminal(),
        Stream::Stderr => std::io::stderr().is_terminal(),
        Stream::Stdin => std::io::stdin().is_terminal(),
    }
}

/// True when the stream is not attached to a terminal.
pub fn isnt(stream: Stream) -> bool {
    !is(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isnt_when_any_stream_returns_the_opposite_of_is() {
        for stream in [Stream::Stdout, Stream::Stderr, Stream::Stdin] {
            assert_eq!(isnt(stream), !is(stream));
        }
    }
}
