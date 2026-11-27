use std::process::Command;

#[test]
fn help_flags_print_usage_without_a_terminal() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_archoxide"))
            .arg(flag)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{flag} exited with {}",
            output.status
        );
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("Usage: archoxide [OPTIONS]"));
        assert!(help.contains("-h, --help"));
        assert!(help.contains("--kitty"));
    }
}

#[test]
fn invalid_arguments_point_to_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_archoxide"))
        .arg("--unknown-option")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("archoxide --help")
    );
}
