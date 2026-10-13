//! Arc Academy Terminal's lessons in a web browser.
//!
//! The same lesson engine the terminal app uses (`arct_core::LessonRun`,
//! the simulated sandbox's shell), compiled to WebAssembly, with the
//! practice files held in memory. It saves nothing, sends nothing and
//! reads no clock: closing the page forgets everything.
//!
//! The API is strings in, JSON out:
//!
//! - `list_lessons()`: `[{id, title, track, difficulty}]`, in the order
//!   the lesson map shows them
//! - `start(lesson_id)`: a fresh run of that lesson; returns the state
//! - `submit(line)`: run what the learner typed (several lines run one
//!   after another) and check the step; returns the state, with one
//!   entry per line
//! - `reset()`: start the current lesson again, with fresh files
//! - `state()`: the state, with no entries
//!
//! The state is `{lesson: {id, title, track, difficulty}, entries:
//! [{prompt_cwd, command, output, feedback_kind, feedback_text, hint}],
//! step: {index, total, number, title, instruction, kind, body, options,
//! challenge}, cwd, done, certificate_ready}`. `step.index` counts from 0;
//! `cwd` is where the next line runs, as a prompt shows it (`~/station`).
//! `feedback_kind` is `success`, `failure`, `partial`, `error` (the line
//! couldn't be read as a command) or `done` (the lesson was already
//! finished). `certificate_ready` says every lesson of this lesson's
//! track has been finished since the page loaded. Errors come back as
//! `{error}`.
//!
//! [`Engine`] is that API in Rust. For the browser, the `arct_*`
//! functions export it over WebAssembly's memory: strings go in as
//! (pointer, length) of UTF-8 bytes the caller wrote into memory from
//! [`arct_alloc`]; each result is a pointer to a 4-byte little-endian
//! length followed by that many bytes of JSON, which the caller frees with
//! [`arct_free`] (length + 4). `web/arct-web.js` does all of this.

use arct_core::{Lesson, LessonLibrary, LessonRun, StepType, Track, Turn, ValidationResult};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeSet;

/// The browser's lesson engine: one lesson at a time, plus which lessons
/// were finished since it started (in memory only)
pub struct Engine {
    library: LessonLibrary,
    run: Option<LessonRun>,
    finished: BTreeSet<String>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

fn track_name(track: Track) -> &'static str {
    match track {
        Track::Explorer => "explorer",
        Track::Operator => "operator",
    }
}

fn lesson_info(lesson: &Lesson) -> Value {
    json!({
        "id": lesson.id,
        "title": lesson.title,
        "track": track_name(lesson.track),
        "difficulty": format!("{:?}", lesson.difficulty).to_lowercase(),
    })
}

fn error(message: impl std::fmt::Display) -> String {
    json!({ "error": message.to_string() }).to_string()
}

impl Engine {
    pub fn new() -> Self {
        Self {
            library: LessonLibrary::new(),
            run: None,
            finished: BTreeSet::new(),
        }
    }

    /// Every built-in lesson, in the order the lesson map shows them
    pub fn list_lessons(&self) -> String {
        Value::Array(self.library.all().into_iter().map(lesson_info).collect()).to_string()
    }

    /// Start a lesson from its first step, with fresh practice files
    pub fn start(&mut self, lesson_id: &str) -> String {
        let Some(lesson) = self.library.get(lesson_id.trim()) else {
            return error(format!("There's no lesson called '{}'.", lesson_id.trim()));
        };
        match LessonRun::new(lesson.clone()) {
            Ok(run) => {
                self.run = Some(run);
                self.state_with(Vec::new())
            }
            Err(e) => error(format!("Could not set up the lesson: {e}")),
        }
    }

    /// Run what the learner typed and check the step. Several lines run
    /// one after another; an empty line is Enter on its own.
    pub fn submit(&mut self, text: &str) -> String {
        let Some(run) = self.run.as_mut() else {
            return error("Start a lesson first.");
        };
        let lines: Vec<&str> = if text.is_empty() {
            vec![""]
        } else {
            text.lines().map(|l| l.trim_end_matches('\r')).collect()
        };
        let mut entries = Vec::new();
        for line in lines {
            let was_done = run.is_done();
            let turn = run.submit(line);
            if turn.finished {
                self.finished.insert(run.lesson().id.clone());
            }
            entries.push(entry(&turn, was_done));
        }
        self.state_with(entries)
    }

    /// Start the current lesson again, with fresh practice files
    pub fn reset(&mut self) -> String {
        match self.run.as_ref().map(|run| run.lesson().id.clone()) {
            Some(id) => self.start(&id),
            None => error("Start a lesson first."),
        }
    }

    /// The current state, with no entries
    pub fn state(&self) -> String {
        self.state_with(Vec::new())
    }

    fn state_with(&self, entries: Vec<Value>) -> String {
        let Some(run) = self.run.as_ref() else {
            return error("Start a lesson first.");
        };
        let lesson = run.lesson();
        let step = run.step();
        let (kind, body, options) = match &step.step_type {
            StepType::CommandExercise { .. } => ("command", step.instruction.clone(), Vec::new()),
            StepType::MultipleChoice { question, options, .. } => ("choice", question.clone(), options.clone()),
            StepType::FillInBlank { template, .. } => ("fill", format!("{}\nTemplate: {template}", step.instruction), Vec::new()),
            StepType::Information { content } => ("info", content.clone(), Vec::new()),
            StepType::Practice { goal, .. } | StepType::Task { goal, .. } => {
                let mut body = goal.clone();
                if !step.instruction.is_empty() && step.instruction != *goal {
                    body = format!("{goal}\n{}", step.instruction);
                }
                ("task", body, Vec::new())
            }
        };
        let track_done = self
            .library
            .all()
            .into_iter()
            .filter(|l| l.track == lesson.track)
            .all(|l| self.finished.contains(&l.id));
        json!({
            "lesson": lesson_info(lesson),
            "entries": entries,
            "step": {
                "index": run.step_index(),
                "total": lesson.steps.len(),
                "number": step.step_number,
                "title": step.title,
                "instruction": step.instruction,
                "kind": kind,
                "body": body,
                "options": options,
                "challenge": step.challenge,
            },
            "cwd": run.display_cwd(),
            "done": run.is_done(),
            "certificate_ready": track_done,
        })
        .to_string()
    }
}

/// One line's entry in the transcript
fn entry(turn: &Turn, was_done: bool) -> Value {
    let (kind, text, hint) = match &turn.result {
        Some(ValidationResult::Success { message }) => ("success", message.clone(), None),
        Some(ValidationResult::Failure { message, hint }) => ("failure", message.clone(), hint.clone()),
        Some(ValidationResult::Partial { message, progress }) => {
            ("partial", format!("{message} ({progress:.0}% correct)"), None)
        }
        None if was_done => (
            "done",
            "This lesson is finished. Start it again, or pick another one.".to_string(),
            None,
        ),
        None => ("error", "That line couldn't be read as a command.".to_string(), None),
    };
    json!({
        "prompt_cwd": turn.prompt_cwd,
        "command": turn.command,
        "output": turn.output,
        "feedback_kind": kind,
        "feedback_text": text,
        "hint": hint,
    })
}

// ----------------------------------------------------------------------------
// WebAssembly exports
// ----------------------------------------------------------------------------

thread_local! {
    static ENGINE: RefCell<Engine> = RefCell::new(Engine::new());
}

fn layout(len: usize) -> std::alloc::Layout {
    std::alloc::Layout::array::<u8>(len.max(1)).expect("a buffer size that fits in memory")
}

/// Hand a result to the caller: a 4-byte little-endian length, then the
/// bytes. The caller frees it with `arct_free(ptr, len + 4)`.
fn hand_out(text: String) -> *mut u8 {
    let bytes = text.into_bytes();
    let total = bytes.len() + 4;
    let ptr = arct_alloc(total);
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    // SAFETY: arct_alloc returned `total` writable bytes
    unsafe {
        std::ptr::copy_nonoverlapping(len.to_le_bytes().as_ptr(), ptr, 4);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(4), bytes.len());
    }
    ptr
}

/// The caller's string: `len` bytes of UTF-8 at `ptr`
///
/// # Safety
///
/// `ptr` must point to `len` readable bytes (or `len` must be 0).
unsafe fn take_in(ptr: *const u8, len: usize) -> String {
    if len == 0 || ptr.is_null() {
        return String::new();
    }
    String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned()
}

/// Reserve `len` bytes for the caller to write a string into
#[no_mangle]
pub extern "C" fn arct_alloc(len: usize) -> *mut u8 {
    // SAFETY: the layout's size is never zero
    let ptr = unsafe { std::alloc::alloc(layout(len)) };
    if ptr.is_null() {
        std::alloc::handle_alloc_error(layout(len));
    }
    ptr
}

/// Give back a buffer from `arct_alloc` (or a result, with its length + 4)
///
/// # Safety
///
/// `ptr` must come from `arct_alloc(len)` (or be a result whose payload is
/// `len - 4` bytes) and not have been freed already.
#[no_mangle]
pub unsafe extern "C" fn arct_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        std::alloc::dealloc(ptr, layout(len));
    }
}

/// `list_lessons()`
#[no_mangle]
pub extern "C" fn arct_list_lessons() -> *mut u8 {
    hand_out(ENGINE.with(|e| e.borrow().list_lessons()))
}

/// `start(lesson_id)`
///
/// # Safety
///
/// `ptr` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn arct_start(ptr: *const u8, len: usize) -> *mut u8 {
    let id = take_in(ptr, len);
    hand_out(ENGINE.with(|e| e.borrow_mut().start(&id)))
}

/// `submit(line)`
///
/// # Safety
///
/// `ptr` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn arct_submit(ptr: *const u8, len: usize) -> *mut u8 {
    let line = take_in(ptr, len);
    hand_out(ENGINE.with(|e| e.borrow_mut().submit(&line)))
}

/// `reset()`
#[no_mangle]
pub extern "C" fn arct_reset() -> *mut u8 {
    hand_out(ENGINE.with(|e| e.borrow_mut().reset()))
}

/// `state()`
#[no_mangle]
pub extern "C" fn arct_state() -> *mut u8 {
    hand_out(ENGINE.with(|e| e.borrow().state()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Value {
        serde_json::from_str(text).expect("valid JSON")
    }

    /// Call an export the way the JavaScript loader does: write the string
    /// into a buffer from arct_alloc, call, read the length-prefixed
    /// result, free both
    fn call(export: unsafe extern "C" fn(*const u8, usize) -> *mut u8, arg: &str) -> Value {
        let input = arct_alloc(arg.len());
        unsafe {
            std::ptr::copy_nonoverlapping(arg.as_ptr(), input, arg.len());
            let out = export(input, arg.len());
            arct_free(input, arg.len());
            read_result(out)
        }
    }

    unsafe fn read_result(ptr: *mut u8) -> Value {
        let mut len = [0u8; 4];
        std::ptr::copy_nonoverlapping(ptr, len.as_mut_ptr(), 4);
        let len = u32::from_le_bytes(len) as usize;
        let text = std::str::from_utf8(std::slice::from_raw_parts(ptr.add(4), len))
            .unwrap()
            .to_string();
        arct_free(ptr, len + 4);
        parse(&text)
    }

    #[test]
    fn lessons_are_listed_with_their_track() {
        let list = parse(&Engine::new().list_lessons());
        let list = list.as_array().unwrap();
        assert_eq!(list.len(), 40);
        assert_eq!(list[0]["track"], "explorer");
        assert_eq!(list[0]["difficulty"], "beginner");
        assert!(list.iter().any(|l| l["id"] == "nav-basics" && l["track"] == "operator"));
    }

    #[test]
    fn a_lesson_plays_through_the_api() {
        let mut engine = Engine::new();
        let state = parse(&engine.start("explorer-look-around"));
        assert_eq!(state["step"]["index"], 0);
        assert_eq!(state["cwd"], "~");
        assert_eq!(state["done"], false);
        assert!(state["entries"].as_array().unwrap().is_empty());

        let library = LessonLibrary::new();
        let lesson = library.get("explorer-look-around").unwrap();
        let mut last = Value::Null;
        for step in &lesson.steps {
            let input = if step.solution.is_empty() {
                match &step.step_type {
                    StepType::Information { .. } => String::new(),
                    StepType::MultipleChoice { correct_index, .. } => correct_index.to_string(),
                    StepType::CommandExercise { expected_command, .. } => expected_command.clone(),
                    other => panic!("no input for {other:?}"),
                }
            } else {
                step.solution.join("\n")
            };
            last = parse(&engine.submit(&input));
            assert_eq!(last["entries"][0]["prompt_cwd"].as_str().map(|p| p.starts_with('~')), Some(true));
        }
        assert_eq!(last["done"], true, "{last}");
        let entries = last["entries"].as_array().unwrap();
        assert_eq!(entries.last().unwrap()["feedback_kind"], "success");
        assert_eq!(last["certificate_ready"], false, "one lesson of the track isn't the track");

        // After the end, lines are answered but not checked
        let after = parse(&engine.submit("ls"));
        assert_eq!(after["entries"][0]["feedback_kind"], "done");

        // reset starts over with fresh files
        let state = parse(&engine.reset());
        assert_eq!(state["step"]["index"], 0);
        assert_eq!(state["done"], false);
    }

    #[test]
    fn wrong_answers_get_hints_and_bad_input_gets_errors() {
        let mut engine = Engine::new();
        assert!(parse(&engine.submit("ls"))["error"].is_string(), "no lesson yet");
        assert!(parse(&engine.start("no-such-lesson"))["error"].is_string());
        parse(&engine.start("nav-basics"));
        let state = parse(&engine.submit("frobnicate"));
        let entry = &state["entries"][0];
        assert_eq!(entry["feedback_kind"], "failure");
        assert!(entry["hint"].is_string());
        assert!(entry["output"].as_str().unwrap().contains("command not found"));
        assert_eq!(state["step"]["index"], 0);
        let state = parse(&engine.submit("echo 'unclosed"));
        assert_eq!(state["entries"][0]["feedback_kind"], "error");
        // Several lines: one entry each
        let state = parse(&engine.submit("pwd\nls\n"));
        assert_eq!(state["entries"].as_array().unwrap().len(), 2);
        assert_eq!(state["entries"][0]["output"], "/lesson-home\n");
    }

    #[test]
    fn the_exports_speak_the_same_api() {
        let list = unsafe { read_result(arct_list_lessons()) };
        assert_eq!(list.as_array().unwrap().len(), 40);
        let state = call(arct_start, "explorer-look-around");
        assert_eq!(state["lesson"]["id"], "explorer-look-around");
        assert_eq!(state["step"]["total"].as_u64().unwrap() as usize, LessonLibrary::new().get("explorer-look-around").unwrap().steps.len());
        let state = call(arct_submit, "ls");
        assert_eq!(state["entries"][0]["command"], "ls");
        let state = unsafe { read_result(arct_state()) };
        assert!(state["entries"].as_array().unwrap().is_empty());
        let state = unsafe { read_result(arct_reset()) };
        assert_eq!(state["step"]["index"], 0);
        // An empty line is Enter: still an answer, never a crash
        let state = call(arct_submit, "");
        assert!(state["entries"].is_array());
    }

    #[test]
    fn finishing_a_whole_track_makes_the_certificate_ready() {
        let mut engine = Engine::new();
        let library = LessonLibrary::new();
        let explorer: Vec<&Lesson> = library.all().into_iter().filter(|l| l.track == Track::Explorer).collect();
        let mut last = Value::Null;
        for lesson in explorer {
            engine.start(&lesson.id);
            for step in &lesson.steps {
                let input = match (&step.step_type, step.solution.is_empty()) {
                    (_, false) => step.solution.join("\n"),
                    (StepType::Information { .. }, true) => String::new(),
                    (StepType::MultipleChoice { correct_index, .. }, true) => correct_index.to_string(),
                    (StepType::CommandExercise { expected_command, .. }, true) => expected_command.clone(),
                    (StepType::FillInBlank { correct_answers, .. }, true) => correct_answers[0].clone(),
                    (other, true) => panic!("no input for {other:?}"),
                };
                last = parse(&engine.submit(&input));
            }
            assert_eq!(last["done"], true, "{}: {last}", lesson.id);
        }
        assert_eq!(last["certificate_ready"], true);
    }
}
