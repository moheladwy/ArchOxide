use super::*;
use std::os::unix::fs::symlink;

fn fixture(path: &Path, payload: u8) -> Vec<u8> {
    let mut bytes = vec![0; 65];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    bytes[8..11].copy_from_slice(b"AI\x02");
    bytes[16] = 3;
    bytes[18] = 62;
    bytes[20] = 1;
    bytes[52] = 64;
    bytes[64] = payload;
    fs::write(path, &bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
    bytes
}

fn store(directory: &Path) -> Store {
    Store::at(&directory.join("archoxide/appimages"), true)
        .unwrap()
        .unwrap()
}

fn import(store: &Store, path: &Path) -> Result<(), String> {
    let source = Source::parse(path.to_str().unwrap())?;
    store.install(&source.name()?, source, None, &mut |_| {})
}

#[test]
fn data_directory_honors_absolute_xdg_paths_and_falls_back_to_home() {
    let home = Some("/home/example".into());
    assert_eq!(
        data_path(Some("/data".into()), home.clone()).unwrap(),
        Path::new("/data/archoxide/appimages")
    );
    assert_eq!(
        data_path(Some("relative".into()), home).unwrap(),
        Path::new("/home/example/.local/share/archoxide/appimages")
    );
    assert!(data_path(None, None).is_err());
}

#[test]
fn validates_sources_and_derives_safe_url_names() {
    let source =
        Source::parse("https://user:secret@example.com/My%20App.appimage?token=1").unwrap();
    assert_eq!(source.name().unwrap(), "My App");
    assert!(Source::parse("http://example.com/App.AppImage").is_err());
    assert!(
        Source::parse("https://example.com/App%2fImage")
            .unwrap()
            .name()
            .is_err()
    );
    assert!(
        Source::parse("https://example.com/%2e%2e")
            .unwrap()
            .name()
            .is_err()
    );
}

#[test]
fn imports_and_removes_only_the_managed_copy() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let path = directory.path().join("Example App.AppImage");
    let content = fixture(&path, 4);
    let alias = directory.path().join("Alias.AppImage");
    symlink(&path, &alias).unwrap();

    let source = Source::parse(alias.to_str().unwrap()).unwrap();
    let mut messages = Vec::new();
    store
        .install(&source.name().unwrap(), source, None, &mut |message| {
            messages.push(message.to_owned());
        })
        .unwrap();
    assert_eq!(
        messages,
        [
            "Copying Example App from a local file",
            "Verifying the AppImage header for Example App",
            "Installing Example App",
        ]
    );
    let entry = store.installed().unwrap().pop().unwrap();
    assert_eq!(entry.name, "Example App");
    assert_eq!(entry.origin.as_deref(), path.to_str());
    assert_eq!(fs::read(&entry.path).unwrap(), content);
    assert_eq!(
        entry.path.metadata().unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o640);
    assert!(import(&store, &path).is_err());

    messages.clear();
    store
        .remove(&entry, &mut |message| messages.push(message.to_owned()))
        .unwrap();
    assert_eq!(messages, ["Removing Example App"]);
    assert!(store.installed().unwrap().is_empty());
    assert!(!entry.path.exists());
    assert_eq!(fs::read(path).unwrap(), content);
}

#[test]
fn removes_selected_managed_copies_and_reports_partial_failure() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    for name in ["Alpha", "Beta", "Gamma"] {
        let path = directory.path().join(format!("{name}.AppImage"));
        fixture(&path, 4);
        import(&store, &path).unwrap();
    }

    let entries = store.installed().unwrap();
    let extra = entries[0].path.parent().unwrap().join("keep.txt");
    fs::write(&extra, "keep").unwrap();
    let result = store.remove_selected(&entries[..2], &mut |_| {});
    let message = result.unwrap_err();
    assert!(message.contains("Removed 1 of 2 selected AppImages"));
    assert!(message.contains("Alpha"));
    assert!(extra.exists());
    assert!(!entries[1].path.exists());
    assert_eq!(
        store.installed().unwrap(),
        [entries[0].clone(), entries[2].clone()]
    );
    assert!(directory.path().join("Beta.AppImage").exists());
}

#[test]
fn a_failed_replacement_keeps_the_binary_and_origin() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let original = directory.path().join("Example.AppImage");
    let original_content = fixture(&original, 4);
    let new_source = directory.path().join("Example-v2.AppImage");
    let mut plain_elf = fixture(&new_source, 5);
    plain_elf[8..11].fill(0);
    fs::write(&new_source, plain_elf).unwrap();
    import(&store, &original).unwrap();

    let first = store.installed().unwrap().pop().unwrap();
    let result = store.replace(&first, Source::Local(new_source.clone()), &mut |_| {});
    assert!(result.unwrap_err().contains("ELF header"));
    assert_eq!(store.installed().unwrap(), vec![first.clone()]);
    assert_eq!(fs::read(&first.path).unwrap(), original_content);

    let new_content = fixture(&new_source, 5);
    store
        .replace(&first, Source::Local(new_source.clone()), &mut |_| {})
        .unwrap();
    let second = store.installed().unwrap().pop().unwrap();
    assert_eq!(second.path, first.path);
    assert_eq!(second.origin.as_deref(), new_source.to_str());
    assert_eq!(fs::read(&second.path).unwrap(), new_content);
    assert!(
        store
            .remove(&first, &mut |_| {})
            .unwrap_err()
            .contains("has changed")
    );
}

#[test]
fn refreshes_other_entries_when_one_source_is_missing() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let first = directory.path().join("First.AppImage");
    let first_content = fixture(&first, 4);
    let second = directory.path().join("Second.AppImage");
    fixture(&second, 5);
    import(&store, &first).unwrap();
    import(&store, &second).unwrap();

    let entries = store.installed().unwrap();
    fs::remove_file(&first).unwrap();
    let updated_content = fixture(&second, 9);
    let mut messages = Vec::new();
    let message = store
        .update(&entries, &mut |message| messages.push(message.to_owned()))
        .unwrap_err();
    assert!(message.contains("Refreshed 1 of 2 AppImages"));
    assert_eq!(
        messages,
        [
            "Refreshing First (1/2)",
            "Could not refresh First",
            "Refreshing Second (2/2)",
            "Copying Second from a local file",
            "Verifying the AppImage header for Second",
            "Installing Second",
            "Refreshed Second",
        ]
    );
    assert_eq!(fs::read(&entries[0].path).unwrap(), first_content);
    assert_eq!(fs::read(&entries[1].path).unwrap(), updated_content);
}

#[test]
fn preserves_unmanaged_entries_and_refuses_to_follow_storage_links() {
    let directory = tempfile::tempdir().unwrap();
    let store = store(directory.path());
    let path = directory.path().join("Example.AppImage");
    fixture(&path, 4);
    let unmanaged = store.root.join("Example");
    symlink(directory.path(), &unmanaged).unwrap();
    assert!(store.installed().unwrap().is_empty());
    assert!(import(&store, &path).is_err());
    assert!(unmanaged.is_symlink());
    fs::remove_file(unmanaged).unwrap();

    import(&store, &path).unwrap();
    let entry = store.installed().unwrap().pop().unwrap();
    let extra = entry.path.parent().unwrap().join("keep.txt");
    fs::write(&extra, "keep me").unwrap();
    assert!(store.remove(&entry, &mut |_| {}).is_err());
    assert_eq!(fs::read_to_string(extra).unwrap(), "keep me");
}
