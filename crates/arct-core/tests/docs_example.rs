//! The example lesson in docs/WRITING-LESSONS.md must stay valid.

#[test]
fn writing_lessons_example_parses() {
    let doc = include_str!("../../../docs/WRITING-LESSONS.md");
    let start = doc.find("```toml\n").expect("toml block") + "```toml\n".len();
    let end = start + doc[start..].find("```").expect("end of block");
    let lesson = arct_core::Lesson::from_toml(&doc[start..end]).expect("example lesson parses");
    assert_eq!(lesson.id, "my-first-lesson");
    assert_eq!(lesson.steps.len(), 4);
    assert!(lesson.steps[3].challenge);
    assert_eq!(lesson.track, arct_core::Track::Explorer);
}
