//! The helper's wire protocol, pure and shared: the x64 helper and the ARM64 controller both
//! compile this file, so they cannot disagree, and malformed input has tests.
//!
//! Four commands, one line each, ASCII only:
//!
//! ```text
//! HOOK <thread id>   PRUNE   STOPALL   QUIT
//! ```
//!
//! Replies: `OK`, `ERR <reason>`, `HOOKS <n>`. Lines are short, the buffer is bounded, and
//! anything unexpected is an error: there is no "best effort" parse of garbage.

#![allow(dead_code)]

pub const MAX_LINE: usize = 64;
pub const MAX_PENDING: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Hook(u32),
    Prune,
    StopAll,
    Quit,
}

pub fn parse_command(line: &str) -> Result<Command, &'static str> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.len() > MAX_LINE {
        return Err("too-long");
    }
    if !line.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
        return Err("not-ascii");
    }
    let mut parts = line.split(' ');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("HOOK"), Some(id), None) => {
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
                return Err("bad-thread");
            }
            match id.parse::<u32>() {
                Ok(0) | Err(_) => Err("bad-thread"),
                Ok(thread) => Ok(Command::Hook(thread)),
            }
        }
        (Some("PRUNE"), None, None) => Ok(Command::Prune),
        (Some("STOPALL"), None, None) => Ok(Command::StopAll),
        (Some("QUIT"), None, None) => Ok(Command::Quit),
        _ => Err("unknown-command"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Ok,
    Err(String),
    Hooks(usize),
}

impl Reply {
    pub fn to_line(&self) -> String {
        match self {
            Reply::Ok => "OK".to_string(),
            Reply::Err(reason) => format!("ERR {reason}"),
            Reply::Hooks(count) => format!("HOOKS {count}"),
        }
    }

    pub fn parse(line: &str) -> Option<Reply> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.len() > MAX_LINE || !line.is_ascii() {
            return None;
        }
        if line == "OK" {
            return Some(Reply::Ok);
        }
        if let Some(reason) = line.strip_prefix("ERR ") {
            return Some(Reply::Err(reason.to_string()));
        }
        if let Some(count) = line.strip_prefix("HOOKS ") {
            return count.parse().ok().map(Reply::Hooks);
        }
        None
    }
}

/// Collects bytes into lines without ever holding more than [`MAX_PENDING`] of them.
#[derive(Default)]
pub struct LineBuffer {
    pending: Vec<u8>,
}

impl LineBuffer {
    /// Add bytes; get back every complete line. `Err` means the peer is not speaking this
    /// protocol (a line longer than the limit) and the connection should be dropped.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, &'static str> {
        self.pending.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            if line.len() > MAX_LINE + 2 {
                return Err("line-too-long");
            }
            lines.push(String::from_utf8_lossy(&line).into_owned());
        }
        if self.pending.len() > MAX_PENDING {
            return Err("no-newline");
        }
        Ok(lines)
    }
}

/// Retry policy after the helper fails to start or stops answering: wait, then wait longer,
/// up to a cap. Pure, so "helper failure" has a test.
#[derive(Debug, Default)]
pub struct Backoff {
    failures: u32,
    retry_at: Option<u64>,
}

impl Backoff {
    const STEPS_MS: [u64; 4] = [60_000, 120_000, 300_000, 600_000];

    pub const fn new() -> Self {
        Self {
            failures: 0,
            retry_at: None,
        }
    }

    pub fn fail(&mut self, now: u64) {
        let step = (self.failures as usize).min(Self::STEPS_MS.len() - 1);
        self.retry_at = Some(now.saturating_add(Self::STEPS_MS[step]));
        self.failures = self.failures.saturating_add(1);
    }

    pub fn succeed(&mut self) {
        self.failures = 0;
        self.retry_at = None;
    }

    pub fn allowed(&self, now: u64) -> bool {
        self.retry_at.map_or(true, |at| now >= at)
    }
}

/// Whether work may be sent to the helper at all. A disabled or missing helper must mean
/// "nothing is sent and nothing is started".
pub fn may_use_helper(option_enabled: bool, installed: bool, backoff: &Backoff, now: u64) -> bool {
    option_enabled && installed && backoff.allowed(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_commands_parse_and_nothing_else_does() {
        assert_eq!(parse_command("HOOK 1234"), Ok(Command::Hook(1234)));
        assert_eq!(parse_command("HOOK 1234\r\n"), Ok(Command::Hook(1234)));
        assert_eq!(parse_command("PRUNE"), Ok(Command::Prune));
        assert_eq!(parse_command("STOPALL"), Ok(Command::StopAll));
        assert_eq!(parse_command("QUIT"), Ok(Command::Quit));
    }

    #[test]
    fn malformed_commands_are_refused() {
        for bad in [
            "",
            " ",
            "hook 4",
            "Hook 4",
            "HOOK",
            "HOOK ",
            "HOOK  4",
            " HOOK 4",
            "HOOK 4 ",
            "HOOK 4 5",
            "HOOK 0",
            "HOOK -1",
            "HOOK +4",
            "HOOK 0x10",
            "HOOK 4294967296",
            "HOOK 1e3",
            "HOOK ４",
            "QUIT now",
            "QUIT\0",
            "PRUNE 1",
            "STOPALL\tNOW",
            "LOAD C:\\evil.dll",
            "EXEC calc.exe",
            "HOOK 5\u{7}",
            "héllo",
        ] {
            assert!(parse_command(bad).is_err(), "should refuse {bad:?}");
        }
        assert_eq!(parse_command(&"A".repeat(500)), Err("too-long"));
    }

    #[test]
    fn there_is_no_command_that_names_a_file_a_program_or_a_path() {
        // The surface is closed: any verb that is not one of the four is refused outright.
        for verb in ["LOAD", "RUN", "EXEC", "DLL", "INJECT", "OPEN", "CMD"] {
            assert_eq!(parse_command(&format!("{verb} 1")), Err("unknown-command"));
        }
    }

    #[test]
    fn replies_round_trip_and_garbage_is_not_parsed() {
        for reply in [
            Reply::Ok,
            Reply::Err("no-dialog".into()),
            Reply::Hooks(0),
            Reply::Hooks(7),
        ] {
            assert_eq!(Reply::parse(&reply.to_line()), Some(reply));
        }
        for bad in [
            "",
            "okay",
            "HOOKS x",
            "HOOKS -1",
            "ERR",
            "ÖK",
            &"OK".repeat(100),
        ] {
            assert_eq!(Reply::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn lines_are_assembled_across_chunks_and_bounded() {
        let mut buffer = LineBuffer::default();
        assert_eq!(buffer.push(b"HOO").unwrap(), Vec::<String>::new());
        assert_eq!(
            buffer.push(b"K 7\nPRU").unwrap(),
            vec!["HOOK 7\n".to_string()]
        );
        assert_eq!(
            buffer.push(b"NE\nQUIT\n").unwrap(),
            vec!["PRUNE\n".to_string(), "QUIT\n".to_string()]
        );
        // Endless data without a newline is a protocol violation, not a memory leak.
        let mut hostile = LineBuffer::default();
        let mut result = Ok(Vec::new());
        for _ in 0..40 {
            result = hostile.push(&[b'A'; 32]);
            if result.is_err() {
                break;
            }
        }
        assert_eq!(result, Err("no-newline"));
        // A complete but overlong line is refused too.
        let mut long = LineBuffer::default();
        let mut line = vec![b'A'; MAX_LINE + 20];
        line.push(b'\n');
        assert_eq!(long.push(&line), Err("line-too-long"));
    }

    #[test]
    fn backoff_grows_is_capped_and_resets() {
        let mut backoff = Backoff::default();
        assert!(backoff.allowed(0));
        backoff.fail(1_000);
        assert!(!backoff.allowed(1_000 + 59_999));
        assert!(backoff.allowed(1_000 + 60_000));
        backoff.fail(100_000);
        assert!(!backoff.allowed(100_000 + 119_999));
        backoff.fail(1_000_000);
        backoff.fail(2_000_000);
        backoff.fail(3_000_000);
        // Capped at ten minutes however many failures.
        assert!(backoff.allowed(3_000_000 + 600_000));
        backoff.succeed();
        assert!(backoff.allowed(0));
    }

    #[test]
    fn a_disabled_missing_or_backing_off_helper_is_never_used() {
        let mut backoff = Backoff::default();
        assert!(may_use_helper(true, true, &backoff, 0));
        assert!(!may_use_helper(false, true, &backoff, 0), "option off");
        assert!(!may_use_helper(true, false, &backoff, 0), "not installed");
        backoff.fail(0);
        assert!(
            !may_use_helper(true, true, &backoff, 10),
            "backing off after a failure"
        );
        assert!(may_use_helper(true, true, &backoff, 60_000));
    }
}
