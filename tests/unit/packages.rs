use std::os::unix::process::ExitStatusExt;

use super::*;

#[test]
fn selected_upgrade_ignores_other_installed_packages_without_refreshing() {
    let installed = ["alpha", "beta", "gamma"]
        .map(str::to_owned)
        .into_iter()
        .collect();
    let names = vec!["beta".to_owned()];
    for aur in [false, true] {
        let command = selected_upgrade_command(aur, &names, &installed).unwrap();
        let args: Vec<_> = command.get_args().collect();
        if aur {
            assert_eq!(command.get_program(), "yay");
            assert_eq!(args, ["-Su", "--ignore", "alpha,gamma"]);
        } else {
            assert_eq!(command.get_program(), "sudo");
            assert_eq!(args, ["pacman", "-Su", "--ignore", "alpha,gamma"]);
        }
    }
    assert!(selected_upgrade_command(false, &[], &installed).is_err());
    assert!(selected_upgrade_command(false, &["missing".into()], &installed).is_err());
}

#[test]
fn update_except_refreshes_with_validated_temporary_exclusions() {
    for aur in [false, true] {
        let command = Transaction::UpgradeExcept {
            aur,
            names: vec!["alpha".into(), "beta".into()],
        }
        .native_command()
        .unwrap();
        let args: Vec<_> = command.get_args().collect();
        let expected = if aur {
            &["-Syu", "--ignore", "alpha,beta"][..]
        } else {
            &["pacman", "-Syu", "--ignore", "alpha,beta"][..]
        };
        assert_eq!(args, expected);
    }
    for names in [vec![], vec!["alpha,beta".into()], vec!["--nodeps".into()]] {
        assert!(
            Transaction::UpgradeExcept { aur: false, names }
                .native_command()
                .is_err()
        );
    }
}

fn command_output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
    Output {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
    }
}

#[test]
fn parses_installed_versions_and_foreign_names() {
    let installed = parse_installed(b"alsa-lib 1.2.15.3-1\nyay 2:13.1.0-1\n").unwrap();
    assert_eq!(installed[0].name, "alsa-lib");
    assert_eq!(installed[1].version, "2:13.1.0-1");
    assert_eq!(installed[1].availability, Availability::Unknown);
    assert_eq!(
        parse_package_names(b"yay\n", "pacman -Qqm").unwrap(),
        HashSet::from(["yay".into()])
    );
    assert!(parse_package_names(b"", "pacman -Qqm").unwrap().is_empty());
}

#[test]
fn parses_package_identity_separately_for_each_source() {
    let repository = parse_search(
        b"core/demo 2:1.4-1 (base) [installed: 2:1.3-1]\n    An example\n\
          extra/demo 1.5-1\n    Another example\n",
        SearchSource::Repository,
    )
    .unwrap();
    let aur = parse_search(
        b"aur/demo 1.7-2 (+40 0.43) [9d4h] (Out-of-date: 2026-11-26) \n    A third example\n",
        SearchSource::Aur,
    )
    .unwrap();
    assert_eq!(repository.len(), 2);
    assert_eq!(repository[0].source, Source::Repository("core".into()));
    assert_eq!(repository[0].name, "demo");
    assert_eq!(repository[0].version, "2:1.4-1");
    assert_eq!(repository[0].description, "An example");
    assert_eq!(repository[1].source, Source::Repository("extra".into()));
    assert_eq!(repository[1].name, "demo");
    assert_eq!(aur[0].source, Source::Aur);
    assert_eq!(aur[0].name, "demo");
    assert_eq!(aur[0].version, "1.7-2");
}

#[test]
fn contains_description_control_characters_and_rejects_malformed_records() {
    let rows = parse_search(
        "aur/demo 1.0-1 (+4 0.00) \n    A\u{1b}[31m demo\u{7}\u{9b}\n    continued\n".as_bytes(),
        SearchSource::Aur,
    )
    .unwrap();
    assert_eq!(rows[0].description, "A[31m demo continued");
    assert!(!rows[0].description.chars().any(char::is_control));
    assert!(parse_search(b"aur/demo 1.0-1\n", SearchSource::Aur).is_err());
    assert!(parse_search(b"extra/demo 1.0-1\n    Description\n", SearchSource::Aur).is_err());
}

#[test]
fn distinguishes_empty_pacman_results_from_diagnostics_and_failure() {
    let no_match = command_output(1, b"", b"");
    assert!(
        pacman_stdout(&no_match, "pacman -Q", false)
            .unwrap()
            .is_empty()
    );
    assert!(
        pacman_stdout(&no_match, "pacman -Qqm", true)
            .unwrap()
            .is_empty()
    );
    assert!(
        pacman_stdout(&no_match, "pacman -Ss", true)
            .unwrap()
            .is_empty()
    );

    let error = command_output(
        1,
        b"",
        b"error: no usable package repositories configured.\n",
    );
    assert!(
        pacman_stdout(&error, "pacman -Ss", true)
            .unwrap_err()
            .contains("no usable package repositories")
    );
    assert!(pacman_stdout(&command_output(1, b"yay\n", b""), "pacman -Qqm", true).is_err());
    let partial = command_output(
        0,
        b"demo 1.0-1\n",
        b"warning: database file does not exist\n",
    );
    assert!(pacman_stdout(&partial, "pacman -Qqm", true).is_err());
    assert_eq!(
        pacman_stdout(&partial, "pacman -Q", false).unwrap(),
        b"demo 1.0-1\n"
    );
}

#[test]
fn recognizes_yay_rpc_diagnostics_even_with_zero_exit_status() {
    assert!(
        yay_search_stdout(&command_output(0, b"", b""))
            .unwrap()
            .is_empty()
    );
    assert!(yay_search_stdout(&command_output(1, b"", b"")).is_err());
    let error = command_output(
        0,
        b"",
        b" -> Error during AUR search: 1 error occurred:\n\t* request failed\n",
    );
    let message = yay_search_stdout(&error).unwrap_err();
    assert!(message.contains("Error during AUR search"));
    assert!(message.contains("request failed"));
    assert!(!message.chars().any(char::is_control));
}

#[test]
fn uses_literal_posix_search_terms_and_skips_empty_queries() {
    assert_eq!(
        literal_pattern(r"[a].+(b)?{2}|^$\"),
        r"\[a]\.\+\(b\)\?\{2}\|\^\$\\"
    );
    assert_eq!(literal_pattern("-"), "[-]");
    assert_eq!(literal_pattern("--color never"), "--color never");
    assert!(search(SearchSource::Repository, " \t ").unwrap().is_empty());
    assert!(search(SearchSource::Aur, "text\0text").is_err());
}

#[test]
fn builds_interactive_transactions_with_exact_arguments() {
    let sample = SearchResult {
        name: "demo-name+@_2".into(),
        version: "3.0-1".into(),
        source: Source::Repository("extra".into()),
        description: String::new(),
    };
    let cases: [(Transaction, &str, &[&str]); 5] = [
        (
            Transaction::Install(sample.clone()),
            "sudo",
            &["pacman", "-S", "--needed", "--", "extra/demo-name+@_2"],
        ),
        (
            Transaction::Install(SearchResult {
                source: Source::Aur,
                ..sample
            }),
            "yay",
            &["-S", "--needed", "--", "aur/demo-name+@_2"],
        ),
        (
            Transaction::Remove(vec!["demo-name+@_2".into(), "other".into()]),
            "sudo",
            &["pacman", "-R", "--", "demo-name+@_2", "other"],
        ),
        (Transaction::Upgrade { aur: true }, "yay", &["-Syu"]),
        (
            Transaction::Upgrade { aur: false },
            "sudo",
            &["pacman", "-Syu"],
        ),
    ];

    for (transaction, program, args) in cases {
        let command = transaction.native_command().unwrap();
        assert_eq!(command.get_program(), program);
        assert!(command.get_args().eq(args.iter().copied()));
        assert_eq!(command.get_envs().count(), 0);
    }
}

#[test]
fn rejects_invalid_transaction_targets() {
    assert!(Transaction::Remove(Vec::new()).native_command().is_err());
    for name in ["", "-Syu", ".hidden", "core/demo", "demo\tname", "demo\n"] {
        assert!(
            Transaction::Remove(vec!["valid".into(), name.into()])
                .native_command()
                .is_err()
        );
    }
    for repository in ["", "..", "extra/demo", "extra\x1b"] {
        let package = SearchResult {
            name: "demo".into(),
            version: "1.0-1".into(),
            source: Source::Repository(repository.into()),
            description: String::new(),
        };
        assert!(Transaction::Install(package).native_command().is_err());
    }
}
