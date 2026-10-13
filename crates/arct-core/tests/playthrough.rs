//! Plays every built-in lesson, start to finish, through the UI-free
//! lesson runner with the practice files held in memory (what the browser
//! runs), the same way the terminal app's playthrough test does through
//! its Enter key. Then plays each lesson again on disk and checks that
//! both print the same at every step.

use arct_core::{
    CommandValidation, Lesson, LessonLibrary, LessonRun, LessonStep, StepType, Turn,
    VirtualFileSystem,
};

/// What a learner would type to finish a step: the lesson's own
/// solution, or the expected answer for steps that have one
fn step_inputs(lesson_id: &str, step: &LessonStep) -> Vec<String> {
    if !step.solution.is_empty() {
        return step.solution.clone();
    }
    match &step.step_type {
        StepType::CommandExercise {
            expected_command,
            validation,
            ..
        } => match validation {
            CommandValidation::AnyOf(options) => vec![options[0].clone()],
            _ => vec![expected_command.clone()],
        },
        StepType::MultipleChoice { correct_index, .. } => vec![(correct_index + 1).to_string()],
        StepType::FillInBlank {
            template,
            correct_answers,
            ..
        } => {
            let answer = &correct_answers[0];
            let filled = match (template.find('{'), template.find('}')) {
                (Some(a), Some(b)) if a < b => {
                    format!("{}{}{}", &template[..a], answer, &template[b + 1..])
                }
                _ => answer.clone(),
            };
            vec![filled]
        }
        StepType::Information { .. } => vec![String::new()],
        StepType::Practice { .. } | StepType::Task { .. } => panic!(
            "{lesson_id} step {} '{}' is a task and needs a `solution`",
            step.step_number, step.title
        ),
    }
}

/// Play a lesson's solutions; returns every turn
fn play(lesson: &Lesson, mut run: LessonRun) -> Vec<Turn> {
    let mut turns = Vec::new();
    for step in &lesson.steps {
        let before = run.step_index();
        for input in step_inputs(&lesson.id, step) {
            turns.push(run.submit(&input));
        }
        let last = turns.last().expect("every step has an input");
        assert!(
            run.step_index() == before + 1 || run.is_done(),
            "{} step {} '{}' did not pass.\n--- output ---\n{}\n--- check ---\n{:?}",
            lesson.id,
            step.step_number,
            step.title,
            last.output,
            last.result
        );
    }
    assert!(run.is_done(), "{} was not finished", lesson.id);
    assert!(turns.iter().any(|t| t.finished));
    turns
}

#[test]
fn every_builtin_lesson_can_be_completed_in_memory() {
    let library = LessonLibrary::new();
    assert_eq!(library.all().len(), 40);
    for lesson in library.all() {
        let run = LessonRun::new(lesson.clone()).unwrap();
        assert!(run.world().is_in_memory());
        play(lesson, run);
    }
}

/// Output with what may differ between the backends taken out: in an
/// `ls -l` line the mode (new files on disk follow the umask) and a
/// folder's size (the disk reports its own); the operating system's
/// error numbers
fn comparable(output: &str) -> String {
    let mut out = String::new();
    for line in output.split_inclusive('\n') {
        let long = line.len() > 10
            && line.as_bytes()[0].is_ascii()
            && matches!(line.as_bytes()[0], b'd' | b'-')
            && line[1..10].chars().all(|c| "rwx-".contains(c))
            && line[10..].starts_with("  ");
        if long {
            let rest = line[10..].trim_start();
            let (size, name) = rest.split_once("  ").unwrap_or(("", rest));
            let size = if line.starts_with('d') { "-" } else { size };
            out.push_str(&format!("{}--------- {size} {name}", &line[..1]));
        } else {
            out.push_str(line);
        }
    }
    while let Some(start) = out.find(" (os error ") {
        match out[start..].find(')') {
            Some(end) => out.replace_range(start..start + end + 1, ""),
            None => break,
        }
    }
    out
}

#[test]
fn every_lesson_prints_the_same_on_disk_and_in_memory() {
    let session = format!("playthrough-{}", std::process::id());
    let library = LessonLibrary::new();
    for lesson in library.all() {
        let memory = play(lesson, LessonRun::new(lesson.clone()).unwrap());
        let on_disk = LessonRun::with_world(lesson.clone(), |l| VirtualFileSystem::for_lesson(l, &session)).unwrap();
        let disk = play(lesson, on_disk);
        for (a, b) in disk.iter().zip(&memory) {
            assert_eq!(a.prompt_cwd, b.prompt_cwd, "{}: {}", lesson.id, a.command);
            assert_eq!(
                comparable(&a.output),
                comparable(&b.output),
                "{}: the backends differ on: {}",
                lesson.id,
                a.command
            );
        }
    }
}

#[test]
fn misses_climb_the_hint_ladder() {
    let library = LessonLibrary::new();
    let lesson = library.get("explorer-look-around").unwrap().clone();
    let mut run = LessonRun::new(lesson).unwrap();
    // Information steps continue on Enter; find the first one that doesn't
    while matches!(run.step().step_type, StepType::Information { .. }) {
        run.submit("");
    }
    let before = run.step_index();
    let turn = run.submit("frobnicate");
    assert_eq!(run.step_index(), before, "a wrong answer doesn't move on");
    assert!(turn.output.contains("command not found"), "{}", turn.output);
    match turn.result {
        Some(arct_core::ValidationResult::Failure { hint, .. }) => assert!(hint.is_some()),
        other => panic!("expected a failure with a hint, got {other:?}"),
    }
    assert_eq!(turn.prompt_cwd, "~");
}

#[test]
fn looking_around_is_not_a_miss() {
    let library = LessonLibrary::new();
    let lesson = library.get("explorer-hidden-compartments").unwrap().clone();
    let mut run = LessonRun::new(lesson.clone()).unwrap();
    // Play up to "Meet the stowaway" with the lesson's own answers
    while run.step().title != "Meet the stowaway" {
        let step = run.step().clone();
        for input in step_inputs(&lesson.id, &step) {
            run.submit(&input);
        }
    }
    let at = run.step_index();
    let hint = |turn: &Turn| match &turn.result {
        Some(arct_core::ValidationResult::Failure { hint, .. }) => hint.clone(),
        other => panic!("expected a failure, got {other:?}"),
    };

    // Listing and moving around while looking for the note: no hints spent
    for line in ["ls", "ls -a station", "cd station", "ls -a vents", "cd .."] {
        let turn = run.submit(line);
        assert_eq!(run.step_index(), at, "{line}");
        assert_eq!(hint(&turn), None, "{line} only looks");
    }
    // Reading the wrong file is an answer, and a miss
    let turn = run.submit("cat station/vents/stowaway.txt");
    assert!(hint(&turn).is_some(), "a wrong read climbs the ladder");

    // Up to the last rung, then the answer, which costs mastery
    let rungs = run.step().hint_ladder().len();
    for _ in 1..rungs {
        run.submit("cat station/vents/stowaway.txt");
    }
    assert!(run.mastered(), "hints only, so far");
    let turn = run.submit("ls -a station/vents");
    assert_eq!(hint(&turn), None);
    assert!(run.mastered(), "looking never reveals the answer");
    run.submit("cat station/vents/stowaway.txt");
    assert!(!run.mastered(), "a miss past the ladder does");
}
