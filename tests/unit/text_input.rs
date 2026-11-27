use super::*;

#[test]
fn cursor_shortcuts_edit_a_query_at_word_boundaries() {
    let mut input = TextInput::from("visual-studio".to_owned());

    input.edit(KeyCode::Home.into());
    for character in "aur/".chars() {
        input.edit(KeyCode::Char(character).into());
    }
    input.edit(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
    assert_eq!(input.cursor(), "aur/visual-".len());
    input.edit(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(input.cursor(), "aur/".len());
    input.edit(KeyCode::End.into());
    input.edit(KeyCode::Char('!').into());
    assert_eq!(input.as_str(), "aur/visual-studio!");
}

#[test]
fn moves_and_deletes_whole_graphemes() {
    let mut input = TextInput::from("a界e\u{301}🚀".to_owned());

    input.edit(KeyCode::Left.into());
    assert_eq!(input.cursor(), "a界e\u{301}".len());
    input.edit(KeyCode::Backspace.into());
    assert_eq!(input.as_str(), "a界🚀");
    input.edit(KeyCode::Home.into());
    input.edit(KeyCode::Right.into());
    input.edit(KeyCode::Delete.into());
    assert_eq!(input.as_str(), "a🚀");
    assert_eq!(input.cursor(), 1);
}
