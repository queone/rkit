use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_swatch"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

#[test]
fn version_is_terminal_and_exact() {
    let output = run(&["--version"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"swatch v1.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_documents_primary_subcommands_and_aliases() {
    for args in [vec![], vec!["-h"], vec!["--help"], vec!["-?"]] {
        let output = run(&args);
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        for section in [
            "Usage",
            "Options",
            "Examples",
            "p, palette",
            "g, grid",
            "b, backgrounds",
        ] {
            assert!(text.contains(section), "missing {section:?}");
        }
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn every_primary_subcommand_matches_its_alias() {
    for (short, long) in [("p", "palette"), ("g", "grid"), ("b", "backgrounds")] {
        let short_output = run(&[short]);
        let long_output = run(&[long]);
        assert!(short_output.status.success());
        assert_eq!(short_output.stdout, long_output.stdout);
        assert_eq!(short_output.stderr, long_output.stderr);
    }
}

#[test]
fn color_policy_suppresses_sgr_for_nonterminal_and_unsupported_environments() {
    let cases = [
        (Some("1"), Some("xterm-256color"), Some("truecolor")),
        (None, Some("dumb"), Some("truecolor")),
        (None, Some("xterm"), None),
        (None, None, None),
    ];
    for (no_color, term, colorterm) in cases {
        let mut command = Command::new(env!("CARGO_BIN_EXE_swatch"));
        command
            .arg("p")
            .env_remove("NO_COLOR")
            .env_remove("TERM")
            .env_remove("COLORTERM");
        if let Some(value) = no_color {
            command.env("NO_COLOR", value);
        }
        if let Some(value) = term {
            command.env("TERM", value);
        }
        if let Some(value) = colorterm {
            command.env("COLORTERM", value);
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert!(!String::from_utf8(output.stdout).unwrap().contains("\x1b["));
    }
}

#[test]
fn plain_palette_contains_every_section_and_ramp_boundary() {
    let output = run(&["p"]);
    let text = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "Standard 16-color palette (0-15)",
        "256-color cube (16-231)",
        "Grayscale ramp (232-255)",
        "Hue ramps",
        "Heat ramp",
        "Gra",
        "Whi",
        " 0:196",
        "10:46",
    ] {
        assert!(text.contains(expected), "missing {expected:?}");
    }
    assert!(!text.contains("\x1b["));
}

#[test]
fn grid_and_backgrounds_preserve_plain_layout_and_tokens() {
    let grid = run(&["g", "HEADER"]);
    let grid_text = String::from_utf8(grid.stdout).unwrap();
    assert!(grid_text.contains("┌"));
    assert!(grid_text.contains("GraX"));
    assert!(grid_text.contains("HeatX"));
    assert!(grid_text.contains("5*"));
    assert!(grid_text.contains("HEADER"));

    let backgrounds = run(&["b", "LABEL"]);
    let bg_text = String::from_utf8(backgrounds.stdout).unwrap();
    for family in [
        "BgGra", "BgRed", "BgOrg", "BgYel", "BgGrn", "BgCya", "BgBlu", "BgPur", "BgMag", "BgWhi",
        "BgHeat",
    ] {
        assert!(bg_text.contains(family));
    }
    assert!(bg_text.contains("LABEL"));
    assert!(!bg_text.contains("\x1b["));
}

#[test]
fn invalid_arguments_exit_two_with_context() {
    for args in [
        vec!["wat"],
        vec!["p", "extra"],
        vec!["g", "--foreground", "15"],
        vec!["grid", "--foreground", "wat", "TOKEN"],
        vec!["b", "--foreground", "256"],
        vec!["backgrounds", "--bad"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.starts_with("swatch: "));
        assert!(error.contains("swatch --help"));
    }
}

#[test]
fn documentation_describes_the_self_contained_command() {
    let readme = include_str!("../README.md");
    let arch = include_str!("../arch.md");
    for text in [readme, arch] {
        assert!(text.contains("swatch"));
        assert!(text.contains("p, palette"));
        assert!(text.contains("g, grid"));
        assert!(text.contains("b, backgrounds"));
    }
}
