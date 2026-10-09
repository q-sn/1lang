//! Snapshot → overwrite → restore round-trip of the clipboard.
use onelang_platform::clipboard;

fn main() {
    let original = clipboard::snapshot().expect("snapshot");
    let before = clipboard::read_text().ok().flatten();
    clipboard::write_text("1lang clipboard test", true).unwrap();
    assert_eq!(clipboard::read_text().unwrap().as_deref(), Some("1lang clipboard test"));
    clipboard::restore(&original).unwrap();
    let after = clipboard::read_text().ok().flatten();
    println!("restored text equal: {}", before == after);
}
