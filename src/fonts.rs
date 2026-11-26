use std::{collections::BTreeMap, io, process::Command};

pub fn installed_families() -> Result<Vec<String>, String> {
    // The first family is the primary name. Later entries can be aliases or localized names.
    let output = Command::new("fc-list")
        .args(["--format", "%{family[0]}\\n"])
        .output()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                "Fontconfig (fc-list) was not found in PATH".to_owned()
            } else {
                format!("Could not list installed fonts with fc-list ({error})")
            }
        })?;

    if !output.status.success() {
        return Err(format!(
            "Could not list installed fonts (fc-list exited with {})",
            output.status
        ));
    }

    let listing = std::str::from_utf8(&output.stdout)
        .map_err(|error| format!("Could not read the installed font names ({error})"))?;
    Ok(parse_families(listing))
}

fn parse_families(listing: &str) -> Vec<String> {
    let mut families = BTreeMap::new();
    for family in listing.lines().filter(|family| is_valid_family(family)) {
        let family = family.trim();
        families.entry(family.to_lowercase()).or_insert(family);
    }
    families.into_values().map(str::to_owned).collect()
}

pub(crate) fn is_valid_family(family: &str) -> bool {
    !family.trim().is_empty()
        && !family
            .chars()
            .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
}

#[cfg(test)]
#[path = "../tests/unit/fonts.rs"]
mod tests;
