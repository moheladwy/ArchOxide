use super::*;

const FIREFOX: &str = "app/org.mozilla.firefox/x86_64/stable";
const FIREFOX_BETA: &str = "app/org.mozilla.firefox/x86_64/beta";
const OBS: &str = "app/com.obsproject.Studio/x86_64/stable";

fn arguments(command: &Command) -> Vec<&str> {
    assert_eq!(command.get_program(), "flatpak");
    command
        .get_args()
        .map(|argument| argument.to_str().unwrap())
        .collect()
}

#[test]
fn installed_refs_preserve_branches_and_scopes() {
    let apps = parse_installed(concat!(
        "org.mozilla.firefox/x86_64/stable\tsystem\tflathub\t161.0\tFirefox\n",
        "org.mozilla.firefox/x86_64/stable\tuser\tflathub\t\tFirefox\n",
        "org.mozilla.firefox/x86_64/beta\tsystem (work)\tflathub-beta\t162.0\tFirefox\n",
    ))
    .unwrap();
    assert_eq!(apps[0].reference, FIREFOX);
    assert_eq!(apps[0].installation, Installation::System);
    assert_eq!(apps[1].reference, FIREFOX);
    assert_eq!(apps[1].installation, Installation::User);
    assert_eq!(apps[1].version, "");
    assert_eq!(apps[2].reference, FIREFOX_BETA);
    assert_eq!(apps[2].installation, Installation::Named("work".into()));
    assert_eq!(apps[2].remote, "flathub-beta");
}

#[test]
fn search_matches_metadata_and_keeps_exact_install_target() {
    let apps = parse_search(
        concat!(
            "app/org.mozilla.firefox/x86_64/beta\t162.0\tFirefox\tPrivate Web Browser\n",
            "app/org.example.Other/x86_64/stable\t\tOther\n",
        ),
        &Installation::Named("work".into()),
        "flathub-beta",
        "web browser",
    )
    .unwrap();
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].branch, "beta");
    assert_eq!(apps[0].description, "Private Web Browser");
    let commands = Operation::Install(apps[0].clone()).commands().unwrap();
    assert_eq!(
        arguments(&commands[0]),
        [
            "install",
            "--installation=work",
            "--",
            "flathub-beta",
            FIREFOX_BETA
        ],
    );
}

#[test]
fn selective_updates_group_by_scope_and_deduplicate_refs() {
    let apps = parse_installed(concat!(
        "org.mozilla.firefox/x86_64/stable\tsystem\tflathub\t161.0\tFirefox\n",
        "org.mozilla.firefox/x86_64/stable\tuser\tflathub\t161.0\tFirefox\n",
        "com.obsproject.Studio/x86_64/stable\tuser\tflathub\t32.0\tOBS Studio\n",
    ))
    .unwrap();
    let commands = Operation::Update(vec![
        apps[0].clone(),
        apps[1].clone(),
        apps[2].clone(),
        apps[2].clone(),
    ])
    .commands()
    .unwrap();
    assert_eq!(commands.len(), 2);
    assert_eq!(
        arguments(&commands[0]),
        ["update", "--user", "--", OBS, FIREFOX]
    );
    assert_eq!(
        arguments(&commands[1]),
        ["update", "--system", "--", FIREFOX]
    );
}

#[test]
fn exclusions_match_exact_installation_and_full_ref() {
    let apps = parse_installed(concat!(
        "org.mozilla.firefox/x86_64/stable\tsystem\tflathub\t161.0\tFirefox\n",
        "org.mozilla.firefox/x86_64/stable\tuser\tflathub\t161.0\tFirefox\n",
        "org.mozilla.firefox/x86_64/beta\tsystem\tflathub-beta\t162.0\tFirefox\n",
    ))
    .unwrap();
    let commands = update_except_commands(&apps[..1], &apps).unwrap();
    assert_eq!(commands.len(), 2);
    assert_eq!(arguments(&commands[0]), ["update", "--user", "--", FIREFOX]);
    assert_eq!(
        arguments(&commands[1]),
        ["update", "--system", "--", FIREFOX_BETA]
    );
    assert!(update_except_commands(&apps, &apps).unwrap().is_empty());
    assert!(Operation::Update(Vec::new()).commands().is_err());
    assert!(Operation::UpdateExcept(Vec::new()).commands().is_err());
}
