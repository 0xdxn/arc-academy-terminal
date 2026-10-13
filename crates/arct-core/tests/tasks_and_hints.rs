//! Outcome checks, task steps and the hint ladder, against a real
//! simulated sandbox.

use arct_core::{
    evaluate_checks, validate_step, Attempt, Check, CommandValidation, LessonStep,
    LessonValidator, PracticeWorld, StepType, VirtualFileSystem,
};

fn sandbox(name: &str) -> VirtualFileSystem {
    VirtualFileSystem::new(name, &format!("test-{}", std::process::id())).expect("sandbox")
}

fn task(checks: Vec<Check>) -> LessonStep {
    LessonStep {
        step_number: 1,
        title: "Build a base".into(),
        instruction: String::new(),
        hint: None,
        step_type: StepType::Task {
            goal: "Make a folder called base with a file map.txt inside".into(),
            checks,
            success_message: "Base built!".into(),
        },
        hints: vec!["mkdir makes folders".into()],
        challenge: false,
        solution: vec![],
    }
}

const NONE: Attempt = Attempt { input: "", output: "" };

#[test]
fn task_passes_only_when_the_world_matches() {
    let vfs = sandbox("task-world");
    let checks = vec![
        Check::DirExists("base".into()),
        Check::FileExists("base/map.txt".into()),
    ];
    let world: &dyn PracticeWorld = &vfs;

    let err = evaluate_checks(&checks, Some(world), &NONE).unwrap_err();
    assert!(err.contains("folder called 'base'"), "{err}");

    vfs.create_directory("/lesson-home/base", false).unwrap();
    let err = evaluate_checks(&checks, Some(world), &NONE).unwrap_err();
    assert!(err.contains("file called 'base/map.txt'"), "{err}");

    vfs.touch_file("/lesson-home/base/map.txt").unwrap();
    assert!(evaluate_checks(&checks, Some(world), &NONE).is_ok());
    let _ = vfs.cleanup();
}

#[test]
fn contains_missing_cwd_output_and_used_checks() {
    let mut vfs = sandbox("task-misc");
    vfs.create_directory("/lesson-home/notes", false).unwrap();
    vfs.write_file("/lesson-home/notes/todo.txt", "feed the cat\n", false)
        .unwrap();
    vfs.change_directory("notes").unwrap();
    let world: &dyn PracticeWorld = &vfs;

    let ok = |c: Check, a: &Attempt| evaluate_checks(&[c], Some(world), a).is_ok();
    assert!(ok(
        Check::FileContains { path: "notes/todo.txt".into(), text: "cat".into() },
        &NONE
    ));
    assert!(!ok(
        Check::FileContains { path: "notes/todo.txt".into(), text: "dog".into() },
        &NONE
    ));
    assert!(ok(Check::Missing("notes/old.txt".into()), &NONE));
    assert!(!ok(Check::Missing("notes/todo.txt".into()), &NONE));
    assert!(ok(Check::CwdIs("notes".into()), &NONE));
    assert!(!ok(Check::CwdIs(String::new()), &NONE));

    let grep = Attempt { input: "cat todo.txt | grep cat", output: "feed the cat" };
    assert!(ok(Check::Used("grep".into()), &grep));
    assert!(!ok(Check::Used("find".into()), &grep));
    assert!(ok(Check::OutputContains("the cat".into()), &grep));
    let _ = vfs.cleanup();
}

#[test]
fn checks_cannot_reach_outside_the_sandbox() {
    let vfs = sandbox("task-escape");
    let world: &dyn PracticeWorld = &vfs;
    assert!(evaluate_checks(&[Check::FileExists("../../../../etc/hosts".into())], Some(world), &NONE).is_err());
    let _ = vfs.cleanup();
}

#[test]
fn task_step_validates_through_validate_step() {
    let vfs = sandbox("task-step");
    let step = task(vec![Check::DirExists("base".into())]);
    let validator = LessonValidator::new();
    let world: &dyn PracticeWorld = &vfs;
    let attempt = Attempt { input: "mkdir base", output: "" };
    assert!(!validate_step(&step, &validator, &attempt, false, Some(world)).is_success());
    vfs.create_directory("/lesson-home/base", false).unwrap();
    assert!(validate_step(&step, &validator, &attempt, false, Some(world)).is_success());
    let _ = vfs.cleanup();
}

#[test]
fn right_command_that_failed_does_not_pass() {
    let step = LessonStep {
        step_number: 1,
        title: "cd".into(),
        instruction: String::new(),
        hint: None,
        step_type: StepType::CommandExercise {
            expected_command: "cd".into(),
            validation: CommandValidation::CommandOnly,
            success_message: "ok".into(),
        },
        hints: vec![],
        challenge: false,
        solution: vec![],
    };
    let validator = LessonValidator::new();
    let attempt = Attempt { input: "cd nowhere", output: "" };
    assert!(validate_step(&step, &validator, &attempt, false, None).is_success());
    assert!(!validate_step(&step, &validator, &attempt, true, None).is_success());
}

#[test]
fn builtin_lessons_parse_and_steps_are_numbered() {
    let library = arct_core::LessonLibrary::new();
    assert!(library.all().len() >= 10);
    for lesson in library.all() {
        assert!(!lesson.steps.is_empty(), "{} has no steps", lesson.id);
        for (i, step) in lesson.steps.iter().enumerate() {
            assert_eq!(step.step_number as usize, i + 1, "{} step numbering", lesson.id);
        }
        for prereq in &lesson.prerequisites {
            assert!(library.get(prereq).is_some(), "{}: unknown prerequisite {prereq}", lesson.id);
        }
    }
}

/// Quiz answers are numbered from 1, as the screen shows them
#[test]
fn quiz_answers_count_from_one() {
    let library = arct_core::LessonLibrary::new();
    let (step, correct) = library
        .all()
        .into_iter()
        .flat_map(|l| l.steps.clone())
        .find_map(|s| match s.step_type {
            arct_core::StepType::MultipleChoice { correct_index, .. } => Some((s.clone(), correct_index)),
            _ => None,
        })
        .expect("a lesson with a quiz");
    let validator = arct_core::LessonValidator::new();
    let answer = |input: &str| {
        let attempt = arct_core::Attempt { input, output: "" };
        arct_core::validate_step(&step, &validator, &attempt, false, None).is_success()
    };
    assert!(answer(&(correct + 1).to_string()));
    assert!(!answer(&correct.to_string()));
    assert!(!answer("0"));
}
