use std::{fs, os::unix::fs::symlink};

use super::*;

#[test]
fn discovers_visible_apps_with_local_overrides_and_localized_names() {
    let temp = tempfile::tempdir().unwrap();
    let local = temp.path().join("local/applications");
    let system = temp.path().join("system/applications");
    fs::create_dir_all(&local).unwrap();
    fs::create_dir_all(system.join("tools")).unwrap();

    let write = |path: &Path, name: &str, fields: &str| {
        fs::write(path, format!("[Desktop Entry]\nName={name}\n{fields}")).unwrap();
    };
    let app = "Type=Application\nExec=/bin/true %U\n";
    write(&system.join("browser.desktop"), "System browser", app);
    write(
        &local.join("browser.desktop"),
        "Zebra Browser",
        &format!("{app}Name[de]=Atlas Browser\nComment=Browse the web\n"),
    );
    write(&system.join("masked.desktop"), "Masked app", app);
    write(&local.join("masked.desktop"), "Mask", "Hidden=true\n");
    write(&system.join("duplicate.desktop"), "Duplicate app", app);
    write(
        &local.join("duplicate.desktop"),
        "No launcher",
        &format!("{app}NoDisplay=true\n"),
    );
    write(
        &system.join("tools/editor.desktop"),
        "Editor",
        &format!("{app}OnlyShowIn=DWM;\nTryExec=/bin/true\n"),
    );
    write(
        &system.join("missing.desktop"),
        "Missing executable",
        &format!("{app}TryExec=/does-not-exist/archoxide-test\n"),
    );
    write(
        &system.join("excluded.desktop"),
        "Excluded desktop",
        &format!("{app}NotShowIn=DWM;\n"),
    );
    write(
        &system.join("link.desktop"),
        "Link",
        "Type=Link\nURL=https://example.com/\n",
    );
    write(
        &system.join("org.example.Activated.desktop"),
        "Zed",
        "Type=Application\nDBusActivatable=true\n",
    );
    let target = temp.path().join("exported-entry");
    write(&target, "Flatpak export", app);
    symlink(target, system.join("org.example.Exported.desktop")).unwrap();
    let ignored = temp.path().join("unrelated");
    fs::create_dir(&ignored).unwrap();
    write(&ignored.join("stray.desktop"), "Stray", app);

    let apps = discover([local, system].into_iter(), &["de".to_owned()], &["DWM"]);
    assert_eq!(
        apps.iter()
            .map(|entry| (entry.name.as_str(), entry.id.as_str()))
            .collect::<Vec<_>>(),
        [
            ("Atlas Browser", "browser"),
            ("Editor", "tools-editor"),
            ("Flatpak export", "org.example.Exported"),
            ("Zed", "org.example.Activated"),
        ]
    );
    assert_eq!(apps[0].comment, "Browse the web");
    assert!(apps[0].path.ends_with("local/applications/browser.desktop"));
}
