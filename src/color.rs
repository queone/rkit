use std::env;
use std::io::{self, IsTerminal};

// Shared 256-color ANSI codes, sourced from governa-color. Used by `jy`;
// add more as future utilities need them.
pub(crate) const BLUE5: &str = "38;5;21";
pub(crate) const BLUE7: &str = "38;5;33";
pub(crate) const GRAY5: &str = "38;5;245";
pub(crate) const GREEN3: &str = "38;5;34";
pub(crate) const GREEN5: &str = "38;5;46";
pub(crate) const MAGENTA5: &str = "38;5;201";
pub(crate) const RED3: &str = "38;5;124";
pub(crate) const WHITE5: &str = "38;5;231";
pub(crate) const WHITE10: &str = "38;5;15";
pub(crate) const YELLOW5: &str = "38;5;220";

#[derive(Clone, Copy)]
pub(crate) struct ColorMode(bool);

impl ColorMode {
    pub(crate) fn detect_stdout() -> Self {
        let terminal = io::stdout().is_terminal();
        let term = env::var_os("TERM").unwrap_or_default();
        let colorterm = env::var_os("COLORTERM").unwrap_or_default();
        let term_text = term.to_string_lossy();
        let colorterm_text = colorterm.to_string_lossy();
        let supports_256 = term_text.contains("256color")
            || colorterm_text == "truecolor"
            || colorterm_text == "24bit";
        Self(terminal && supports_256 && env::var_os("NO_COLOR").is_none() && term_text != "dumb")
    }

    #[cfg(test)]
    pub(crate) const fn new(enabled: bool) -> Self {
        Self(enabled)
    }

    pub(crate) const fn enabled(self) -> bool {
        self.0
    }

    pub(crate) fn paint(self, code: &str, text: &str) -> String {
        if self.enabled() {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }
}
