//! Integration tests for Arc Academy Terminal core functionality
//!
//! These tests verify that the major components work together correctly.
//! They use the terminal app's records (stats, challenges), so they need
//! the default "progress" feature.
#![cfg(feature = "progress")]

use arct_core::*;

#[test]
fn test_command_analysis_and_explanation() {
    // Test that command parsing and explanation work end-to-end
    let analyzer = CommandAnalyzer::new();
    let mut educator = Educator::new();

    let cmd = analyzer.parse("ls -la /home").expect("Failed to parse command");

    assert_eq!(cmd.program, "ls");
    assert!(!cmd.flags.is_empty());
    assert!(!cmd.args.is_empty());

    let explanation = educator.explain(&cmd).expect("Failed to get explanation");
    assert!(!explanation.summary.is_empty());
    assert!(!explanation.description.is_empty());
}

#[test]
fn test_lesson_validation_flow() {
    // Test that lesson loading and validation works
    let library = LessonLibrary::new();
    let lessons = library.all();

    assert!(!lessons.is_empty(), "Should have lessons available");

    // Use a known lesson and step so the test is deterministic:
    // nav-basics step 1 expects `pwd` with CommandOnly validation.
    let lesson = library.get("nav-basics").expect("nav-basics lesson should exist");
    assert!(!lesson.steps.is_empty(), "Lesson should have steps");

    let validator = LessonValidator::new();
    let first_step = &lesson.steps[0];

    match &first_step.step_type {
        StepType::CommandExercise { expected_command, validation, .. } => {
            // Matching input passes
            let result = validator.validate_command("pwd", expected_command, validation);
            assert!(result.is_success(), "Matching command should pass validation");

            // Non-matching input fails
            let result = validator.validate_command("ls -la", expected_command, validation);
            assert!(!result.is_success(), "Wrong command should fail validation");

            // Empty input fails
            let result = validator.validate_command("", expected_command, validation);
            assert!(!result.is_success(), "Empty input should fail validation");
        }
        other => panic!("nav-basics step 1 should be a CommandExercise, got {:?}", other),
    }

    // Every CommandExercise/Practice validation in the library must reject
    // empty input (no silent stub passes)
    for lesson in library.all() {
        for step in &lesson.steps {
            if let StepType::CommandExercise { expected_command, validation, .. } = &step.step_type {
                let result = validator.validate_command("", expected_command, validation);
                assert!(
                    !result.is_success(),
                    "Lesson '{}' step {} accepted empty input",
                    lesson.id,
                    step.step_number
                );
            }
        }
    }
}

#[test]
fn test_achievement_system() {
    // Test that achievements unlock correctly
    let mut stats = UserStats::new();

    // Initially no achievements
    assert_eq!(stats.achievements.total_unlocked(), 0);

    // Complete a lesson
    stats.record_lesson_completion("test_lesson".to_string(), Difficulty::Beginner);

    // Check for achievements
    let newly_unlocked = stats.check_achievements();

    // Should unlock "First Steps" achievement
    assert!(!newly_unlocked.is_empty(), "Should unlock first achievement");
    assert!(stats.achievements.is_unlocked("first_steps"));
}

#[test]
fn test_challenge_system() {
    // Test that daily challenges are generated deterministically
    let mut manager1 = ChallengeManager::new();
    let mut manager2 = ChallengeManager::new();

    let challenge1 = manager1.get_daily_challenge();
    let challenge2 = manager2.get_daily_challenge();

    // Same day should give same challenge
    assert_eq!(challenge1.id, challenge2.id);
    assert!(challenge1.points > 0);
}

#[test]
fn test_virtual_filesystem_operations() {
    // Test that virtual filesystem works correctly
    let vfs = VirtualFileSystem::new("test_lesson", "test_session")
        .expect("Failed to create virtual filesystem");

    // Should start in lesson-home
    let current = vfs.get_current_dir();
    assert!(current.to_string_lossy().contains("lesson-home"));

    // Should be able to list directory
    let entries = vfs.list_directory(None).expect("Failed to list directory");
    assert!(!entries.is_empty(), "Should have some entries");
}

#[test]
fn test_user_stats_persistence_format() {
    // Test that UserStats can be serialized/deserialized
    let mut stats = UserStats::new();
    stats.record_lesson_completion("lesson1".to_string(), Difficulty::Beginner);
    stats.record_command_use("ls".to_string());

    // Serialize
    let json = serde_json::to_string(&stats).expect("Failed to serialize");

    // Deserialize
    let restored: UserStats = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(restored.lessons_completed.len(), 1);
    assert_eq!(restored.commands_used.len(), 1);
}

#[test]
fn test_recommendation_engine() {
    // Test that lesson recommendations work
    let engine = RecommendationEngine::new();
    let stats = UserStats::new();
    let completed_lessons = std::collections::HashSet::new();

    let recommendations = engine.get_recommendations(&completed_lessons, &stats, 5);

    // Should recommend beginner lessons for new user
    assert!(!recommendations.is_empty());
    assert!(recommendations[0].lesson.difficulty == Difficulty::Beginner);
}

#[test]
fn test_dangerous_command_detection() {
    // Test that dangerous commands are detected
    let analyzer = CommandAnalyzer::new();
    let mut educator = Educator::new();

    let dangerous_cmd = analyzer.parse("rm -rf /").expect("Failed to parse");
    let explanation = educator.explain(&dangerous_cmd).expect("Failed to explain");

    // Should have warnings for dangerous command
    assert!(!explanation.warnings.is_empty(), "Should warn about dangerous command");
}

#[test]
fn test_playground_real_practice_flow() {
    // End-to-end flow of real-filesystem practice mode: open the playground,
    // enter a lesson (starter files materialize), guard the commands a
    // learner would type, and reset back to pristine starter files.
    let root = std::env::temp_dir().join(format!(
        "arct-integration-playground-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);

    let mut playground = Playground::open(root.clone()).expect("playground should open");

    // Built-in lesson setup materializes into <root>/<lesson-id>/
    let library = LessonLibrary::new();
    let lesson = library.get("file-viewing").expect("lesson exists");
    let dir = playground
        .enter_lesson(&lesson.id, &lesson.setup)
        .expect("enter_lesson should succeed");
    assert!(dir.join("server.log").exists());
    assert!(dir.join("notes.txt").exists());

    // Guard: normal lesson commands pass, escapes and disasters are refused
    let guard = PlaygroundGuard::new(playground.root());
    assert!(guard.check("grep http server.log", playground.cwd()).is_allowed());
    assert!(guard.check("head -n 5 server.log", playground.cwd()).is_allowed());
    assert!(!guard.check("cat /etc/hostname", playground.cwd()).is_allowed());
    assert!(!guard.check("rm -rf /", playground.cwd()).is_allowed());
    assert!(!guard.check("cd .. && cat ../secret", playground.cwd()).is_allowed());

    // cd persists inside the playground and refuses to leave it
    playground.change_directory("..").expect("cd to playground root is fine");
    assert!(playground.change_directory("..").is_err());

    // Reset wipes learner changes and restores starter files
    std::fs::write(dir.join("server.log"), "clobbered").unwrap();
    playground.reset_lesson().expect("reset should succeed");
    let restored = std::fs::read_to_string(dir.join("server.log")).unwrap();
    assert!(restored.contains("http server started"));

    let _ = std::fs::remove_dir_all(&root);
}
