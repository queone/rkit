//! Xterm palette and rkit ramp inspection for the `swatch` utility.

use crate::color::ColorMode;
use std::ffi::OsString;
use std::io::Write;

const PROGRAM_NAME: &str = "swatch";
const WHITE: &str = "38;5;15";
const DEFAULT_TOKEN: &str = "TOKEN";

#[derive(Clone, Copy)]
struct Ramp {
    name: &'static str,
    indices: [u8; 11],
}

const RAMPS: [Ramp; 11] = [
    Ramp {
        name: "Gra",
        indices: [16, 233, 236, 239, 242, 245, 248, 251, 254, 255, 231],
    },
    Ramp {
        name: "Red",
        indices: [16, 52, 88, 124, 160, 196, 203, 210, 217, 224, 231],
    },
    Ramp {
        name: "Org",
        indices: [16, 88, 130, 166, 172, 208, 214, 215, 222, 229, 230],
    },
    Ramp {
        name: "Yel",
        indices: [16, 58, 100, 142, 184, 220, 226, 227, 228, 229, 230],
    },
    Ramp {
        name: "Grn",
        indices: [16, 22, 28, 34, 40, 46, 83, 120, 157, 194, 231],
    },
    Ramp {
        name: "Cya",
        indices: [16, 23, 30, 37, 44, 51, 87, 123, 159, 195, 231],
    },
    Ramp {
        name: "Blu",
        indices: [16, 17, 18, 19, 20, 21, 27, 33, 75, 111, 195],
    },
    Ramp {
        name: "Pur",
        indices: [16, 53, 54, 55, 91, 92, 99, 105, 141, 177, 219],
    },
    Ramp {
        name: "Mag",
        indices: [16, 53, 90, 127, 164, 201, 207, 213, 219, 225, 231],
    },
    Ramp {
        name: "Whi",
        indices: [16, 59, 102, 145, 188, 231, 252, 253, 254, 255, 15],
    },
    Ramp {
        name: "Heat",
        indices: [196, 202, 208, 214, 220, 226, 190, 154, 118, 82, 46],
    },
];

enum Command {
    Palette,
    Grid {
        token: String,
        reverse: bool,
        foreground: Option<u8>,
    },
    Backgrounds {
        token: String,
        foreground: Option<u8>,
    },
}

/// Runs `swatch` and writes its process output to the supplied streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    run_with_color(args, version, stdout, stderr, ColorMode::detect_stdout())
}

fn run_with_color<I, S, W, E>(
    args: I,
    version: &str,
    stdout: &mut W,
    stderr: &mut E,
    color: ColorMode,
) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if args.is_empty() {
        let _ = write!(stdout, "{}", help(color, version));
        return 0;
    }
    if args.len() == 1 && is_version(&args[0]) {
        let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
        return 0;
    }
    if args.len() == 1 && is_help(&args[0]) {
        let _ = write!(stdout, "{}", help(color, version));
        return 0;
    }
    if args.iter().any(is_version) {
        return diagnostic(
            stderr,
            "version flag cannot be combined with a subcommand or operand",
        );
    }
    if args.iter().any(is_help) {
        return diagnostic(
            stderr,
            "help flag cannot be combined with a subcommand or operand",
        );
    }

    match parse_command(&args) {
        Ok(Command::Palette) => render_palette(stdout, color),
        Ok(Command::Grid {
            token,
            reverse,
            foreground,
        }) => render_grid(stdout, color, &token, reverse, foreground),
        Ok(Command::Backgrounds { token, foreground }) => {
            render_backgrounds(stdout, color, &token, foreground)
        }
        Err(message) => return diagnostic(stderr, &message),
    }
    0
}

fn parse_command(args: &[OsString]) -> Result<Command, String> {
    let name = args[0]
        .to_str()
        .ok_or_else(|| "subcommand is not valid UTF-8; use p, g, or b".to_owned())?;
    match name {
        "p" | "palette" => {
            if args.len() == 1 {
                Ok(Command::Palette)
            } else {
                Err(format!("{name}: does not accept options or operands"))
            }
        }
        "g" | "grid" => parse_grid(name, &args[1..]),
        "b" | "backgrounds" => parse_backgrounds(name, &args[1..]),
        _ => Err(format!("unknown subcommand {name:?}; use p, g, or b")),
    }
}

fn parse_grid(name: &str, args: &[OsString]) -> Result<Command, String> {
    let mut reverse = false;
    let mut foreground = None;
    let mut token = None;
    let mut index = 0;
    while index < args.len() {
        let value = text_arg(name, &args[index])?;
        match value {
            "-r" | "--reverse" => reverse = true,
            "-f" | "--foreground" => {
                if foreground.is_some() {
                    return Err(format!("{name}: foreground option may be used only once"));
                }
                index += 1;
                let next = args.get(index).ok_or_else(|| {
                    format!("{name}: {value} requires an xterm index from 0 through 255")
                })?;
                foreground = Some(parse_index(name, text_arg(name, next)?)?);
            }
            _ if value.starts_with('-') => return Err(format!("{name}: unknown option {value:?}")),
            _ if token.is_none() => token = Some(value.to_owned()),
            _ => {
                return Err(format!(
                    "{name}: extra operand {value:?}; provide at most one token"
                ));
            }
        }
        index += 1;
    }
    if foreground.is_some() && !reverse {
        return Err(format!("{name}: --foreground requires --reverse"));
    }
    Ok(Command::Grid {
        token: normalized_token(token),
        reverse,
        foreground,
    })
}

fn parse_backgrounds(name: &str, args: &[OsString]) -> Result<Command, String> {
    let mut foreground = None;
    let mut token = None;
    let mut index = 0;
    while index < args.len() {
        let value = text_arg(name, &args[index])?;
        match value {
            "-f" | "--foreground" => {
                if foreground.is_some() {
                    return Err(format!("{name}: foreground option may be used only once"));
                }
                index += 1;
                let next = args.get(index).ok_or_else(|| {
                    format!("{name}: {value} requires an xterm index from 0 through 255")
                })?;
                foreground = Some(parse_index(name, text_arg(name, next)?)?);
            }
            _ if value.starts_with('-') => return Err(format!("{name}: unknown option {value:?}")),
            _ if token.is_none() => token = Some(value.to_owned()),
            _ => {
                return Err(format!(
                    "{name}: extra operand {value:?}; provide at most one token"
                ));
            }
        }
        index += 1;
    }
    Ok(Command::Backgrounds {
        token: normalized_token(token),
        foreground,
    })
}

fn text_arg<'a>(name: &str, value: &'a OsString) -> Result<&'a str, String> {
    value
        .to_str()
        .ok_or_else(|| format!("{name}: argument is not valid UTF-8; use a UTF-8 token"))
}

fn parse_index(name: &str, value: &str) -> Result<u8, String> {
    value.parse::<u8>().map_err(|_| {
        format!("{name}: foreground index {value:?} must be an integer from 0 through 255")
    })
}

fn normalized_token(token: Option<String>) -> String {
    match token {
        Some(value) if !value.is_empty() => value,
        _ => DEFAULT_TOKEN.to_owned(),
    }
}

fn render_palette<W: Write>(out: &mut W, color: ColorMode) {
    let _ = writeln!(out, "Xterm 256-color palette");
    let _ = writeln!(out);
    let _ = writeln!(out, "Standard 16-color palette (0-15):");
    for index in 0u16..16 {
        let block = color.paint(&format!("38;5;{index}"), "███");
        let _ = write!(out, "  {block} {index:<3}");
        if (index + 1) % 8 == 0 {
            let _ = writeln!(out);
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "256-color cube (16-231) — index = 16 + 36r + 6g + b:");
    for red in 0u16..6 {
        for green in 0u16..6 {
            let _ = write!(out, "  r={red} g={green}  ");
            for blue in 0u16..6 {
                let index = 16 + 36 * red + 6 * green + blue;
                let block = color.paint(&format!("38;5;{index}"), "███");
                let _ = write!(out, " {block} {index:<3}");
            }
            let _ = writeln!(out);
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Grayscale ramp (232-255):");
    for index in 232u16..=255 {
        let block = color.paint(&format!("38;5;{index}"), "███");
        let _ = write!(out, "  {block} {index:<3}");
        if (index - 231) % 6 == 0 {
            let _ = writeln!(out);
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Hue ramps (Hue0 dim → Hue5 canonical → Hue10 bright):");
    for ramp in &RAMPS[..10] {
        let _ = write!(out, "  {:<3} ", ramp.name);
        for (step, index) in ramp.indices.iter().enumerate() {
            let block = color.paint(&format!("38;5;{index}"), "███");
            let _ = write!(out, " {block}{step:>2}:{index:<3}");
        }
        let _ = writeln!(out);
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Heat ramp 0-10 (red → yellow → green):");
    let _ = write!(out, "  ");
    for (step, index) in RAMPS[10].indices.iter().enumerate() {
        let block = color.paint(&format!("38;5;{index}"), "███");
        let _ = write!(out, " {block}{step:>2}:{index:<3}");
    }
    let _ = writeln!(out);
}

fn render_grid<W: Write>(
    out: &mut W,
    color: ColorMode,
    token: &str,
    reverse: bool,
    foreground: Option<u8>,
) {
    let cell_width = token.chars().count().max(5);
    let label_dash = "─".repeat(4);
    let cell_dash = "─".repeat(cell_width + 2);
    let top = format!(
        "┌{label_dash}{}┐",
        format!("┬{cell_dash}").repeat(RAMPS.len())
    );
    let middle = format!(
        "├{label_dash}{}┤",
        format!("┼{cell_dash}").repeat(RAMPS.len())
    );
    let bottom = format!(
        "└{label_dash}{}┘",
        format!("┴{cell_dash}").repeat(RAMPS.len())
    );
    let _ = writeln!(out, "{top}");
    let _ = write!(out, "│    ");
    for ramp in &RAMPS {
        let _ = write!(
            out,
            "│ {:<width$} ",
            format!("{}X", ramp.name),
            width = cell_width
        );
    }
    let _ = writeln!(out, "│\n{middle}");
    for step in 0..=10 {
        let label = if step == 5 {
            "5*".to_owned()
        } else {
            step.to_string()
        };
        let _ = write!(out, "│ {label:<2} ");
        for ramp in &RAMPS {
            let text = format!(" {token:<width$} ", width = cell_width);
            let code = if !reverse {
                format!("38;5;{}", ramp.indices[step])
            } else if let Some(fg) = foreground {
                format!("48;5;{};38;5;{fg}", ramp.indices[step])
            } else {
                format!("48;5;{}", ramp.indices[step])
            };
            let _ = write!(out, "│{}", color.paint(&code, &text));
        }
        let _ = writeln!(out, "│");
    }
    let _ = writeln!(out, "{bottom}");
}

fn render_backgrounds<W: Write>(
    out: &mut W,
    color: ColorMode,
    token: &str,
    foreground: Option<u8>,
) {
    let width = token.chars().count();
    let _ = writeln!(out, "Background ramp swatches (steps 0..10):");
    for ramp in &RAMPS {
        let _ = write!(out, "  {:<6} ", format!("Bg{}", ramp.name));
        for index in ramp.indices {
            let fg = foreground.unwrap_or_else(|| contrast_foreground(index));
            let text = format!(" {token:<width$} ");
            let _ = write!(
                out,
                "{}",
                color.paint(&format!("48;5;{index};38;5;{fg}"), &text)
            );
        }
        let _ = writeln!(out);
    }
}

fn contrast_foreground(index: u8) -> u8 {
    let (red, green, blue) = match index {
        232..=255 => {
            let level = 8 + u32::from(index - 232) * 10;
            (level, level, level)
        }
        16..=231 => {
            let value = u32::from(index - 16);
            let levels = [0u32, 95, 135, 175, 215, 255];
            (
                levels[(value / 36) as usize],
                levels[((value % 36) / 6) as usize],
                levels[(value % 6) as usize],
            )
        }
        0..=6 => return 15,
        _ => return 16,
    };
    let luminance = (299 * red + 587 * green + 114 * blue) / 1000;
    if luminance < 128 { 15 } else { 16 }
}

fn is_version(value: &OsString) -> bool {
    value == "-v" || value == "--version"
}
fn is_help(value: &OsString) -> bool {
    value == "-?" || value == "-h" || value == "--help"
}

fn diagnostic<E: Write>(stderr: &mut E, message: &str) -> u8 {
    let _ = writeln!(
        stderr,
        "{PROGRAM_NAME}: {message}; run 'swatch --help' for usage"
    );
    2
}

fn help(color: ColorMode, version: &str) -> String {
    let name = color.paint(WHITE, PROGRAM_NAME);
    let usage = color.paint(WHITE, "Usage");
    let options = color.paint(WHITE, "Options");
    let examples = color.paint(WHITE, "Examples");
    format!(
        "{name} v{version}\nXterm palette and rkit ramp inspector\n\
{usage}\n  {name} <subcommand> [options] [TOKEN]\n\
\nSubcommands\n  p, palette       Print the complete xterm palette and rkit ramps\n  g, grid          Print the bordered ramp-by-step grid\n  b, backgrounds   Print background-ramp swatch rows\n\
\n{options}\n  -r, --reverse          Use ramp colors as grid cell backgrounds\n  -f, --foreground INDEX Set grid or swatch text to xterm INDEX (0-255)\n  -v, --version          Print version and exit\n  -?, -h, --help         Print this usage page\n\
\n  TOKEN defaults to TOKEN when omitted or empty. Grid --foreground requires --reverse.\n\
\n{examples}\n  swatch p\n  swatch palette\n  swatch g --reverse HEADER\n  swatch b --foreground 15 LABEL\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoke(args: &[&str], enabled: bool) -> (u8, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with_color(
            args.iter().copied(),
            "1.0.0",
            &mut stdout,
            &mut stderr,
            ColorMode::new(enabled),
        );
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    #[test]
    fn aliases_render_identically() {
        for (short, long) in [("p", "palette"), ("g", "grid"), ("b", "backgrounds")] {
            assert_eq!(invoke(&[short], false), invoke(&[long], false));
        }
    }

    #[test]
    fn ramp_table_matches_all_ordered_indices() {
        let expected = [
            [16, 233, 236, 239, 242, 245, 248, 251, 254, 255, 231],
            [16, 52, 88, 124, 160, 196, 203, 210, 217, 224, 231],
            [16, 88, 130, 166, 172, 208, 214, 215, 222, 229, 230],
            [16, 58, 100, 142, 184, 220, 226, 227, 228, 229, 230],
            [16, 22, 28, 34, 40, 46, 83, 120, 157, 194, 231],
            [16, 23, 30, 37, 44, 51, 87, 123, 159, 195, 231],
            [16, 17, 18, 19, 20, 21, 27, 33, 75, 111, 195],
            [16, 53, 54, 55, 91, 92, 99, 105, 141, 177, 219],
            [16, 53, 90, 127, 164, 201, 207, 213, 219, 225, 231],
            [16, 59, 102, 145, 188, 231, 252, 253, 254, 255, 15],
            [196, 202, 208, 214, 220, 226, 190, 154, 118, 82, 46],
        ];
        assert_eq!(RAMPS.map(|ramp| ramp.indices), expected);
    }

    #[test]
    fn injected_color_covers_grid_and_background_contrast() {
        let (_, default_grid, _) = invoke(&["g", "--reverse"], true);
        assert!(default_grid.contains("\x1b[48;5;16m"));
        assert!(!default_grid.contains("\x1b[48;5;16;38;5;"));

        let (_, grid, _) = invoke(&["g", "--reverse", "--foreground", "15"], true);
        assert!(grid.contains("\x1b[48;5;16;38;5;15m"));
        let (_, backgrounds, _) = invoke(&["b"], true);
        assert!(backgrounds.contains("\x1b[48;5;16;38;5;15m"));
        assert!(backgrounds.contains("\x1b[48;5;231;38;5;16m"));

        assert_eq!(contrast_foreground(0), 15);
        assert_eq!(contrast_foreground(15), 16);
        assert_eq!(contrast_foreground(16), 15);
        assert_eq!(contrast_foreground(231), 16);
        assert_eq!(contrast_foreground(232), 15);
        assert_eq!(contrast_foreground(255), 16);

        let (_, overridden, _) = invoke(&["b", "--foreground", "7"], true);
        assert!(overridden.contains("\x1b[48;5;16;38;5;7m"));
    }

    #[test]
    fn plain_output_preserves_layout_and_default_token() {
        let (_, grid, _) = invoke(&["g", ""], false);
        assert!(grid.contains("GraX"));
        assert!(grid.contains("HeatX"));
        assert!(grid.contains("5*"));
        assert!(grid.contains(DEFAULT_TOKEN));
        assert!(!grid.contains("\x1b["));
        assert_eq!(grid.matches("│ 5* ").count(), 1);
        assert_eq!(
            grid.lines().filter(|line| line.starts_with('│')).count(),
            12
        );

        let (_, backgrounds, _) = invoke(&["b", "SAMPLE"], false);
        assert_eq!(backgrounds.lines().count(), 12);
        for ramp in RAMPS {
            let line = backgrounds
                .lines()
                .find(|line| line.contains(&format!("Bg{}", ramp.name)))
                .unwrap();
            assert_eq!(line.matches("SAMPLE").count(), 11);
        }
    }

    #[test]
    fn invalid_arguments_are_contextual_status_two_errors() {
        for args in [
            vec!["unknown"],
            vec!["p", "extra"],
            vec!["g", "-f", "15"],
            vec!["b", "-f", "256"],
            vec!["g", "--bad"],
        ] {
            let (code, stdout, stderr) = invoke(&args, false);
            assert_eq!(code, 2);
            assert!(stdout.is_empty());
            assert!(stderr.starts_with("swatch: "));
            assert!(stderr.contains("swatch --help"));
        }
    }
}
