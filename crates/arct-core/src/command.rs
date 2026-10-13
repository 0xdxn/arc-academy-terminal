//! Command parsing, analysis, and categorization

use crate::types::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a parsed shell command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub raw: String,
    pub program: String,
    pub args: Vec<String>,
    pub category: CommandCategory,
    pub danger_level: DangerLevel,
    pub flags: Vec<Flag>,
}

/// Categories of commands for educational grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandCategory {
    Navigation,
    FileManagement,
    TextProcessing,
    SystemInfo,
    ProcessManagement,
    Networking,
    Permissions,
    Archiving,
    Search,
    Git,
    Build,
    Package,
    Unknown,
}

/// Danger level assessment for commands
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DangerLevel {
    Safe,       // Read-only, no system modifications
    Caution,    // Modifies files but recoverable
    Dangerous,  // Can cause data loss or system issues
    Critical,   // Requires elevated privileges or very dangerous
}

/// Represents a command flag/option with its meaning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flag {
    pub raw: String,
    pub short: Option<char>,
    pub long: Option<String>,
    pub description: Option<String>,
    pub argument: Option<String>,
}

/// Command metadata for educational purposes
#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub name: String,
    pub category: CommandCategory,
    pub danger_level: DangerLevel,
    pub summary: &'static str,
    pub description: &'static str,
    pub common_flags: Vec<FlagInfo>,
    pub examples: Vec<Example>,
    pub related_commands: Vec<&'static str>,
}

/// Information about a specific flag
#[derive(Debug, Clone)]
pub struct FlagInfo {
    pub flag: &'static str,
    pub description: &'static str,
    pub example: Option<&'static str>,
}

/// Example usage of a command
#[derive(Debug, Clone)]
pub struct Example {
    pub command: &'static str,
    pub description: &'static str,
    pub use_case: &'static str,
}

/// Analyzes commands and provides educational context
pub struct CommandAnalyzer {
    commands: HashMap<String, CommandInfo>,
}

impl CommandAnalyzer {
    pub fn new() -> Self {
        let mut analyzer = Self {
            commands: HashMap::new(),
        };
        analyzer.register_builtins();
        analyzer
    }

    /// Parse a raw command string into a Command
    pub fn parse(&self, input: &str) -> Result<Command> {
        let parts = shellwords::split(input)
            .map_err(|e| Error::ParseError(e.to_string()))?;

        if parts.is_empty() {
            return Err(Error::ParseError("Empty command".to_string()));
        }

        let program = parts[0].clone();
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        let info = self.get_command_info(&program);
        let category = info.map(|i| i.category).unwrap_or(CommandCategory::Unknown);
        let danger_level = info.map(|i| i.danger_level).unwrap_or(DangerLevel::Safe);

        let flags = self.parse_flags(&args);

        Ok(Command {
            raw: input.to_string(),
            program,
            args,
            category,
            danger_level,
            flags,
        })
    }

    /// Extract and parse flags from arguments
    fn parse_flags(&self, args: &[String]) -> Vec<Flag> {
        let mut flags = Vec::new();

        for arg in args {
            if arg.starts_with("--") {
                // Long flag
                let (name, value) = if let Some(idx) = arg.find('=') {
                    (arg[2..idx].to_string(), Some(arg[idx + 1..].to_string()))
                } else {
                    (arg[2..].to_string(), None)
                };

                flags.push(Flag {
                    raw: arg.clone(),
                    short: None,
                    long: Some(name),
                    description: None,
                    argument: value,
                });
            } else if arg.starts_with('-') && arg.len() > 1 && !arg.chars().nth(1).unwrap().is_ascii_digit() {
                // Short flag(s)
                for ch in arg[1..].chars() {
                    flags.push(Flag {
                        raw: format!("-{}", ch),
                        short: Some(ch),
                        long: None,
                        description: None,
                        argument: None,
                    });
                }
            }
        }

        flags
    }

    /// Get command information if available
    pub fn get_command_info(&self, cmd: &str) -> Option<&CommandInfo> {
        self.commands.get(cmd)
    }

    /// Suggest similar commands for typos
    pub fn suggest_similar(&self, cmd: &str) -> Vec<String> {
        let mut suggestions: Vec<(String, usize)> = self
            .commands
            .keys()
            .map(|k| (k.clone(), levenshtein_distance(cmd, k)))
            .filter(|(_, dist)| *dist <= 2)
            .collect();

        suggestions.sort_by_key(|(_, dist)| *dist);
        suggestions.truncate(5);
        suggestions.into_iter().map(|(cmd, _)| cmd).collect()
    }

    /// Register built-in command definitions
    fn register_builtins(&mut self) {
        // Navigation commands
        self.register(CommandInfo {
            name: "ls".to_string(),
            category: CommandCategory::Navigation,
            danger_level: DangerLevel::Safe,
            summary: "List directory contents",
            description: "Shows you what files and folders are in your current directory, like opening a folder in a file browser. By default it shows just names, but with flags like -l (long format) you can see details like file sizes, permissions, and modification dates. The -a flag shows hidden files (those starting with a dot), and -h makes file sizes human-readable (like '2.3GB' instead of '2456334342'). This is usually the first command you run when exploring a new directory.",
            common_flags: vec![
                FlagInfo { flag: "-l", description: "Long format: shows permissions, owner, size, and date", example: Some("ls -l") },
                FlagInfo { flag: "-a", description: "All files: includes hidden files (starting with .)", example: Some("ls -a") },
                FlagInfo { flag: "-h", description: "Human-readable: shows file sizes in KB, MB, GB", example: Some("ls -lh") },
                FlagInfo { flag: "-t", description: "Sort by time: newest files first", example: None },
                FlagInfo { flag: "-r", description: "Reverse order: reverses the sort", example: None },
            ],
            examples: vec![
                Example { command: "ls -la", description: "List all files in long format", use_case: "See all files including hidden ones with details" },
                Example { command: "ls -lh", description: "List files with human-readable sizes", use_case: "Quickly see file sizes in MB/GB" },
            ],
            related_commands: vec!["cd", "pwd", "tree", "find"],
        });

        self.register(CommandInfo {
            name: "cd".to_string(),
            category: CommandCategory::Navigation,
            danger_level: DangerLevel::Safe,
            summary: "Change directory",
            description: "Moves you from one folder to another in the filesystem, like clicking folders in a file browser. Your terminal is always 'in' some folder - this command lets you move around. Use 'cd ..' to go up one level to the parent folder, 'cd ~' to go to your home folder, or 'cd /path/to/folder' to go to a specific location. Think of it as your navigation tool for moving through your file system.",
            common_flags: vec![],
            examples: vec![
                Example { command: "cd /home/user", description: "Go to specific directory", use_case: "Navigate to an absolute path" },
                Example { command: "cd ..", description: "Go up one directory", use_case: "Move to parent directory" },
                Example { command: "cd -", description: "Go to previous directory", use_case: "Toggle between two directories" },
            ],
            related_commands: vec!["pwd", "ls", "pushd", "popd"],
        });

        self.register(CommandInfo {
            name: "pwd".to_string(),
            category: CommandCategory::Navigation,
            danger_level: DangerLevel::Safe,
            summary: "Print working directory",
            description: "Displays the complete path of where you currently are in the filesystem. Like a 'You Are Here' sign in a mall - it tells you exactly where you're standing. Useful when you're lost or need to know the full path to copy to another command or share with someone. The output will be something like '/home/username/Documents' showing the complete chain from the root of the filesystem to your current location.",
            common_flags: vec![
                FlagInfo { flag: "-P", description: "Physical path: shows actual path resolving symlinks", example: None },
            ],
            examples: vec![
                Example { command: "pwd", description: "Show current directory", use_case: "Find out where you are" },
            ],
            related_commands: vec!["cd", "ls"],
        });

        // File management
        self.register(CommandInfo {
            name: "rm".to_string(),
            category: CommandCategory::FileManagement,
            danger_level: DangerLevel::Dangerous,
            summary: "Remove files or directories",
            description: "Careful: Permanently deletes files or directories from your system. Unlike moving files to a recycle bin in a graphical interface, this command completely removes them - there's no undo! Use 'rm file.txt' to delete a single file, or 'rm -r folder/' to delete a directory and everything inside it. The -i flag will ask for confirmation before each deletion, which is much safer for beginners. Always double-check what you're deleting because once it's gone, it's gone forever!",
            common_flags: vec![
                FlagInfo { flag: "-r", description: "Recursive: remove directories and their contents", example: Some("rm -r folder/") },
                FlagInfo { flag: "-f", description: "Force: ignore warnings and nonexistent files", example: None },
                FlagInfo { flag: "-i", description: "Interactive: prompt before each removal", example: Some("rm -i file.txt") },
            ],
            examples: vec![
                Example { command: "rm file.txt", description: "Delete a single file", use_case: "Remove unwanted file" },
                Example { command: "rm -ri folder/", description: "Safely delete directory with confirmation", use_case: "Remove directory with safety prompts" },
            ],
            related_commands: vec!["rmdir", "trash", "shred"],
        });

        self.register(CommandInfo {
            name: "cp".to_string(),
            category: CommandCategory::FileManagement,
            danger_level: DangerLevel::Caution,
            summary: "Copy files and directories",
            description: "Creates a duplicate copy of files or directories, leaving the original untouched. Think of it like the copy-paste function in a file browser. Use 'cp file.txt backup.txt' to copy a file, or 'cp -r folder/ backup/' to copy an entire directory with all its contents. The -i flag will warn you before overwriting existing files, and -p preserves the original file's permissions and timestamps, which is useful for backups.",
            common_flags: vec![
                FlagInfo { flag: "-r", description: "Recursive: copy directories and their contents", example: Some("cp -r folder/ backup/") },
                FlagInfo { flag: "-i", description: "Interactive: prompt before overwriting existing files", example: Some("cp -i file.txt dest/") },
                FlagInfo { flag: "-v", description: "Verbose: show files as they are copied", example: None },
                FlagInfo { flag: "-p", description: "Preserve: keep original permissions, timestamps, and ownership", example: None },
            ],
            examples: vec![
                Example { command: "cp file.txt backup.txt", description: "Copy a file", use_case: "Create a backup of a file" },
                Example { command: "cp -r folder/ backup/", description: "Copy a directory recursively", use_case: "Backup an entire directory" },
            ],
            related_commands: vec!["mv", "rsync", "dd"],
        });

        self.register(CommandInfo {
            name: "mv".to_string(),
            category: CommandCategory::FileManagement,
            danger_level: DangerLevel::Caution,
            summary: "Move or rename files and directories",
            description: "Moves files to a different location or renames them - it's the same operation in Linux! Think of it like cut-and-paste or the rename function in a file browser. Unlike 'cp', the original file disappears after moving. Use 'mv oldname.txt newname.txt' to rename a file, or 'mv file.txt /other/folder/' to move it to a different directory. The -i flag prompts for confirmation before overwriting any existing files with the same name.",
            common_flags: vec![
                FlagInfo { flag: "-i", description: "Interactive: prompt before overwriting", example: Some("mv -i old.txt new.txt") },
                FlagInfo { flag: "-v", description: "Verbose: show what is being moved", example: None },
                FlagInfo { flag: "-n", description: "No-clobber: don't overwrite existing files", example: None },
            ],
            examples: vec![
                Example { command: "mv old.txt new.txt", description: "Rename a file", use_case: "Change a filename" },
                Example { command: "mv file.txt folder/", description: "Move to directory", use_case: "Relocate a file" },
            ],
            related_commands: vec!["cp", "rename"],
        });

        self.register(CommandInfo {
            name: "mkdir".to_string(),
            category: CommandCategory::FileManagement,
            danger_level: DangerLevel::Safe,
            summary: "Make directories",
            description: "Creates new folders (directories) to organize your files, just like creating a new folder in a file browser. You can create a single folder with 'mkdir foldername' or create nested folders all at once with 'mkdir -p parent/child/grandchild'. The -p flag is especially handy because it creates all the parent folders if they don't exist, instead of giving you an error.",
            common_flags: vec![
                FlagInfo { flag: "-p", description: "Parents: create parent directories as needed", example: Some("mkdir -p a/b/c") },
                FlagInfo { flag: "-v", description: "Verbose: print a message for each created directory", example: None },
            ],
            examples: vec![
                Example { command: "mkdir project", description: "Create a directory", use_case: "Make a new folder" },
                Example { command: "mkdir -p path/to/folder", description: "Create nested directories", use_case: "Create directory structure in one command" },
            ],
            related_commands: vec!["rmdir", "touch"],
        });

        self.register(CommandInfo {
            name: "touch".to_string(),
            category: CommandCategory::FileManagement,
            danger_level: DangerLevel::Safe,
            summary: "Create empty files or update timestamps",
            description: "Quickly creates a new empty file or updates when an existing file was last accessed. Most commonly used to create placeholder files like 'touch newfile.txt' which creates an empty file you can edit later. If the file already exists, it just updates the timestamp without changing the content. Useful for testing scripts, creating template files, or triggering systems that watch for file changes.",
            common_flags: vec![
                FlagInfo { flag: "-a", description: "Access time: change only the access time", example: None },
                FlagInfo { flag: "-m", description: "Modification time: change only the modification time", example: None },
            ],
            examples: vec![
                Example { command: "touch file.txt", description: "Create an empty file", use_case: "Initialize a new file" },
                Example { command: "touch *.txt", description: "Update timestamps of multiple files", use_case: "Refresh file modification times" },
            ],
            related_commands: vec!["mkdir", "echo"],
        });

        // Text processing and viewing
        self.register(CommandInfo {
            name: "cat".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Concatenate and display file contents",
            description: "Displays the entire contents of a file right in your terminal, like opening a text file to read it. The name comes from 'concatenate' because it can also combine multiple files together. Use it for quick peeks at small files or config files. For large files, use 'less' or 'head' instead since cat will dump everything at once. You can also use it to combine files like 'cat file1.txt file2.txt > combined.txt'.",
            common_flags: vec![
                FlagInfo { flag: "-n", description: "Number: number all output lines", example: Some("cat -n file.txt") },
                FlagInfo { flag: "-A", description: "Show all: display non-printing characters", example: None },
            ],
            examples: vec![
                Example { command: "cat file.txt", description: "Display file contents", use_case: "Read a file" },
                Example { command: "cat file1.txt file2.txt > combined.txt", description: "Combine files", use_case: "Merge multiple files into one" },
            ],
            related_commands: vec!["less", "more", "head", "tail"],
        });

        self.register(CommandInfo {
            name: "less".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "View file contents with pagination",
            description: "Opens files in an interactive viewer where you can scroll up and down through the content, unlike 'cat' which dumps everything at once. The name means 'less is more' - it's an improved version of the older 'more' command. Use arrow keys or Page Up/Down to navigate, press '/' to search for text, and 'q' to quit. Perfect for reading log files, documentation, or any file too long to fit on one screen. You can also pipe command output into it like 'ls -la | less'.",
            common_flags: vec![
                FlagInfo { flag: "-N", description: "Show line numbers", example: Some("less -N file.txt") },
                FlagInfo { flag: "-S", description: "Chop long lines instead of wrapping", example: None },
            ],
            examples: vec![
                Example { command: "less largefile.log", description: "View large file", use_case: "Browse long files efficiently" },
                Example { command: "ls -la | less", description: "Pipe output to less", use_case: "Paginate command output" },
            ],
            related_commands: vec!["more", "cat", "head", "tail"],
        });

        self.register(CommandInfo {
            name: "head".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Display first lines of a file",
            description: "Shows just the beginning of a file, like peeking at the first page of a book. By default it displays the first 10 lines, but you can change this with the -n flag. Super useful for quickly checking what's in a file without opening the whole thing, or for seeing the header row of a CSV file. For example, 'head -n 20 file.txt' shows the first 20 lines. Often paired with 'tail' to see the end of a file.",
            common_flags: vec![
                FlagInfo { flag: "-n", description: "Number: specify how many lines to show", example: Some("head -n 20 file.txt") },
            ],
            examples: vec![
                Example { command: "head file.txt", description: "Show first 10 lines", use_case: "Preview file contents" },
                Example { command: "head -n 5 file.txt", description: "Show first 5 lines", use_case: "Quick peek at file start" },
            ],
            related_commands: vec!["tail", "cat", "less"],
        });

        self.register(CommandInfo {
            name: "tail".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Display last lines of a file",
            description: "Shows just the end of a file, like reading the last page of a book. By default it displays the last 10 lines, perfect for checking recent log entries or the end of a file. The -f flag is incredibly useful - it 'follows' the file, continuously showing new lines as they're added. This is essential for watching log files in real-time as your application runs. For example, 'tail -f /var/log/app.log' lets you watch errors appear live while debugging.",
            common_flags: vec![
                FlagInfo { flag: "-n", description: "Number: specify how many lines to show", example: Some("tail -n 20 file.txt") },
                FlagInfo { flag: "-f", description: "Follow: continuously show new lines as they're added", example: Some("tail -f /var/log/syslog") },
            ],
            examples: vec![
                Example { command: "tail file.txt", description: "Show last 10 lines", use_case: "See end of file" },
                Example { command: "tail -f app.log", description: "Follow log file in real-time", use_case: "Monitor live log output" },
            ],
            related_commands: vec!["head", "cat", "less"],
        });

        self.register(CommandInfo {
            name: "grep".to_string(),
            category: CommandCategory::Search,
            danger_level: DangerLevel::Safe,
            summary: "Search for patterns in files",
            description: "Searches through files to find lines containing specific text, like using Ctrl+F in a document editor but much more powerful. You can search one file, multiple files, or even entire directory trees. It's case-sensitive by default, but -i makes it ignore case. The -r flag searches recursively through all files in folders. Super useful for finding where something is mentioned in your code, logs, or config files. For example, 'grep error app.log' finds all lines containing 'error' in your log file.",
            common_flags: vec![
                FlagInfo { flag: "-i", description: "Ignore case: make search case-insensitive", example: Some("grep -i \"error\" log.txt") },
                FlagInfo { flag: "-r", description: "Recursive: search in all files in directories", example: Some("grep -r \"TODO\" .") },
                FlagInfo { flag: "-n", description: "Line number: show line numbers with matches", example: Some("grep -n \"function\" code.js") },
                FlagInfo { flag: "-v", description: "Invert: show lines that DON'T match", example: None },
            ],
            examples: vec![
                Example { command: "grep \"error\" app.log", description: "Find errors in log", use_case: "Debug application issues" },
                Example { command: "grep -r \"TODO\" src/", description: "Search all source files", use_case: "Find all TODO comments in codebase" },
            ],
            related_commands: vec!["find", "awk", "sed"],
        });

        self.register(CommandInfo {
            name: "find".to_string(),
            category: CommandCategory::Search,
            danger_level: DangerLevel::Safe,
            summary: "Search for files and directories",
            description: "Searches your filesystem to locate files and folders matching specific criteria like name, size, or modification date. Unlike grep which searches inside files, find searches for the files themselves. For example, 'find . -name \"*.txt\"' finds all text files in the current directory and subdirectories. It's incredibly powerful but the syntax can be tricky at first. You can combine multiple criteria and even execute commands on the files it finds.",
            common_flags: vec![
                FlagInfo { flag: "-name", description: "Search by filename pattern", example: Some("find . -name \"*.txt\"") },
                FlagInfo { flag: "-type", description: "Filter by type (f=file, d=directory)", example: Some("find . -type f") },
                FlagInfo { flag: "-size", description: "Filter by file size", example: Some("find . -size +10M") },
            ],
            examples: vec![
                Example { command: "find . -name \"*.log\"", description: "Find all log files", use_case: "Locate files by extension" },
                Example { command: "find /home -type f -mtime -7", description: "Files modified in last 7 days", use_case: "Find recently changed files" },
            ],
            related_commands: vec!["locate", "grep", "which"],
        });

        // System information
        self.register(CommandInfo {
            name: "ps".to_string(),
            category: CommandCategory::ProcessManagement,
            danger_level: DangerLevel::Safe,
            summary: "Display running processes",
            description: "Shows a snapshot of all the programs currently running on your system, like Task Manager on Windows or Activity Monitor on Mac. Each running program is called a 'process' and has a unique process ID (PID). The most common usage is 'ps aux' which shows all processes with detailed info including who's running them and how much CPU and memory they're using. Often combined with grep like 'ps aux | grep nginx' to find a specific program and check if it's running.",
            common_flags: vec![
                FlagInfo { flag: "aux", description: "All processes with detailed info (common usage)", example: Some("ps aux") },
                FlagInfo { flag: "-ef", description: "Full format listing of all processes", example: Some("ps -ef") },
            ],
            examples: vec![
                Example { command: "ps aux", description: "Show all processes", use_case: "System overview" },
                Example { command: "ps aux | grep nginx", description: "Find specific process", use_case: "Check if program is running" },
            ],
            related_commands: vec!["top", "htop", "kill", "pgrep"],
        });

        self.register(CommandInfo {
            name: "top".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Display dynamic real-time system information",
            description: "Opens an interactive, live-updating display of your system's processes, like a continuously refreshing Task Manager. Shows which programs are using the most CPU and memory, updates every few seconds, and lets you see exactly what's happening on your system right now. Unlike 'ps' which gives you a one-time snapshot, 'top' keeps running and updating. Press 'q' to quit when you're done. Very useful for finding programs that are hogging resources or checking if your system is under heavy load.",
            common_flags: vec![],
            examples: vec![
                Example { command: "top", description: "Monitor system resources", use_case: "Check CPU and memory usage in real-time" },
            ],
            related_commands: vec!["htop", "ps", "free"],
        });

        self.register(CommandInfo {
            name: "df".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Report filesystem disk space usage",
            description: "Reports how much disk space is free and used on all your drives and partitions, like checking your computer's storage in system settings. The name stands for 'disk free'. Use 'df -h' (human-readable) to see sizes in GB/MB instead of confusing block counts. Perfect for quickly checking if you're running out of disk space or seeing which partition is getting full. Shows mounted filesystems including external drives, network shares, and your main hard drive.",
            common_flags: vec![
                FlagInfo { flag: "-h", description: "Human-readable: show sizes in GB, MB, KB", example: Some("df -h") },
            ],
            examples: vec![
                Example { command: "df -h", description: "Show disk usage", use_case: "Check available disk space" },
            ],
            related_commands: vec!["du", "free"],
        });

        self.register(CommandInfo {
            name: "du".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Estimate file and directory space usage",
            description: "Shows how much disk space individual files and directories are consuming, helping you find what's hogging all your storage. The name stands for 'disk usage'. While 'df' shows overall disk space, 'du' drills down to show you which folders are taking up room. Use 'du -sh *' to see the size of each item in the current directory, or 'du -h --max-depth=1' to avoid diving too deep into subdirectories. Essential for finding and cleaning up large files when you're running low on space.",
            common_flags: vec![
                FlagInfo { flag: "-h", description: "Human-readable sizes", example: Some("du -h folder/") },
                FlagInfo { flag: "-s", description: "Summary: show only total for each argument", example: Some("du -sh *") },
            ],
            examples: vec![
                Example { command: "du -sh *", description: "Size of each item in current directory", use_case: "Find what's taking up space" },
                Example { command: "du -h --max-depth=1", description: "Show directory sizes one level deep", use_case: "Identify large directories" },
            ],
            related_commands: vec!["df", "ls"],
        });

        self.register(CommandInfo {
            name: "chmod".to_string(),
            category: CommandCategory::Permissions,
            danger_level: DangerLevel::Caution,
            summary: "Change file permissions",
            description: "Controls who can read, write, or execute a file. Linux files have three permission levels: owner, group, and everyone else. Each can have read (r), write (w), and execute (x) permissions. You'll often see it used like 'chmod +x script.sh' to make a script executable, or 'chmod 644 file.txt' using numbers where 6=read+write, 4=read only. The three digits represent owner, group, and others. Understanding chmod is essential for security and making scripts runnable.",
            common_flags: vec![
                FlagInfo { flag: "-R", description: "Recursive: change permissions of directory and contents", example: Some("chmod -R 755 folder/") },
            ],
            examples: vec![
                Example { command: "chmod 755 script.sh", description: "Make file executable", use_case: "Allow script to run" },
                Example { command: "chmod u+x file", description: "Add execute permission for owner", use_case: "Grant execution rights" },
            ],
            related_commands: vec!["chown", "chgrp", "ls -l"],
        });

        self.register(CommandInfo {
            name: "chown".to_string(),
            category: CommandCategory::Permissions,
            danger_level: DangerLevel::Dangerous,
            summary: "Change file owner and group",
            description: "Changes which user and group 'owns' a file or directory, controlling who has ultimate authority over it. In Linux, every file has an owner and a group, determining who can access it. This command usually requires sudo since you need admin rights to transfer ownership. Use it like 'sudo chown username file.txt' to change the owner, or 'sudo chown user:group file.txt' to set both. Common when moving files between users or fixing permission issues on servers.",
            common_flags: vec![
                FlagInfo { flag: "-R", description: "Recursive: change ownership recursively", example: Some("chown -R user:group folder/") },
            ],
            examples: vec![
                Example { command: "chown user file.txt", description: "Change file owner", use_case: "Transfer file ownership" },
                Example { command: "chown user:group file.txt", description: "Change owner and group", use_case: "Set both owner and group" },
            ],
            related_commands: vec!["chmod", "chgrp"],
        });

        // Archiving and compression
        self.register(CommandInfo {
            name: "tar".to_string(),
            category: CommandCategory::Archiving,
            danger_level: DangerLevel::Caution,
            summary: "Archive files",
            description: "Creates and extracts .tar archives, which bundle multiple files and folders into a single file for easier storage or transfer. Think of it as creating a ZIP file, but the standard format for Linux. The name stands for 'tape archive' from the old days of backup tapes. Usually combined with compression: 'tar -czf backup.tar.gz folder/' creates a compressed archive, and 'tar -xzf backup.tar.gz' extracts it. The -v flag shows which files are being processed, helpful for large archives.",
            common_flags: vec![
                FlagInfo { flag: "-c", description: "Create: create a new archive", example: Some("tar -czf archive.tar.gz folder/") },
                FlagInfo { flag: "-x", description: "Extract: extract files from archive", example: Some("tar -xzf archive.tar.gz") },
                FlagInfo { flag: "-z", description: "Gzip: compress/decompress with gzip", example: None },
                FlagInfo { flag: "-v", description: "Verbose: list files being processed", example: None },
                FlagInfo { flag: "-f", description: "File: specify archive filename", example: None },
            ],
            examples: vec![
                Example { command: "tar -czf backup.tar.gz folder/", description: "Create compressed archive", use_case: "Backup directory" },
                Example { command: "tar -xzf archive.tar.gz", description: "Extract compressed archive", use_case: "Unpack tarball" },
            ],
            related_commands: vec!["gzip", "zip", "unzip"],
        });

        // Networking
        self.register(CommandInfo {
            name: "curl".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Safe,
            summary: "Transfer data from or to a server",
            description: "Fetches data from or sends data to web servers and other network resources, like a command-line web browser. Supports many protocols including HTTP, HTTPS, and FTP. Super useful for testing APIs, downloading files, or checking if a website is responding. For example, 'curl https://api.example.com' fetches and displays the response, while 'curl -O url' downloads a file. Developers use it constantly for debugging web services and making API requests from scripts.",
            common_flags: vec![
                FlagInfo { flag: "-O", description: "Output: save with remote filename", example: Some("curl -O https://example.com/file.zip") },
                FlagInfo { flag: "-L", description: "Location: follow redirects", example: None },
            ],
            examples: vec![
                Example { command: "curl https://api.example.com", description: "Fetch URL content", use_case: "Test API endpoints" },
                Example { command: "curl -O https://example.com/file.zip", description: "Download file", use_case: "Download from URL" },
            ],
            related_commands: vec!["wget", "http"],
        });

        self.register(CommandInfo {
            name: "wget".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Safe,
            summary: "Network downloader",
            description: "Downloads files from the web directly to your computer, like a download manager for the command line. Unlike curl which can do many things, wget specializes in downloading. It's great for downloading large files because it can resume interrupted downloads with the -c flag. Use it like 'wget https://example.com/file.zip' and it saves the file to your current directory. Can also download entire websites recursively with the -r flag, making it perfect for offline viewing or backups.",
            common_flags: vec![
                FlagInfo { flag: "-c", description: "Continue: resume interrupted download", example: Some("wget -c https://example.com/large.iso") },
                FlagInfo { flag: "-r", description: "Recursive: download entire website", example: None },
            ],
            examples: vec![
                Example { command: "wget https://example.com/file.zip", description: "Download file", use_case: "Fetch file from web" },
            ],
            related_commands: vec!["curl", "aria2c"],
        });

        self.register(CommandInfo {
            name: "ping".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Safe,
            summary: "Test network connectivity",
            description: "Tests if another computer or website is reachable over the network by sending small test packets and measuring the response time. Like knocking on a door to see if anyone's home. The response time (latency) is shown in milliseconds - lower is better. Use 'ping google.com' to check if your internet is working, or 'ping 192.168.1.1' to test your router connection. It keeps running until you stop it with Ctrl+C, or use -c to send a specific number of packets like 'ping -c 4 google.com'.",
            common_flags: vec![
                FlagInfo { flag: "-c", description: "Count: number of packets to send", example: Some("ping -c 4 google.com") },
            ],
            examples: vec![
                Example { command: "ping google.com", description: "Test internet connection", use_case: "Check if host is reachable" },
                Example { command: "ping -c 4 192.168.1.1", description: "Send 4 packets", use_case: "Quick connectivity test" },
            ],
            related_commands: vec!["traceroute", "netstat"],
        });

        // Git commands
        self.register(CommandInfo {
            name: "git".to_string(),
            category: CommandCategory::Git,
            danger_level: DangerLevel::Caution,
            summary: "Distributed version control system",
            description: "The industry-standard tool for tracking changes to code and collaborating with other developers. It's like having unlimited undo for your entire project, with the ability to work on different features in parallel and merge them together. Every change you make can be saved as a 'commit' with a description of what you did. Essential for modern software development - nearly every coding project uses git. Common commands include 'git status' to see what changed, 'git add' to stage files, and 'git commit' to save your changes to history.",
            common_flags: vec![],
            examples: vec![
                Example { command: "git status", description: "Check repository status", use_case: "See what's changed" },
                Example { command: "git add .", description: "Stage all changes", use_case: "Prepare files for commit" },
                Example { command: "git commit -m \"message\"", description: "Save changes", use_case: "Record changes to repository" },
            ],
            related_commands: vec!["svn", "hg"],
        });

        self.register(CommandInfo {
            name: "echo".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Display a line of text",
            description: "Prints whatever you type directly to the screen, like a simple print statement. Commonly used to display messages in scripts, show the value of variables like 'echo $PATH', or write text to files using redirection like 'echo \"Hello\" > file.txt'. You can use it for debugging scripts by echoing variable values to see what's happening, or to create simple text files without opening an editor.",
            common_flags: vec![
                FlagInfo { flag: "-n", description: "No newline: don't output trailing newline", example: Some("echo -n \"text\"") },
                FlagInfo { flag: "-e", description: "Enable interpretation of backslash escapes", example: None },
            ],
            examples: vec![
                Example { command: "echo \"Hello World\"", description: "Print text", use_case: "Display message" },
                Example { command: "echo \"text\" > file.txt", description: "Write to file", use_case: "Create file with content" },
            ],
            related_commands: vec!["printf", "cat"],
        });

        self.register(CommandInfo {
            name: "man".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Display manual pages",
            description: "Opens the built-in manual (documentation) for any command, like a help system right in your terminal. Short for 'manual'. Use it whenever you want to learn what a command does and what flags it accepts. For example, 'man ls' shows you everything about the ls command. The manual opens in a pager (like less), so you can scroll through it and press 'q' to quit. Every serious Linux user keeps 'man' handy - it's how you look things up without leaving the terminal.",
            common_flags: vec![],
            examples: vec![
                Example { command: "man ls", description: "Read ls documentation", use_case: "Learn about command options" },
            ],
            related_commands: vec!["help", "info", "whatis"],
        });

        self.register(CommandInfo {
            name: "sudo".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Critical,
            summary: "Execute command as superuser",
            description: "Careful: Runs commands with administrator (root) privileges, giving you full power over the system. Short for 'superuser do'. Like clicking 'Run as Administrator' on Windows - it bypasses normal security restrictions. You'll need to enter your password the first time you use it. Essential for system administration tasks like installing software, modifying system files, or changing settings. Be VERY careful what you run with sudo - you can accidentally damage your entire system since there are no safety nets!",
            common_flags: vec![
                FlagInfo { flag: "-u", description: "User: run as specified user instead of root", example: Some("sudo -u user command") },
            ],
            examples: vec![
                Example { command: "sudo apt update", description: "Update package list", use_case: "System maintenance with root privileges" },
            ],
            related_commands: vec!["su", "doas"],
        });

        self.register(CommandInfo {
            name: "whoami".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Print current user name",
            description: "Displays your current username - literally tells you 'who am I' in the system. Simple but useful, especially when you're working on multiple computers, have switched users with 'su', or are sshed into a remote server. If you're not sure what account you're using or need to confirm you're the right user before doing something important, just type 'whoami'. Often used in scripts to check which user is running them.",
            common_flags: vec![],
            examples: vec![
                Example { command: "whoami", description: "Show current username", use_case: "Verify which user you are" },
            ],
            related_commands: vec!["id", "who"],
        });

        self.register(CommandInfo {
            name: "history".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Show command history",
            description: "Shows a numbered list of all the commands you've recently run in your terminal, like a browser history for the command line. Super useful for finding and reusing complex commands you typed earlier without retyping them. You can re-run a command by typing '!123' where 123 is the command number. Combine with grep like 'history | grep git' to find all git commands you've used. Your shell remembers hundreds or thousands of commands between sessions.",
            common_flags: vec![],
            examples: vec![
                Example { command: "history", description: "Show recent commands", use_case: "Find and reuse previous commands" },
                Example { command: "history | grep git", description: "Search command history", use_case: "Find specific past commands" },
            ],
            related_commands: vec!["fc", "!!"],
        });

        self.register(CommandInfo {
            name: "clear".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Clear the terminal screen",
            description: "Wipes all the text from your terminal screen, giving you a fresh clean view. Like clearing a whiteboard - everything disappears visually, but your command history is still saved and can be accessed with the up arrow or 'history' command. Useful when your terminal gets cluttered with output and you want to start fresh, or before taking a screenshot. You can also use Ctrl+L as a keyboard shortcut in most terminals.",
            common_flags: vec![],
            examples: vec![
                Example { command: "clear", description: "Clean terminal screen", use_case: "Start fresh with clean view" },
            ],
            related_commands: vec!["reset"],
        });

        // Advanced text processing
        self.register(CommandInfo {
            name: "sed".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Caution,
            summary: "Stream editor for filtering and transforming text",
            description: "A powerful tool for automatically editing text, like find-and-replace on steroids. The name means 'stream editor' because it processes text line by line. Most commonly used for search-and-replace operations like 'sed 's/old/new/g' file.txt' which replaces all occurrences of 'old' with 'new'. The -i flag edits files in place, which is convenient but risky since it modifies the original file. Can also extract specific lines, delete lines, and perform complex text transformations. The syntax is cryptic but extremely powerful once you learn it.",
            common_flags: vec![
                FlagInfo { flag: "-i", description: "In-place: edit files in place (careful!)", example: Some("sed -i 's/old/new/g' file.txt") },
                FlagInfo { flag: "-e", description: "Expression: add script to be executed", example: None },
            ],
            examples: vec![
                Example { command: "sed 's/old/new/g' file.txt", description: "Replace text", use_case: "Find and replace in file" },
                Example { command: "sed -n '10,20p' file.txt", description: "Print lines 10-20", use_case: "Extract specific lines" },
            ],
            related_commands: vec!["awk", "grep", "tr"],
        });

        self.register(CommandInfo {
            name: "awk".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Pattern scanning and text processing language",
            description: "A text processing powerhouse that excels at working with columnar data like CSV files, logs, or tables. It automatically splits each line into fields (columns) and lets you manipulate them. For example, 'awk '{print $1}' file.txt' prints just the first column of each line. Great for extracting specific columns from data, doing calculations, filtering rows, and formatting output. More powerful than cut but simpler than writing a full script. The -F flag lets you specify what separates columns, like commas or colons.",
            common_flags: vec![
                FlagInfo { flag: "-F", description: "Field separator: specify delimiter", example: Some("awk -F':' '{print $1}' /etc/passwd") },
            ],
            examples: vec![
                Example { command: "awk '{print $1}' file.txt", description: "Print first column", use_case: "Extract specific fields from text" },
                Example { command: "awk -F':' '{print $1}' /etc/passwd", description: "List all usernames", use_case: "Parse colon-separated data" },
            ],
            related_commands: vec!["sed", "cut", "grep"],
        });

        self.register(CommandInfo {
            name: "sort".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Sort lines of text files",
            description: "Arranges lines of text in order, either alphabetically or numerically. Like organizing a list in alphabetical order. By default it sorts alphabetically A-Z, but use -n for numeric sorting (so '10' comes after '2' instead of before). The -r flag reverses the sort order, and -u removes duplicate lines while sorting. Perfect for organizing lists, finding the top/bottom items in data, or preparing input for other commands like 'uniq' which requires sorted input to work properly.",
            common_flags: vec![
                FlagInfo { flag: "-n", description: "Numeric: sort by numerical value", example: Some("sort -n numbers.txt") },
                FlagInfo { flag: "-r", description: "Reverse: sort in descending order", example: Some("sort -r file.txt") },
                FlagInfo { flag: "-u", description: "Unique: remove duplicate lines", example: None },
            ],
            examples: vec![
                Example { command: "sort file.txt", description: "Sort alphabetically", use_case: "Organize text lines" },
                Example { command: "sort -n -r sizes.txt", description: "Sort numbers descending", use_case: "Find largest values" },
            ],
            related_commands: vec!["uniq", "cut"],
        });

        self.register(CommandInfo {
            name: "uniq".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Report or omit repeated lines",
            description: "Removes duplicate lines from text, but only if they're adjacent (next to each other). That's why it's almost always used with 'sort' first - 'sort file.txt | uniq' gives you all unique lines. The -c flag is super useful: it counts how many times each line appears, perfect for finding the most common items in a list. For example, analyzing log files to count how many times each error occurred. Simple but powerful for data analysis tasks.",
            common_flags: vec![
                FlagInfo { flag: "-c", description: "Count: prefix lines with occurrence count", example: Some("sort file.txt | uniq -c") },
                FlagInfo { flag: "-d", description: "Duplicates: only print duplicate lines", example: None },
            ],
            examples: vec![
                Example { command: "sort file.txt | uniq", description: "Get unique lines", use_case: "Remove duplicates from file" },
                Example { command: "sort file.txt | uniq -c", description: "Count occurrences", use_case: "Find most common entries" },
            ],
            related_commands: vec!["sort", "comm"],
        });

        self.register(CommandInfo {
            name: "cut".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Remove sections from lines of files",
            description: "Extracts specific columns or character positions from each line of text, like cutting out columns from a spreadsheet. Great for working with CSV files, tab-separated data, or any structured text. Use -d to specify the delimiter (what separates columns) and -f to pick which fields you want. For example, 'cut -d',' -f1,3 data.csv' grabs just the 1st and 3rd columns from a CSV. Simpler than awk for basic column extraction but less flexible for complex tasks.",
            common_flags: vec![
                FlagInfo { flag: "-d", description: "Delimiter: specify field separator", example: Some("cut -d',' -f1,3 data.csv") },
                FlagInfo { flag: "-f", description: "Fields: select specific fields", example: Some("cut -f1-3 file.txt") },
                FlagInfo { flag: "-c", description: "Characters: select character positions", example: None },
            ],
            examples: vec![
                Example { command: "cut -d',' -f1,3 data.csv", description: "Extract CSV columns", use_case: "Parse CSV files" },
                Example { command: "cut -c1-10 file.txt", description: "First 10 characters", use_case: "Truncate lines" },
            ],
            related_commands: vec!["awk", "paste"],
        });

        self.register(CommandInfo {
            name: "wc".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Print newline, word, and byte counts",
            description: "Counts lines, words, and characters (bytes) in files, giving you quick statistics about text. The name stands for 'word count'. By default shows all three counts, but you can use -l for just lines, -w for just words, or -c for just characters. Super useful for quickly checking how long a file is, counting entries in a list, or seeing how much code you've written. For example, 'wc -l file.txt' tells you exactly how many lines are in a file.",
            common_flags: vec![
                FlagInfo { flag: "-l", description: "Lines: count only lines", example: Some("wc -l file.txt") },
                FlagInfo { flag: "-w", description: "Words: count only words", example: Some("wc -w file.txt") },
                FlagInfo { flag: "-c", description: "Bytes: count only bytes", example: None },
            ],
            examples: vec![
                Example { command: "wc file.txt", description: "Count lines, words, bytes", use_case: "Get file statistics" },
                Example { command: "wc -l *.txt", description: "Count lines in all text files", use_case: "Compare file sizes" },
            ],
            related_commands: vec!["cat", "du"],
        });

        // Process management
        self.register(CommandInfo {
            name: "kill".to_string(),
            category: CommandCategory::ProcessManagement,
            danger_level: DangerLevel::Dangerous,
            summary: "Terminate processes",
            description: "Sends signals to running processes, usually to stop them. Despite the name, it doesn't always 'kill' - it sends different types of signals. The default signal (SIGTERM) asks a process to shut down gracefully, giving it time to clean up. Use 'kill -9' (SIGKILL) as a last resort to forcefully terminate unresponsive processes - this doesn't give them a chance to save data. You need the process ID (PID) which you can get from 'ps' or 'top'. For example, 'kill 1234' stops process 1234.",
            common_flags: vec![
                FlagInfo { flag: "-9", description: "SIGKILL: force kill (last resort)", example: Some("kill -9 1234") },
                FlagInfo { flag: "-15", description: "SIGTERM: graceful termination (default)", example: None },
            ],
            examples: vec![
                Example { command: "kill 1234", description: "Terminate process 1234", use_case: "Stop a running process" },
                Example { command: "kill -9 1234", description: "Force kill process", use_case: "Stop unresponsive process" },
            ],
            related_commands: vec!["killall", "pkill", "ps"],
        });

        self.register(CommandInfo {
            name: "killall".to_string(),
            category: CommandCategory::ProcessManagement,
            danger_level: DangerLevel::Dangerous,
            summary: "Kill processes by name",
            description: "Careful: Terminates ALL processes that match a program name, which is more convenient than 'kill' when you don't know the PID but know the program name. Be careful - it will stop EVERY instance of that program! For example, 'killall firefox' closes all Firefox windows at once. Useful for cleaning up multiple stuck processes or ensuring all instances of a program are stopped. The -9 flag forces immediate termination. Double-check the name before running this - typos can stop the wrong programs!",
            common_flags: vec![
                FlagInfo { flag: "-9", description: "Force kill all matching processes", example: Some("killall -9 firefox") },
            ],
            examples: vec![
                Example { command: "killall firefox", description: "Kill all Firefox processes", use_case: "Close all instances of a program" },
            ],
            related_commands: vec!["kill", "pkill"],
        });

        self.register(CommandInfo {
            name: "pkill".to_string(),
            category: CommandCategory::ProcessManagement,
            danger_level: DangerLevel::Dangerous,
            summary: "Signal processes based on name and attributes",
            description: "A smarter version of 'killall' that can filter processes by various criteria like user, terminal, or partial name matches. For example, 'pkill -u username' kills all processes owned by that user, and 'pkill chrome' kills anything with 'chrome' in the name. More flexible than killall because you can target specific subsets of processes. The pattern matching uses regular expressions, so it's powerful but requires care. Check what you're about to kill with 'pgrep' first using the same pattern.",
            common_flags: vec![
                FlagInfo { flag: "-u", description: "User: kill processes owned by user", example: Some("pkill -u username") },
            ],
            examples: vec![
                Example { command: "pkill chrome", description: "Kill processes matching 'chrome'", use_case: "Stop browser processes" },
            ],
            related_commands: vec!["kill", "killall", "pgrep"],
        });

        self.register(CommandInfo {
            name: "bg".to_string(),
            category: CommandCategory::ProcessManagement,
            danger_level: DangerLevel::Safe,
            summary: "Resume jobs in background",
            description: "Continues running a program in the background after you've paused it with Ctrl+Z. When you press Ctrl+Z, a program suspends (freezes). The 'bg' command resumes it but lets it run in the background so you can use your terminal for other things. Useful when you realize a long-running task is blocking your terminal and you want to continue working. Pair it with 'fg' to bring jobs back to the foreground, and 'jobs' to see all background tasks.",
            common_flags: vec![],
            examples: vec![
                Example { command: "bg", description: "Resume last suspended job", use_case: "Continue process in background" },
                Example { command: "bg %1", description: "Resume job 1 in background", use_case: "Resume specific job" },
            ],
            related_commands: vec!["fg", "jobs", "&"],
        });

        self.register(CommandInfo {
            name: "fg".to_string(),
            category: CommandCategory::ProcessManagement,
            danger_level: DangerLevel::Safe,
            summary: "Bring jobs to foreground",
            description: "Brings a background or paused program back to the foreground, making it the active process in your terminal again. If you have a program running in the background or you suspended it with Ctrl+Z, 'fg' returns control to it so you can interact with it. For example, if you backgrounded a text editor, 'fg' brings it back so you can type in it again. Use 'jobs' to see all available jobs and their numbers, then 'fg %2' to bring job 2 to the foreground.",
            common_flags: vec![],
            examples: vec![
                Example { command: "fg", description: "Bring last job to foreground", use_case: "Return to background process" },
                Example { command: "fg %1", description: "Bring job 1 to foreground", use_case: "Resume specific background job" },
            ],
            related_commands: vec!["bg", "jobs", "Ctrl+Z"],
        });

        self.register(CommandInfo {
            name: "jobs".to_string(),
            category: CommandCategory::ProcessManagement,
            danger_level: DangerLevel::Safe,
            summary: "List active jobs",
            description: "Lists all the programs currently running or paused in your terminal session, showing their job numbers and status. Each background or suspended task gets a job number (like [1], [2]) that you can use with 'fg' or 'bg' commands. Shows whether each job is running, stopped, or done. This only shows jobs started from your current shell, not all system processes - use 'ps' for that. Essential for managing multiple tasks in one terminal window.",
            common_flags: vec![
                FlagInfo { flag: "-l", description: "Long format: show process IDs", example: Some("jobs -l") },
            ],
            examples: vec![
                Example { command: "jobs", description: "List all jobs", use_case: "See background processes" },
            ],
            related_commands: vec!["bg", "fg", "ps"],
        });

        // Compression and archiving
        self.register(CommandInfo {
            name: "gzip".to_string(),
            category: CommandCategory::Archiving,
            danger_level: DangerLevel::Caution,
            summary: "Compress files",
            description: "Compresses files to save disk space, creating .gz files that are typically 60-70% smaller. By default it replaces the original file with the compressed version, so use -k to keep the original. Common for compressing log files, backups, or any large files you want to store. To decompress, use 'gunzip file.gz' or 'gzip -d file.gz'. Often combined with tar like 'tar -czf' to create compressed archives. Good balance of compression speed and file size reduction.",
            common_flags: vec![
                FlagInfo { flag: "-k", description: "Keep: keep original file", example: Some("gzip -k file.txt") },
                FlagInfo { flag: "-d", description: "Decompress: uncompress .gz files", example: Some("gzip -d file.gz") },
            ],
            examples: vec![
                Example { command: "gzip file.txt", description: "Compress file", use_case: "Reduce file size" },
                Example { command: "gzip -k *.log", description: "Compress keeping originals", use_case: "Archive log files" },
            ],
            related_commands: vec!["gunzip", "tar", "bzip2"],
        });

        self.register(CommandInfo {
            name: "gunzip".to_string(),
            category: CommandCategory::Archiving,
            danger_level: DangerLevel::Safe,
            summary: "Decompress gzip files",
            description: "Decompresses .gz files back to their original form, restoring files that were compressed with gzip. It's actually just a convenient wrapper around 'gzip -d'. By default it removes the .gz file after decompression, leaving you with the original uncompressed file. Use it like 'gunzip file.txt.gz' to get back file.txt. Safe and straightforward - just unzips compressed files so you can use them again.",
            common_flags: vec![],
            examples: vec![
                Example { command: "gunzip file.gz", description: "Decompress file", use_case: "Extract compressed file" },
            ],
            related_commands: vec!["gzip", "tar"],
        });

        self.register(CommandInfo {
            name: "zip".to_string(),
            category: CommandCategory::Archiving,
            danger_level: DangerLevel::Safe,
            summary: "Package and compress files",
            description: "Creates .zip archives that work across all operating systems - Windows, Mac, and Linux. While Linux prefers .tar.gz, ZIP files are perfect for sharing with Windows users or creating cross-platform archives. Use 'zip archive.zip file1 file2' to zip individual files, or 'zip -r archive.zip folder/' to zip an entire directory with -r (recursive). The original files stay untouched. Everyone knows how to open ZIP files, making this the most universal archive format.",
            common_flags: vec![
                FlagInfo { flag: "-r", description: "Recursive: include directories", example: Some("zip -r archive.zip folder/") },
            ],
            examples: vec![
                Example { command: "zip archive.zip file1.txt file2.txt", description: "Create zip archive", use_case: "Package files for sharing" },
                Example { command: "zip -r backup.zip project/", description: "Zip entire directory", use_case: "Archive project folder" },
            ],
            related_commands: vec!["unzip", "tar", "gzip"],
        });

        self.register(CommandInfo {
            name: "unzip".to_string(),
            category: CommandCategory::Archiving,
            danger_level: DangerLevel::Safe,
            summary: "Extract compressed files from ZIP archive",
            description: "Extracts files from .zip archives, unpacking them into your current directory or a specified location. Use 'unzip archive.zip' to extract everything, or 'unzip -l archive.zip' to just list the contents without extracting (useful for previewing). The -d flag lets you extract to a specific directory like 'unzip file.zip -d /target/folder'. Works with ZIP files from any operating system. The archives stay intact after extraction so you can unzip them again later.",
            common_flags: vec![
                FlagInfo { flag: "-l", description: "List: show archive contents without extracting", example: Some("unzip -l archive.zip") },
                FlagInfo { flag: "-d", description: "Directory: extract to specific directory", example: Some("unzip file.zip -d dest/") },
            ],
            examples: vec![
                Example { command: "unzip archive.zip", description: "Extract zip file", use_case: "Unpack downloaded archive" },
                Example { command: "unzip -l archive.zip", description: "List archive contents", use_case: "Preview files before extracting" },
            ],
            related_commands: vec!["zip", "tar"],
        });

        // Networking
        self.register(CommandInfo {
            name: "ssh".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Caution,
            summary: "Secure shell - remote login",
            description: "Opens a secure, encrypted connection to another computer over the network. Think of it like remote desktop for the command line - you can control another machine as if you were sitting right in front of it. Commonly used to manage servers, run commands on remote computers, or access your work machine from home. The connection is encrypted, so passwords and data stay private even on public networks.",
            common_flags: vec![
                FlagInfo { flag: "-p", description: "Port: specify SSH port", example: Some("ssh -p 2222 user@host") },
                FlagInfo { flag: "-i", description: "Identity: use specific private key", example: Some("ssh -i ~/.ssh/key user@host") },
            ],
            examples: vec![
                Example { command: "ssh user@example.com", description: "Connect to remote server", use_case: "Remote server access" },
                Example { command: "ssh -p 2222 user@host", description: "Connect on custom port", use_case: "Non-standard SSH port" },
            ],
            related_commands: vec!["scp", "sftp", "rsync"],
        });

        self.register(CommandInfo {
            name: "scp".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Caution,
            summary: "Secure copy - transfer files over SSH",
            description: "Securely copies files between your computer and remote servers using SSH encryption, like a secure version of copy-paste over the network. Use it to upload files to a server or download files from one. The syntax is 'scp source destination' where either can be remote (user@host:path). For example, 'scp file.txt user@server:~/' uploads a file, while 'scp user@server:~/file.txt .' downloads it. The -r flag copies entire directories recursively. Note: the port flag is -P (capital P), not -p like ssh!",
            common_flags: vec![
                FlagInfo { flag: "-r", description: "Recursive: copy directories", example: Some("scp -r folder/ user@host:~/") },
                FlagInfo { flag: "-P", description: "Port: specify SSH port (capital P!)", example: Some("scp -P 2222 file user@host:~/") },
            ],
            examples: vec![
                Example { command: "scp file.txt user@host:~/", description: "Copy file to remote", use_case: "Upload file to server" },
                Example { command: "scp user@host:~/file.txt .", description: "Copy file from remote", use_case: "Download from server" },
            ],
            related_commands: vec!["ssh", "rsync", "sftp"],
        });

        self.register(CommandInfo {
            name: "rsync".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Caution,
            summary: "Remote file synchronization",
            description: "An incredibly efficient tool for syncing files and directories between locations, either locally or over the network. Unlike scp which copies everything, rsync is smart - it only transfers the parts of files that changed, making it much faster for updates and backups. The -a flag preserves everything (permissions, timestamps, etc.) and is almost always used. Common for keeping backup copies synchronized, deploying websites, or maintaining identical copies of directories. The --delete flag makes the destination exactly match the source by removing extra files.",
            common_flags: vec![
                FlagInfo { flag: "-a", description: "Archive: preserve permissions, timestamps, etc.", example: Some("rsync -a src/ dest/") },
                FlagInfo { flag: "-v", description: "Verbose: show files being transferred", example: None },
                FlagInfo { flag: "-z", description: "Compress: compress during transfer", example: None },
            ],
            examples: vec![
                Example { command: "rsync -avz folder/ user@host:~/backup/", description: "Sync to remote", use_case: "Backup to server" },
                Example { command: "rsync -av --delete src/ dest/", description: "Mirror directories", use_case: "Keep directories identical" },
            ],
            related_commands: vec!["scp", "cp", "ssh"],
        });

        // Useful utilities
        self.register(CommandInfo {
            name: "ln".to_string(),
            category: CommandCategory::FileManagement,
            danger_level: DangerLevel::Caution,
            summary: "Create links between files",
            description: "Creates links to files or directories - think of them as shortcuts or aliases. The -s flag creates symbolic (soft) links, which are like shortcuts that point to another file's path. These are what you usually want. Without -s, it creates hard links which are more complex (multiple names for the same data). Common uses include creating shorter paths to frequently accessed directories, maintaining multiple versions of programs, or making files appear in multiple locations without copying them. For example, 'ln -s /long/path/to/file shortcut' creates a convenient shortcut.",
            common_flags: vec![
                FlagInfo { flag: "-s", description: "Symbolic: create symbolic link instead of hard link", example: Some("ln -s target link") },
            ],
            examples: vec![
                Example { command: "ln -s /path/to/file link", description: "Create symbolic link", use_case: "Create shortcut to file" },
                Example { command: "ln -s /usr/bin/python3 python", description: "Link python to python3", use_case: "Alias command name" },
            ],
            related_commands: vec!["cp", "mv"],
        });

        self.register(CommandInfo {
            name: "file".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Determine file type",
            description: "Identifies what type of file something really is by analyzing its actual contents, not just trusting the file extension. It can tell you if something is a text file, image, executable, archive, or dozens of other types. Useful when you encounter files with no extension, wrong extensions, or you want to verify what you're dealing with. For example, 'file mystery' might tell you it's a JPEG image even if it has no .jpg extension. Smarter than just looking at filenames!",
            common_flags: vec![],
            examples: vec![
                Example { command: "file document.pdf", description: "Check file type", use_case: "Verify file format" },
                Example { command: "file *", description: "Check all files in directory", use_case: "Identify unknown files" },
            ],
            related_commands: vec!["ls", "stat"],
        });

        self.register(CommandInfo {
            name: "which".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Locate a command",
            description: "Shows you exactly where a command is located on your system by searching through your PATH directories. When you type a command like 'python', the shell looks through PATH to find it - 'which' shows you which one it finds. Useful for figuring out which version of a program you're using, especially when multiple versions are installed. For example, 'which python' might show '/usr/bin/python' or '/usr/local/bin/python'. If 'which' finds nothing, the command isn't in your PATH.",
            common_flags: vec![],
            examples: vec![
                Example { command: "which python", description: "Find python location", use_case: "Locate command executable" },
                Example { command: "which -a python", description: "Show all matches in PATH", use_case: "Find all versions of command" },
            ],
            related_commands: vec!["whereis", "type", "command"],
        });

        self.register(CommandInfo {
            name: "date".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Print or set system date and time",
            description: "Shows the current date and time, or formats it in custom ways for scripts and logs. Without any options, it displays the full date and time. But you can format it however you want using format codes like '+%Y-%m-%d' for year-month-day. Super useful in scripts for creating timestamped filenames, log entries, or calculating how long something took. For example, 'date +%Y%m%d' gives you '20231215' which is perfect for backup filenames. Can also set the system time, but that requires sudo.",
            common_flags: vec![],
            examples: vec![
                Example { command: "date", description: "Show current date/time", use_case: "Get current timestamp" },
                Example { command: "date '+%Y-%m-%d'", description: "Custom date format", use_case: "Format date for filenames" },
            ],
            related_commands: vec!["cal", "timedatectl"],
        });

        self.register(CommandInfo {
            name: "alias".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Create command shortcuts",
            description: "Creates custom shortcuts for commands you use frequently, saving you tons of typing. Like creating your own abbreviated commands. For example, 'alias ll=\"ls -la\"' lets you type just 'll' instead of 'ls -la' every time. These shortcuts only last for your current terminal session unless you add them to your .bashrc or .zshrc file to make them permanent. Run 'alias' with no arguments to see all your current aliases. Power users create dozens of these to speed up their workflow dramatically!",
            common_flags: vec![],
            examples: vec![
                Example { command: "alias ll='ls -la'", description: "Create 'll' shortcut", use_case: "Quick long listing" },
                Example { command: "alias", description: "List all aliases", use_case: "See existing shortcuts" },
            ],
            related_commands: vec!["unalias", "function"],
        });

        self.register(CommandInfo {
            name: "export".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Set environment variables",
            description: "Sets environment variables that control how programs behave and where they look for things. Environment variables are like global settings for your terminal session. The most common one is PATH, which tells your shell where to find commands. For example, 'export EDITOR=vim' sets vim as your default editor, and 'export PATH=$PATH:/new/folder' adds a new directory to your command search path. These last only for your current session unless you add them to your .bashrc or .zshrc file.",
            common_flags: vec![],
            examples: vec![
                Example { command: "export PATH=$PATH:/new/path", description: "Add to PATH", use_case: "Make commands available" },
                Example { command: "export EDITOR=vim", description: "Set default editor", use_case: "Configure environment" },
            ],
            related_commands: vec!["env", "printenv"],
        });

        self.register(CommandInfo {
            name: "env".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Display environment variables",
            description: "Displays all your environment variables (the settings that control your terminal and programs) or runs a command with modified variables. Run 'env' alone to see everything - you'll see PATH, HOME, USER, and dozens of others. You can also use it to temporarily change variables for a single command like 'env DEBUG=1 myprogram' without affecting your overall environment. Useful for debugging to see what environment a program is seeing, or checking what variables are set.",
            common_flags: vec![],
            examples: vec![
                Example { command: "env", description: "List all environment variables", use_case: "Check environment settings" },
                Example { command: "env VAR=value command", description: "Run with custom environment", use_case: "Temporary environment changes" },
            ],
            related_commands: vec!["export", "printenv"],
        });

        // Package managers
        self.register(CommandInfo {
            name: "apt".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Dangerous,
            summary: "Advanced Package Tool (Debian/Ubuntu)",
            description: "The package manager for Debian-based Linux systems like Ubuntu - like an app store for the command line. Use it to install, update, and remove software. 'sudo apt update' refreshes the list of available packages, 'sudo apt install program' installs software, and 'sudo apt upgrade' updates everything you have installed. Much easier than downloading and installing programs manually. Requires sudo because it modifies system software. Essential for keeping your system updated and installing new tools.",
            common_flags: vec![],
            examples: vec![
                Example { command: "sudo apt update", description: "Update package list", use_case: "Refresh available packages" },
                Example { command: "sudo apt install package", description: "Install package", use_case: "Add new software" },
                Example { command: "sudo apt upgrade", description: "Upgrade all packages", use_case: "Update installed software" },
            ],
            related_commands: vec!["apt-get", "dpkg", "snap"],
        });

        self.register(CommandInfo {
            name: "yum".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Dangerous,
            summary: "Yellowdog Updater Modified (RedHat/CentOS)",
            description: "The package manager for RedHat-based Linux systems like CentOS and older Fedora versions. Does the same job as apt but for a different family of Linux distributions. Use 'sudo yum update' to update all packages, 'sudo yum install program' to install software, and 'sudo yum remove program' to uninstall. Being gradually replaced by dnf on newer systems, but still widely used on enterprise servers. Like apt, it requires root privileges to modify system packages.",
            common_flags: vec![],
            examples: vec![
                Example { command: "sudo yum update", description: "Update all packages", use_case: "System maintenance" },
                Example { command: "sudo yum install package", description: "Install package", use_case: "Add new software" },
            ],
            related_commands: vec!["dnf", "rpm"],
        });

        self.register(CommandInfo {
            name: "dnf".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Dangerous,
            summary: "Dandified YUM (Modern RedHat/Fedora)",
            description: "The modern replacement for yum on Fedora and newer RedHat systems, offering better performance and dependency resolution. It's basically yum 2.0 - faster, more reliable, and with cleaner output. The commands are almost identical: 'sudo dnf install', 'sudo dnf update', 'sudo dnf remove'. If you're on a modern RedHat/Fedora system, use dnf instead of yum. Most yum commands work with dnf too, making the transition easy. Like its predecessor, requires sudo for most operations.",
            common_flags: vec![],
            examples: vec![
                Example { command: "sudo dnf install package", description: "Install package", use_case: "Add software" },
                Example { command: "sudo dnf update", description: "Update packages", use_case: "System updates" },
            ],
            related_commands: vec!["yum", "rpm"],
        });

        self.register(CommandInfo {
            name: "pacman".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Dangerous,
            summary: "Package manager (Arch Linux)",
            description: "The package manager for Arch Linux and Arch-based distributions, known for being fast and powerful but with a unique syntax. Unlike apt or yum, it uses flags: -S to install (sync), -R to remove, -Syu to update everything. 'sudo pacman -Syu' is the Arch equivalent of 'apt update && apt upgrade'. The Arch package repository is huge and very up-to-date. Power and flexibility come with complexity, so read the prompts carefully before confirming - pacman is very literal about what you ask it to do!",
            common_flags: vec![
                FlagInfo { flag: "-S", description: "Sync: install packages", example: Some("sudo pacman -S package") },
                FlagInfo { flag: "-Syu", description: "Full system upgrade", example: Some("sudo pacman -Syu") },
                FlagInfo { flag: "-R", description: "Remove: uninstall packages", example: Some("sudo pacman -R package") },
            ],
            examples: vec![
                Example { command: "sudo pacman -Syu", description: "System upgrade", use_case: "Update entire system" },
                Example { command: "sudo pacman -S package", description: "Install package", use_case: "Add new software" },
            ],
            related_commands: vec!["yay", "paru"],
        });

        // More networking
        self.register(CommandInfo {
            name: "netstat".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Safe,
            summary: "Network statistics",
            description: "Displays active network connections, listening ports, and routing information - like a network traffic monitor for your system. The most useful command is 'netstat -tuln' which shows all TCP/UDP ports that programs are listening on. Great for checking if a web server is listening on port 80, seeing what's connected to your machine, or diagnosing network issues. Being gradually replaced by the newer 'ss' command, but still widely used and available. Essential for network troubleshooting and security audits.",
            common_flags: vec![
                FlagInfo { flag: "-tuln", description: "Show TCP/UDP listening ports with numbers", example: Some("netstat -tuln") },
                FlagInfo { flag: "-r", description: "Display routing table", example: None },
            ],
            examples: vec![
                Example { command: "netstat -tuln", description: "List listening ports", use_case: "See what services are running" },
                Example { command: "netstat -r", description: "Show routing table", use_case: "Check network routes" },
            ],
            related_commands: vec!["ss", "lsof", "ip"],
        });

        self.register(CommandInfo {
            name: "ifconfig".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Caution,
            summary: "Configure network interfaces",
            description: "Shows information about your network interfaces (like WiFi and Ethernet adapters) including IP addresses, network masks, and connection status. The classic way to check 'what's my IP address' on Linux. Run 'ifconfig' to see all interfaces with their IPs. While still widely used, it's being replaced by the more powerful 'ip' command on modern systems. You can also use it to configure networks, but that requires sudo and is rarely done manually anymore. Quick way to check your network configuration.",
            common_flags: vec![],
            examples: vec![
                Example { command: "ifconfig", description: "Show all interfaces", use_case: "Check network configuration" },
                Example { command: "ifconfig eth0", description: "Show specific interface", use_case: "Get interface details" },
            ],
            related_commands: vec!["ip", "iwconfig"],
        });

        self.register(CommandInfo {
            name: "ip".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Caution,
            summary: "Show/manipulate routing and network devices",
            description: "The modern, powerful replacement for ifconfig with more features and better consistency. Use 'ip addr show' to see IP addresses (like ifconfig), 'ip route show' for routing information, and 'ip link show' for network interfaces. The syntax is more structured and scriptable than ifconfig. Becoming the standard on all Linux systems - if you're learning networking commands, learn 'ip' instead of ifconfig. Can do everything ifconfig does and much more, from managing VLANs to setting up tunnels.",
            common_flags: vec![],
            examples: vec![
                Example { command: "ip addr show", description: "Show IP addresses", use_case: "View network configuration" },
                Example { command: "ip route show", description: "Display routing table", use_case: "Check network routes" },
                Example { command: "ip link show", description: "Show network interfaces", use_case: "List network devices" },
            ],
            related_commands: vec!["ifconfig", "route"],
        });

        self.register(CommandInfo {
            name: "nc".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Caution,
            summary: "Netcat - networking swiss army knife",
            description: "A versatile networking tool that can read and write data across network connections, often called the 'Swiss Army knife' of networking. You can use it to test if a port is open, create simple chat servers, transfer files, debug network services, or scan ports. For example, 'nc -l 8080' listens on port 8080, while 'nc example.com 80' connects to port 80. Great for testing network connectivity, debugging services, or quick file transfers. Simple but incredibly powerful for network troubleshooting.",
            common_flags: vec![
                FlagInfo { flag: "-l", description: "Listen mode: listen for incoming connections", example: Some("nc -l 8080") },
                FlagInfo { flag: "-v", description: "Verbose: more detailed output", example: None },
            ],
            examples: vec![
                Example { command: "nc -l 8080", description: "Listen on port 8080", use_case: "Test network connectivity" },
                Example { command: "nc example.com 80", description: "Connect to port 80", use_case: "Test HTTP connection" },
            ],
            related_commands: vec!["telnet", "curl", "socat"],
        });

        self.register(CommandInfo {
            name: "traceroute".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Safe,
            summary: "Trace route to network host",
            description: "Traces the complete network path from your computer to a destination, showing every router (hop) along the way. Like following breadcrumbs across the internet. Each line shows a router your data passes through, with response times. Incredibly useful for diagnosing where network problems occur - if packets get slow or lost at a specific hop, you know where the problem is. For example, 'traceroute google.com' shows you all the routers between you and Google. Network admins use this constantly for troubleshooting.",
            common_flags: vec![],
            examples: vec![
                Example { command: "traceroute google.com", description: "Trace route to Google", use_case: "Diagnose network problems" },
            ],
            related_commands: vec!["ping", "mtr"],
        });

        self.register(CommandInfo {
            name: "nslookup".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Safe,
            summary: "Query DNS servers",
            description: "Looks up domain names in DNS (Domain Name System) to find their IP addresses, or vice versa. Like looking up a phone number in a directory. Use it to check if a domain resolves correctly, find a website's IP address, or troubleshoot DNS problems. For example, 'nslookup google.com' shows you Google's IP addresses. Essential for diagnosing website connection issues - if nslookup can't find a domain, the problem is likely DNS-related. Being replaced by 'dig' on modern systems but still widely used.",
            common_flags: vec![],
            examples: vec![
                Example { command: "nslookup google.com", description: "Look up IP for domain", use_case: "DNS troubleshooting" },
            ],
            related_commands: vec!["dig", "host"],
        });

        self.register(CommandInfo {
            name: "dig".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Safe,
            summary: "DNS lookup utility",
            description: "A more powerful and detailed DNS lookup tool than nslookup, providing comprehensive information about DNS records. Network professionals prefer it for its detailed output showing query time, DNS server used, and complete record information. Use 'dig google.com' for full details, or 'dig +short google.com' to just get the IP address quickly. Can query specific record types (A, MX, TXT, etc.) and provides timing information useful for performance troubleshooting. The go-to tool for serious DNS investigation.",
            common_flags: vec![
                FlagInfo { flag: "+short", description: "Short output: just the answer", example: Some("dig +short google.com") },
            ],
            examples: vec![
                Example { command: "dig google.com", description: "Query DNS records", use_case: "Detailed DNS information" },
                Example { command: "dig +short google.com", description: "Get just the IP", use_case: "Quick DNS lookup" },
            ],
            related_commands: vec!["nslookup", "host"],
        });

        // Disk utilities
        self.register(CommandInfo {
            name: "mount".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Dangerous,
            summary: "Mount filesystems",
            description: "Careful: Attaches storage devices (like USB drives, hard disks, or network shares) to your filesystem so you can access their files. In Linux, you must 'mount' a drive before using it - it doesn't happen automatically like on Windows. Run 'mount' alone to see what's currently mounted, or 'sudo mount /dev/sdb1 /mnt' to mount a device. Always unmount with 'umount' before removing drives to prevent data corruption! Modern desktop Linux often automounts USB drives, but servers require manual mounting.",
            common_flags: vec![
                FlagInfo { flag: "-t", description: "Type: specify filesystem type", example: Some("mount -t ext4 /dev/sdb1 /mnt") },
            ],
            examples: vec![
                Example { command: "mount", description: "Show mounted filesystems", use_case: "See what's mounted" },
                Example { command: "sudo mount /dev/sdb1 /mnt", description: "Mount device", use_case: "Access external drive" },
            ],
            related_commands: vec!["umount", "df", "lsblk"],
        });

        self.register(CommandInfo {
            name: "umount".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Dangerous,
            summary: "Unmount filesystems",
            description: "Careful: Safely detaches mounted filesystems, ensuring all data is written before disconnecting the device. ALWAYS run this before unplugging USB drives or external drives - yanking them out while mounted can corrupt data! Use 'sudo umount /mnt' where /mnt is the mount point. If it says 'device is busy', something is still using files on that drive - close programs and try again. The safe eject button equivalent for Linux command line. Note the spelling: it's 'umount', not 'unmount'!",
            common_flags: vec![],
            examples: vec![
                Example { command: "sudo umount /mnt", description: "Unmount filesystem", use_case: "Safely remove external drive" },
            ],
            related_commands: vec!["mount", "eject"],
        });

        self.register(CommandInfo {
            name: "lsblk".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "List block devices",
            description: "Displays all storage devices (hard drives, SSDs, USB drives) in a tree structure showing their partitions and mount points. Think of it as a visual map of all your disks and how they're divided up. Use 'lsblk -f' to also see filesystem types and UUIDs, which is super useful when setting up Arch Linux or troubleshooting mount issues. Much easier to read than parsing fdisk output. Perfect for quickly seeing what drives are connected and where they're mounted.",
            common_flags: vec![
                FlagInfo { flag: "-f", description: "Filesystems: show filesystem information", example: Some("lsblk -f") },
            ],
            examples: vec![
                Example { command: "lsblk", description: "List all block devices", use_case: "See connected drives" },
                Example { command: "lsblk -f", description: "Show with filesystem details", use_case: "Check filesystem types" },
            ],
            related_commands: vec!["fdisk", "df", "mount"],
        });

        self.register(CommandInfo {
            name: "fdisk".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Critical,
            summary: "Partition table manipulator",
            description: "CRITICAL: A powerful but dangerous tool for creating, deleting, and modifying disk partitions. Think of it like dividing a hard drive into separate sections - but one wrong move can erase everything! Essential for setting up Arch Linux where you need to partition your disk before installation. Use 'fdisk -l' to safely LIST partitions without changing anything. When actually partitioning (just 'fdisk /dev/sda'), you work in an interactive menu - changes aren't saved until you explicitly write them with 'w'. Beginners should consider using the safer 'cfdisk' which has a friendlier interface.",
            common_flags: vec![
                FlagInfo { flag: "-l", description: "List: show partition tables (safe)", example: Some("sudo fdisk -l") },
            ],
            examples: vec![
                Example { command: "sudo fdisk -l", description: "List partitions (safe)", use_case: "View disk layout" },
            ],
            related_commands: vec!["parted", "gparted", "lsblk"],
        });

        // More compression
        self.register(CommandInfo {
            name: "bzip2".to_string(),
            category: CommandCategory::Archiving,
            danger_level: DangerLevel::Caution,
            summary: "Compress files with bzip2",
            description: "Compresses files more effectively than gzip, typically achieving 10-15% better compression, but takes longer to run. Creates .bz2 files that are great for archiving large files you don't access frequently. Like gzip, it replaces the original file unless you use -k to keep it. Common for distributing source code (you'll see .tar.bz2 archives). Use 'bzip2 -d' to decompress, or the 'bunzip2' command. When you need maximum space savings and don't mind waiting a bit longer, bzip2 is your friend.",
            common_flags: vec![
                FlagInfo { flag: "-k", description: "Keep: keep original file", example: Some("bzip2 -k file.txt") },
                FlagInfo { flag: "-d", description: "Decompress: uncompress files", example: Some("bzip2 -d file.bz2") },
            ],
            examples: vec![
                Example { command: "bzip2 file.txt", description: "Compress file", use_case: "Better compression ratio" },
                Example { command: "bzip2 -d file.bz2", description: "Decompress file", use_case: "Extract bz2 file" },
            ],
            related_commands: vec!["gzip", "xz", "tar"],
        });

        self.register(CommandInfo {
            name: "xz".to_string(),
            category: CommandCategory::Archiving,
            danger_level: DangerLevel::Caution,
            summary: "Compress with xz (best compression)",
            description: "The champion of compression - squeezes files down to the smallest possible size, but takes the longest time to do it. Creates .xz files that can be 30-50% smaller than gzip. Increasingly popular for software distribution (many Linux packages now use .tar.xz format). Great for long-term archival where you want to save maximum disk space and don't mind the slow compression. Decompression is reasonably fast. Use 'xz -d' to decompress or the 'unxz' command. When disk space is at a premium and you have time to spare, xz delivers the best results.",
            common_flags: vec![
                FlagInfo { flag: "-k", description: "Keep: keep original file", example: Some("xz -k file.txt") },
                FlagInfo { flag: "-d", description: "Decompress: uncompress files", example: Some("xz -d file.xz") },
            ],
            examples: vec![
                Example { command: "xz file.txt", description: "Compress file", use_case: "Maximum compression" },
                Example { command: "xz -d file.xz", description: "Decompress file", use_case: "Extract xz file" },
            ],
            related_commands: vec!["gzip", "bzip2", "tar"],
        });

        // Text editors
        self.register(CommandInfo {
            name: "nano".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Simple text editor",
            description: "The most beginner-friendly command-line text editor - no confusing modes or cryptic commands! When you open a file, you can just start typing. All the keyboard shortcuts are helpfully displayed at the bottom of the screen (^ means Ctrl, so ^X means Ctrl+X to exit). Perfect for quick config file edits, writing scripts, or editing text without leaving the terminal. Use Ctrl+O to save (WriteOut), Ctrl+X to exit, and Ctrl+W to search. If you're new to Linux, start with nano before attempting vim or emacs!",
            common_flags: vec![],
            examples: vec![
                Example { command: "nano file.txt", description: "Edit file", use_case: "Simple text editing" },
                Example { command: "nano -w file.txt", description: "Disable line wrapping", use_case: "Edit code files" },
            ],
            related_commands: vec!["vim", "emacs", "vi"],
        });

        self.register(CommandInfo {
            name: "vi".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Visual editor (classic)",
            description: "The classic Unix text editor that's guaranteed to be on every Linux system - even minimal ones. It's modal, meaning it has different modes for different tasks, which is confusing at first but powerful once learned. Starts in 'normal' mode where you can't type - press 'i' to enter 'insert' mode to actually edit text. Press ESC to go back to normal mode, then type ':wq' and press Enter to save and quit (or ':q!' to quit without saving). Learning vi basics is essential because when you SSH into servers, vi is often the only editor available.",
            common_flags: vec![],
            examples: vec![
                Example { command: "vi file.txt", description: "Edit file", use_case: "Universal text editor" },
            ],
            related_commands: vec!["vim", "nano", "emacs"],
        });

        self.register(CommandInfo {
            name: "vim".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Vi IMproved - advanced text editor",
            description: "An enhanced version of vi with syntax highlighting, plugins, multiple undo levels, and tons of features that make it a favorite among developers. The learning curve is steep - expect to spend a week getting comfortable - but once mastered, you can edit text incredibly fast without ever touching your mouse. Has the same modal system as vi (insert mode for typing, normal mode for commands) but with way more capabilities. Power users customize it extensively with plugins and configurations. If you're serious about command-line editing and willing to invest time learning, vim will reward you with blazing speed.",
            common_flags: vec![],
            examples: vec![
                Example { command: "vim file.txt", description: "Edit file", use_case: "Advanced text editing" },
                Example { command: "vim +10 file.txt", description: "Open at line 10", use_case: "Jump to specific line" },
            ],
            related_commands: vec!["vi", "nvim", "nano"],
        });

        // System services
        self.register(CommandInfo {
            name: "systemctl".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Dangerous,
            summary: "Control systemd services",
            description: "The control center for managing system services (daemons) on modern Linux systems using systemd. Like the Services panel in Windows, but more powerful. Use it to start/stop services like web servers or databases, check their status, enable them to start at boot, or view their logs. For example, 'systemctl status nginx' checks if nginx is running, 'sudo systemctl start nginx' launches it, and 'sudo systemctl enable nginx' makes it auto-start on boot. Essential for server administration and managing background services on your Linux system.",
            common_flags: vec![],
            examples: vec![
                Example { command: "systemctl status nginx", description: "Check service status", use_case: "See if service is running" },
                Example { command: "sudo systemctl start nginx", description: "Start service", use_case: "Launch a service" },
                Example { command: "sudo systemctl enable nginx", description: "Enable on boot", use_case: "Auto-start service" },
            ],
            related_commands: vec!["service", "journalctl"],
        });

        self.register(CommandInfo {
            name: "service".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Dangerous,
            summary: "Run init script (legacy)",
            description: "The old-school way of managing services from before systemd became standard on most Linux distributions. Still works on modern systems and is simpler to remember for basic operations - 'sudo service nginx start/stop/restart/status'. It's actually a compatibility wrapper that calls systemctl behind the scenes on systemd systems. Some older tutorials and scripts still use it. While 'systemctl' is more powerful and the modern standard, 'service' is perfectly fine for basic start/stop operations and works on both old and new systems.",
            common_flags: vec![],
            examples: vec![
                Example { command: "sudo service nginx status", description: "Check service", use_case: "Service status check" },
                Example { command: "sudo service nginx restart", description: "Restart service", use_case: "Apply configuration changes" },
            ],
            related_commands: vec!["systemctl", "init"],
        });

        self.register(CommandInfo {
            name: "crontab".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Caution,
            summary: "Schedule periodic jobs",
            description: "Your personal task scheduler for automating commands that need to run at specific times or intervals. Like Windows Task Scheduler but controlled through the command line. Use 'crontab -e' to edit your schedule, where each line specifies when and what to run (format: minute hour day month weekday command). Perfect for automating backups, cleanup scripts, or any repetitive tasks. For example, you could run a backup script every night at 2 AM, or check for updates every Monday. Use 'crontab -l' to view your current scheduled jobs.",
            common_flags: vec![
                FlagInfo { flag: "-e", description: "Edit: edit crontab file", example: Some("crontab -e") },
                FlagInfo { flag: "-l", description: "List: display crontab", example: Some("crontab -l") },
            ],
            examples: vec![
                Example { command: "crontab -e", description: "Edit scheduled jobs", use_case: "Add automated tasks" },
                Example { command: "crontab -l", description: "List cron jobs", use_case: "See scheduled tasks" },
            ],
            related_commands: vec!["at", "systemd-timer"],
        });

        // File utilities
        self.register(CommandInfo {
            name: "diff".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Compare files line by line",
            description: "Compares two files and shows exactly what's different between them, line by line. Like Track Changes in Word but for any text files. Incredibly useful for seeing what changed between versions of a config file, comparing your edited code to the original, or reviewing changes before committing to git. The output shows lines that were added (marked with +), removed (marked with -), or changed. Use the -u flag for 'unified diff' format which is easier to read and what git uses. Essential tool for developers and system administrators.",
            common_flags: vec![
                FlagInfo { flag: "-u", description: "Unified: unified diff format (most common)", example: Some("diff -u file1 file2") },
                FlagInfo { flag: "-r", description: "Recursive: compare directories", example: Some("diff -r dir1/ dir2/") },
            ],
            examples: vec![
                Example { command: "diff file1.txt file2.txt", description: "Compare two files", use_case: "See what changed" },
                Example { command: "diff -u old.txt new.txt", description: "Unified diff format", use_case: "Git-style comparison" },
            ],
            related_commands: vec!["patch", "comm", "cmp"],
        });

        self.register(CommandInfo {
            name: "patch".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Caution,
            summary: "Apply diff file to original",
            description: "Takes a patch file (created by diff) and applies those changes to the original files, automatically making all the edits for you. Like having someone tell you exactly which lines to change, add, or remove, and doing it automatically. Commonly used to apply bug fixes or updates to source code without downloading entire new files. For example, open source projects often distribute patches for security fixes. Use it like 'patch < changes.patch' to update your files. Be cautious - it modifies files directly, so keep backups when patching important code!",
            common_flags: vec![],
            examples: vec![
                Example { command: "patch < changes.patch", description: "Apply patch", use_case: "Update files from diff" },
            ],
            related_commands: vec!["diff", "git apply"],
        });

        self.register(CommandInfo {
            name: "tree".to_string(),
            category: CommandCategory::Navigation,
            danger_level: DangerLevel::Safe,
            summary: "List directory contents in tree format",
            description: "Displays your directory structure as a beautiful visual tree with branches showing the hierarchy, making it easy to see how folders and files are organized at a glance. Like Windows Explorer's folder tree view, but prettier! Perfect for understanding a project's structure, documenting folder layouts, or getting a quick overview of what's in a directory and its subdirectories. Use -L to limit depth (like 'tree -L 2' for just 2 levels deep) to avoid overwhelming output in large projects. Great for README files and documentation.",
            common_flags: vec![
                FlagInfo { flag: "-L", description: "Level: limit depth of recursion", example: Some("tree -L 2") },
                FlagInfo { flag: "-d", description: "Directories: show only directories", example: Some("tree -d") },
            ],
            examples: vec![
                Example { command: "tree", description: "Show directory tree", use_case: "Visualize folder structure" },
                Example { command: "tree -L 2", description: "Show 2 levels deep", use_case: "Quick project overview" },
            ],
            related_commands: vec!["ls", "find"],
        });

        self.register(CommandInfo {
            name: "xargs".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Caution,
            summary: "Build and execute commands from standard input",
            description: "Converts input from one command into arguments for another command, enabling powerful pipelines and batch operations. Think of it as a bridge that lets you use the output of one command as the input to another in creative ways. For example, 'find . -name \"*.log\" | xargs rm' finds all .log files and deletes them. The -I flag lets you control exactly where the input goes in the command. Essential for processing lists of files or performing the same operation on multiple items. Great for automation but use carefully - always test with 'echo' first!",
            common_flags: vec![
                FlagInfo { flag: "-I", description: "Replace string: specify placeholder", example: Some("find . -name '*.txt' | xargs -I {} cp {} /backup/") },
            ],
            examples: vec![
                Example { command: "find . -name '*.log' | xargs rm", description: "Delete all .log files", use_case: "Batch file operations" },
                Example { command: "ls *.txt | xargs -I {} cp {} /backup/", description: "Copy files with placeholder", use_case: "Process each file" },
            ],
            related_commands: vec!["find", "parallel"],
        });

        self.register(CommandInfo {
            name: "tee".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Read from stdin and write to stdout and files",
            description: "Splits command output into two streams - showing it on your screen AND saving it to a file at the same time. Like a T-junction in a pipe (hence the name 'tee'). Super useful when you want to watch a command's output live while also keeping a permanent log. For example, 'make | tee build.log' lets you watch your code compile while saving all the output to a file for later review. The -a flag appends instead of overwriting. Perfect for logging long-running processes or build outputs.",
            common_flags: vec![
                FlagInfo { flag: "-a", description: "Append: append to file instead of overwriting", example: Some("command | tee -a log.txt") },
            ],
            examples: vec![
                Example { command: "ls -la | tee output.txt", description: "Save and display output", use_case: "Log command output" },
                Example { command: "make | tee build.log", description: "Log build output", use_case: "Save compilation output" },
            ],
            related_commands: vec!["cat", "redirect"],
        });

        self.register(CommandInfo {
            name: "watch".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Execute a program periodically",
            description: "Runs a command over and over again at regular intervals (default every 2 seconds) and displays the updated output, clearing the screen each time. Like hitting refresh repeatedly on a webpage, but automated. Perfect for monitoring things that change over time - disk space usage, active processes, file modifications, or network connections. For example, 'watch df -h' shows you disk space updating in real-time. Use -n to change the interval, like 'watch -n 5 command' to run every 5 seconds. Press Ctrl+C to stop watching.",
            common_flags: vec![
                FlagInfo { flag: "-n", description: "Interval: seconds between updates", example: Some("watch -n 2 df -h") },
            ],
            examples: vec![
                Example { command: "watch df -h", description: "Monitor disk space", use_case: "Watch space usage in real-time" },
                Example { command: "watch -n 5 'ls -l'", description: "Watch directory every 5 seconds", use_case: "Monitor file changes" },
            ],
            related_commands: vec!["top", "tail -f"],
        });

        self.register(CommandInfo {
            name: "time".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Time command execution",
            description: "Measures and reports how long a command takes to run, giving you three numbers: real time (total clock time), user time (CPU time in your code), and system time (CPU time in kernel/system calls). Like a stopwatch for your commands. Great for benchmarking scripts, finding performance bottlenecks, or just satisfying curiosity about how long things take. For example, 'time ls -R' shows how long it takes to recursively list all files. The output appears after the command finishes. Essential for optimization work.",
            common_flags: vec![],
            examples: vec![
                Example { command: "time ls -R", description: "Time a command", use_case: "Measure performance" },
                Example { command: "time ./script.sh", description: "Time script execution", use_case: "Benchmark script" },
            ],
            related_commands: vec!["timeout", "date"],
        });

        self.register(CommandInfo {
            name: "sleep".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Delay for specified time",
            description: "Pauses for a specified amount of time before continuing - literally makes your terminal sleep! Takes a number of seconds by default, but you can use 'm' for minutes, 'h' for hours, or 'd' for days. Essential in scripts when you need to wait between operations, like pausing before retrying a failed network connection or spacing out API requests. For example, 'sleep 5' waits 5 seconds, 'sleep 2m' waits 2 minutes. Can also chain commands like 'echo Starting... && sleep 3 && echo Done!' for timed sequences.",
            common_flags: vec![],
            examples: vec![
                Example { command: "sleep 5", description: "Sleep for 5 seconds", use_case: "Add delay in script" },
                Example { command: "sleep 1m", description: "Sleep for 1 minute", use_case: "Longer delays" },
            ],
            related_commands: vec!["wait", "timeout"],
        });

        // User management
        self.register(CommandInfo {
            name: "useradd".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Dangerous,
            summary: "Create a new user",
            description: "Careful: Creates a new user account on the system, assigning them a username, user ID, and optionally a home directory and default shell. Requires sudo/root access since you're modifying system accounts. Use -m to create their home directory automatically (highly recommended). For example, 'sudo useradd -m john' creates user john with a home directory at /home/john. After creating the user, you'll want to set their password with 'sudo passwd john'. Essential for multi-user systems and servers. Note: Some systems have 'adduser' which is more interactive and beginner-friendly.",
            common_flags: vec![
                FlagInfo { flag: "-m", description: "Create home directory", example: Some("sudo useradd -m username") },
            ],
            examples: vec![
                Example { command: "sudo useradd -m john", description: "Create user with home dir", use_case: "Add new system user" },
            ],
            related_commands: vec!["userdel", "usermod", "adduser"],
        });

        self.register(CommandInfo {
            name: "userdel".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Dangerous,
            summary: "Delete a user account",
            description: "Careful: Permanently removes a user account from the system. Requires root privileges. By default it only removes the account entry, leaving their home directory and files intact. Use the -r flag to also delete their home directory and mail spool - but be careful, this deletes all their files! For example, 'sudo userdel john' removes the account but keeps /home/john, while 'sudo userdel -r john' removes everything. Always backup important data before deleting users. Used for cleaning up old accounts or removing compromised accounts on servers.",
            common_flags: vec![
                FlagInfo { flag: "-r", description: "Remove: delete home directory and mail spool", example: Some("sudo userdel -r username") },
            ],
            examples: vec![
                Example { command: "sudo userdel john", description: "Delete user", use_case: "Remove user account" },
                Example { command: "sudo userdel -r john", description: "Delete user and files", use_case: "Complete user removal" },
            ],
            related_commands: vec!["useradd", "usermod"],
        });

        self.register(CommandInfo {
            name: "passwd".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Caution,
            summary: "Change user password",
            description: "Changes a user's password securely. When run without arguments, it changes YOUR password - the system will ask for your current password, then the new one twice for confirmation. With sudo, you can reset other users' passwords like 'sudo passwd john' without needing their old password (useful when users forget their passwords). The password you type won't be visible on screen for security. Important for maintaining account security and resetting forgotten passwords. The system enforces password complexity rules on most distributions.",
            common_flags: vec![],
            examples: vec![
                Example { command: "passwd", description: "Change your password", use_case: "Update your password" },
                Example { command: "sudo passwd john", description: "Change another user's password", use_case: "Reset user password" },
            ],
            related_commands: vec!["chpasswd", "usermod"],
        });

        self.register(CommandInfo {
            name: "groups".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Print group memberships",
            description: "Lists all the groups that a user belongs to, which determines what files and resources they can access. In Linux, permissions are managed through user and group ownership. Being in the 'sudo' group gives admin privileges, 'docker' group allows Docker access, etc. Run 'groups' alone to see YOUR groups, or 'groups username' to see someone else's. If you can't access something even with the right file permissions, check if you're in the required group. After being added to a new group, you may need to log out and back in for it to take effect.",
            common_flags: vec![],
            examples: vec![
                Example { command: "groups", description: "Show your groups", use_case: "Check group membership" },
                Example { command: "groups john", description: "Show user's groups", use_case: "View another user's groups" },
            ],
            related_commands: vec!["id", "usermod"],
        });

        // Build tools
        self.register(CommandInfo {
            name: "make".to_string(),
            category: CommandCategory::Build,
            danger_level: DangerLevel::Safe,
            summary: "Build automation tool",
            description: "The classic build automation tool that reads a Makefile containing instructions on how to compile your project and runs them in the correct order. It's smart - only recompiles files that changed, saving time on large projects. Standard for C/C++ projects but used for many other types of builds too. When you download source code and see a Makefile, typically you run './configure && make && sudo make install' to build and install it. The -j flag enables parallel compilation using multiple CPU cores, dramatically speeding up builds on modern machines. Essential for building software from source.",
            common_flags: vec![
                FlagInfo { flag: "-j", description: "Jobs: number of parallel jobs", example: Some("make -j4") },
            ],
            examples: vec![
                Example { command: "make", description: "Build project", use_case: "Compile from source" },
                Example { command: "make install", description: "Install built software", use_case: "System-wide installation" },
            ],
            related_commands: vec!["cmake", "gcc", "configure"],
        });

        self.register(CommandInfo {
            name: "npm".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Caution,
            summary: "Node Package Manager",
            description: "The official package manager for Node.js and JavaScript, managing libraries and tools for your web development projects. Run 'npm install' in a project folder to download all dependencies listed in package.json. Use 'npm install package-name' to add a new package, 'npm start' to run your application, and 'npm run build' for build scripts. It's like apt for JavaScript - handles versioning, dependencies, and scripts automatically. Essential for any modern JavaScript or Node.js development. The npm registry has over a million packages available!",
            common_flags: vec![],
            examples: vec![
                Example { command: "npm install", description: "Install dependencies", use_case: "Set up Node.js project" },
                Example { command: "npm start", description: "Run start script", use_case: "Launch application" },
                Example { command: "npm install package", description: "Install package", use_case: "Add dependency" },
            ],
            related_commands: vec!["yarn", "pnpm", "npx"],
        });

        self.register(CommandInfo {
            name: "pip".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Caution,
            summary: "Python Package Installer",
            description: "The standard package manager for Python, downloading and installing libraries from PyPI (Python Package Index) - a repository with hundreds of thousands of Python packages. Use 'pip install requests' to add a library, 'pip list' to see what's installed, and 'pip install -r requirements.txt' to install all dependencies for a project at once. The 'pip' command usually works with Python 2, while 'pip3' is for Python 3 (though on modern systems, pip often points to pip3). Essential for any Python development - it's how you add functionality beyond Python's standard library.",
            common_flags: vec![],
            examples: vec![
                Example { command: "pip install requests", description: "Install package", use_case: "Add Python library" },
                Example { command: "pip list", description: "List installed packages", use_case: "See what's installed" },
                Example { command: "pip install -r requirements.txt", description: "Install from file", use_case: "Set up project dependencies" },
            ],
            related_commands: vec!["pip3", "conda", "poetry"],
        });

        self.register(CommandInfo {
            name: "cargo".to_string(),
            category: CommandCategory::Build,
            danger_level: DangerLevel::Safe,
            summary: "Rust package manager and build tool",
            description: "The all-in-one build tool and package manager for Rust, handling compilation, dependencies, testing, and more. It's what makes Rust development so smooth! Run 'cargo build' to compile your project, 'cargo run' to build and execute it, 'cargo test' to run tests, and 'cargo new project-name' to start a new project with all the boilerplate. Downloads dependencies from crates.io automatically. This very tool - Arc Academy Terminal - is built with cargo! If you're learning Rust, you'll use cargo constantly. It's beloved by developers for being fast, reliable, and having great error messages.",
            common_flags: vec![],
            examples: vec![
                Example { command: "cargo build", description: "Build project", use_case: "Compile Rust code" },
                Example { command: "cargo run", description: "Build and run", use_case: "Test application" },
                Example { command: "cargo test", description: "Run tests", use_case: "Execute test suite" },
            ],
            related_commands: vec!["rustc", "rustup"],
        });

        // More system monitoring
        self.register(CommandInfo {
            name: "htop".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Interactive process viewer",
            description: "A beautiful, modernized version of 'top' with colors, mouse support, and a much friendlier interface. Shows CPU usage for each core, memory usage, and all running processes in a sortable, filterable list. You can click on column headers to sort, use arrow keys to navigate, press F9 to kill processes, and F6 to sort by different criteria. Way easier than memorizing 'top' keyboard shortcuts! If it's not installed by default, it's worth installing - most Linux users prefer it over top. Perfect for quickly checking what's using your CPU or memory.",
            common_flags: vec![],
            examples: vec![
                Example { command: "htop", description: "Launch interactive monitor", use_case: "Visual system monitoring" },
            ],
            related_commands: vec!["top", "ps", "atop"],
        });

        self.register(CommandInfo {
            name: "free".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Display memory usage",
            description: "Displays how much RAM is free, used, and available on your system in a simple table format. Shows both physical memory (RAM) and swap space (disk-based virtual memory). Use the -h flag for human-readable output in GB/MB instead of confusing kilobytes. Important: Linux uses 'available' memory for caching to speed things up, so 'used' looks higher than it really is - check the 'available' column for how much is truly free for applications. Quick way to see if you're running low on memory or if a program is eating all your RAM.",
            common_flags: vec![
                FlagInfo { flag: "-h", description: "Human-readable: show in GB/MB/KB", example: Some("free -h") },
            ],
            examples: vec![
                Example { command: "free -h", description: "Show memory usage", use_case: "Check available RAM" },
            ],
            related_commands: vec!["top", "vmstat"],
        });

        self.register(CommandInfo {
            name: "uptime".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Show how long system has been running",
            description: "Shows how long your system has been running since the last reboot, along with load averages. Great for bragging about server stability (\"my server has been up for 500 days!\") or checking if a recent reboot occurred. Also displays the current time, how many users are logged in, and the load average (how busy the CPU has been over the last 1, 5, and 15 minutes). On servers, high uptime is often a point of pride, though regular reboots for security updates are actually better practice. Simple but informative.",
            common_flags: vec![],
            examples: vec![
                Example { command: "uptime", description: "Show system uptime", use_case: "Check how long server has been up" },
            ],
            related_commands: vec!["w", "who"],
        });

        self.register(CommandInfo {
            name: "uname".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Print system information",
            description: "Displays basic information about your system including the kernel name (Linux), kernel version, hardware architecture (x86_64, ARM, etc.), and more. Use 'uname -a' to see everything at once - useful when reporting bugs or checking system specs. The -r flag shows just the kernel version, helpful for verifying you're running the latest kernel after updates. Often the first command you run when troubleshooting to confirm what system you're dealing with, especially when SSHing into unfamiliar servers.",
            common_flags: vec![
                FlagInfo { flag: "-a", description: "All: print all information", example: Some("uname -a") },
                FlagInfo { flag: "-r", description: "Release: kernel release version", example: Some("uname -r") },
            ],
            examples: vec![
                Example { command: "uname -a", description: "Show all system info", use_case: "Get kernel and OS details" },
                Example { command: "uname -r", description: "Show kernel version", use_case: "Check kernel release" },
            ],
            related_commands: vec!["lsb_release", "hostnamectl"],
        });

        self.register(CommandInfo {
            name: "hostname".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Show or set system hostname",
            description: "Shows your computer's hostname - the name it uses to identify itself on the network. Like a computer's name tag. Useful when you're managing multiple servers and need to quickly confirm which one you're connected to. Use 'hostname -I' (capital i) to also see your IP addresses. The hostname is often displayed in your terminal prompt, but this command gives you just the name. You can set a new hostname with sudo, though most modern systems use 'hostnamectl' for that now.",
            common_flags: vec![
                FlagInfo { flag: "-I", description: "IP addresses: show all IP addresses", example: Some("hostname -I") },
            ],
            examples: vec![
                Example { command: "hostname", description: "Show hostname", use_case: "Get computer name" },
                Example { command: "hostname -I", description: "Show IP addresses", use_case: "Get network IPs" },
            ],
            related_commands: vec!["hostnamectl", "uname"],
        });

        // System control
        self.register(CommandInfo {
            name: "reboot".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Critical,
            summary: "Reboot the system",
            description: "CRITICAL: Immediately restarts your entire computer. Everything stops - all programs close, all users are kicked off, and the system boots back up from scratch. Use this after kernel updates or when the system is misbehaving and needs a fresh start. On multi-user servers, ALWAYS warn other users first! The command requires sudo privileges. After reboot, you'll need to log back in and restart any programs you were running. On servers, this is a big deal - plan reboots carefully and ideally during maintenance windows.",
            common_flags: vec![],
            examples: vec![
                Example { command: "sudo reboot", description: "Restart system", use_case: "Reboot server/computer" },
            ],
            related_commands: vec!["shutdown", "systemctl reboot"],
        });

        self.register(CommandInfo {
            name: "shutdown".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Critical,
            summary: "Shutdown or restart the system",
            description: "CRITICAL: The controlled way to power off or restart your system, optionally scheduling it for later. Unlike just hitting the power button, this gracefully closes all programs and saves data. Use 'sudo shutdown -h now' to power off immediately, or 'sudo shutdown -r now' to reboot. You can schedule it with 'sudo shutdown -h +30' to shutdown in 30 minutes, giving users warning time. The system will broadcast messages to logged-in users about the impending shutdown. Use 'sudo shutdown -c' to cancel a scheduled shutdown. More polite and safer than 'reboot' or 'poweroff' for managed systems.",
            common_flags: vec![
                FlagInfo { flag: "-h", description: "Halt: shutdown and power off", example: Some("sudo shutdown -h now") },
                FlagInfo { flag: "-r", description: "Reboot: restart the system", example: Some("sudo shutdown -r now") },
            ],
            examples: vec![
                Example { command: "sudo shutdown -h now", description: "Shutdown immediately", use_case: "Power off system" },
                Example { command: "sudo shutdown -r +10", description: "Reboot in 10 minutes", use_case: "Scheduled restart" },
            ],
            related_commands: vec!["reboot", "halt", "poweroff"],
        });

        self.register(CommandInfo {
            name: "dmesg".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Print kernel ring buffer messages",
            description: "Displays messages from the Linux kernel's ring buffer, including boot messages, hardware detection events, driver loading, and importantly, hardware errors. Like peeking into the kernel's diary to see what it's been up to. Super useful for troubleshooting hardware issues - if your USB device isn't working, dmesg will show if the kernel even detected it. Use 'dmesg | tail' to see recent messages, or 'dmesg | grep -i error' to find problems. The -H flag makes it more readable with timestamps. Essential diagnostic tool when hardware isn't behaving.",
            common_flags: vec![
                FlagInfo { flag: "-H", description: "Human-readable: easier to read format", example: Some("dmesg -H") },
                FlagInfo { flag: "-w", description: "Wait: follow new messages in real-time", example: Some("dmesg -w") },
            ],
            examples: vec![
                Example { command: "dmesg | tail", description: "Show recent kernel messages", use_case: "Check for hardware errors" },
                Example { command: "dmesg | grep -i error", description: "Find kernel errors", use_case: "Diagnose system issues" },
            ],
            related_commands: vec!["journalctl", "syslog"],
        });

        self.register(CommandInfo {
            name: "journalctl".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Query systemd journal logs",
            description: "The modern, powerful log viewer for systemd-based Linux systems, replacing old scattered log files with a unified, queryable database of system events. Use it to debug services, check what happened during boot, or investigate system problems. 'journalctl -f' follows logs in real-time like 'tail -f', while 'journalctl -u nginx' shows logs only for the nginx service. You can filter by time, priority, service, or search for specific text. Logs are stored centrally and include timestamps, priorities, and metadata. The go-to tool for system troubleshooting on modern Linux.",
            common_flags: vec![
                FlagInfo { flag: "-f", description: "Follow: stream new log entries", example: Some("journalctl -f") },
                FlagInfo { flag: "-u", description: "Unit: show logs for specific service", example: Some("journalctl -u nginx") },
            ],
            examples: vec![
                Example { command: "journalctl -f", description: "Follow system logs", use_case: "Monitor live log output" },
                Example { command: "journalctl -u nginx", description: "Show nginx logs", use_case: "Debug service issues" },
            ],
            related_commands: vec!["dmesg", "systemctl", "tail"],
        });

        // More package managers
        self.register(CommandInfo {
            name: "brew".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Caution,
            summary: "Homebrew package manager (macOS/Linux)",
            description: "The beloved package manager that started on macOS and is now available on Linux too. Known for being user-friendly with great documentation and a huge selection of packages. Use 'brew install package-name' to install software, 'brew update' to refresh package lists, and 'brew upgrade' to update everything installed. It installs packages in its own directory to avoid conflicting with system packages. macOS users swear by it, and it's growing popular on Linux as an alternative to apt/yum. Great for installing development tools and CLI utilities.",
            common_flags: vec![],
            examples: vec![
                Example { command: "brew install package", description: "Install package", use_case: "Add software on macOS" },
                Example { command: "brew update", description: "Update package list", use_case: "Get latest packages" },
                Example { command: "brew upgrade", description: "Upgrade all packages", use_case: "Update installed software" },
            ],
            related_commands: vec!["apt", "yum"],
        });

        self.register(CommandInfo {
            name: "snap".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Caution,
            summary: "Snap package manager (Ubuntu)",
            description: "Canonical's universal package format that works across different Linux distributions with apps running in isolated sandboxes for security. Snap packages are self-contained with all their dependencies, so they're larger but guaranteed to work consistently. Pre-installed on Ubuntu and growing in adoption. Use 'snap install package-name' to add software, 'snap list' to see what's installed. The sandboxing provides extra security but can cause permission issues. Some users love the convenience, others prefer traditional packages - it's somewhat controversial in the Linux community but undeniably useful.",
            common_flags: vec![],
            examples: vec![
                Example { command: "snap install package", description: "Install snap package", use_case: "Add sandboxed app" },
                Example { command: "snap list", description: "List installed snaps", use_case: "See installed packages" },
            ],
            related_commands: vec!["apt", "flatpak"],
        });

        // Container tools
        self.register(CommandInfo {
            name: "docker".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Caution,
            summary: "Container platform",
            description: "The industry-standard platform for containerization - packaging applications with their dependencies into isolated, portable containers that run consistently anywhere. Think of containers as lightweight virtual machines that start in seconds instead of minutes. Use 'docker ps' to see running containers, 'docker run image-name' to start one, and 'docker build' to create your own. Revolutionary for development (\"it works on my machine\" becomes irrelevant) and deployment. Learning Docker is essential for modern DevOps and cloud development. Containers are everywhere in modern tech infrastructure.",
            common_flags: vec![],
            examples: vec![
                Example { command: "docker ps", description: "List running containers", use_case: "See active containers" },
                Example { command: "docker run image", description: "Run container from image", use_case: "Start containerized app" },
                Example { command: "docker build -t name .", description: "Build container image", use_case: "Create custom image" },
            ],
            related_commands: vec!["docker-compose", "podman", "kubectl"],
        });

        // More text utilities
        self.register(CommandInfo {
            name: "tr".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Translate or delete characters",
            description: "Translates (replaces) or deletes characters from text, working character-by-character rather than on whole words. Simple but incredibly useful for text transformations. For example, 'echo \"hello\" | tr 'a-z' 'A-Z'' converts to uppercase, 'tr -d ' ' removes all spaces, and 'tr '\\n' ' '' replaces newlines with spaces. Great for cleaning up text, changing case, removing unwanted characters, or swapping characters. Works with character ranges and special characters. Simpler than sed for basic character-level operations.",
            common_flags: vec![
                FlagInfo { flag: "-d", description: "Delete: remove specified characters", example: Some("echo 'text' | tr -d 't'") },
            ],
            examples: vec![
                Example { command: "echo 'hello' | tr 'a-z' 'A-Z'", description: "Convert to uppercase", use_case: "Change text case" },
                Example { command: "echo 'hello' | tr -d 'l'", description: "Remove all 'l' characters", use_case: "Delete specific chars" },
            ],
            related_commands: vec!["sed", "awk"],
        });

        self.register(CommandInfo {
            name: "printf".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Format and print data",
            description: "A more powerful version of echo with precise formatting control, similar to printf in C and other programming languages. Lets you format output exactly how you want with format specifiers like %s for strings, %d for numbers, and %f for decimals. Unlike echo, printf doesn't automatically add a newline - you control everything with \\n. For example, 'printf \"Name: %s\\nAge: %d\\n\" John 25' formats output nicely. Essential for scripting when you need predictable, formatted output or when building structured data. More portable across systems than echo's varying implementations.",
            common_flags: vec![],
            examples: vec![
                Example { command: "printf '%s\\n' 'Hello'", description: "Print with newline", use_case: "Formatted output" },
                Example { command: "printf '%d\\n' 42", description: "Print number", use_case: "Format numbers" },
            ],
            related_commands: vec!["echo", "awk"],
        });

        self.register(CommandInfo {
            name: "basename".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Strip directory from filename",
            description: "Strips the directory path from a full file path, leaving just the filename. For example, 'basename /home/user/documents/file.txt' returns just 'file.txt'. You can also remove file extensions: 'basename /path/to/file.txt .txt' gives you just 'file'. Super useful in shell scripts when you need to work with filenames but have full paths - extract the name, process it, then reconstruct the path. Pairs perfectly with dirname which does the opposite (keeps the directory, removes the filename). Simple but essential for path manipulation in scripts.",
            common_flags: vec![],
            examples: vec![
                Example { command: "basename /path/to/file.txt", description: "Get filename", use_case: "Extract filename from path" },
                Example { command: "basename /path/to/file.txt .txt", description: "Remove extension too", use_case: "Get name without extension" },
            ],
            related_commands: vec!["dirname", "realpath"],
        });

        self.register(CommandInfo {
            name: "dirname".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Strip filename from path",
            description: "Does the opposite of basename - removes the filename and keeps the directory path. For example, 'dirname /home/user/documents/file.txt' returns '/home/user/documents'. Essential in scripts when you need to know what directory a file is in, or to build new paths relative to a file's location. For instance, if a script needs to find config files in the same directory it's running from, dirname helps extract that path. Together with basename, you can completely decompose and rebuild file paths programmatically.",
            common_flags: vec![],
            examples: vec![
                Example { command: "dirname /path/to/file.txt", description: "Get directory path", use_case: "Extract directory from path" },
            ],
            related_commands: vec!["basename", "realpath"],
        });

        self.register(CommandInfo {
            name: "readlink".to_string(),
            category: CommandCategory::FileManagement,
            danger_level: DangerLevel::Safe,
            summary: "Print resolved symbolic links",
            description: "Follows a symbolic link (shortcut) and shows you where it actually points to. Symbolic links can be confusing - they look like files but are really just pointers. Use 'readlink link-name' to see the target, or better yet, 'readlink -f link-name' to get the absolute canonical path, resolving all intermediate links and relative paths. For example, '/usr/bin/python' might be a link to 'python3', which itself links to 'python3.9' - readlink -f shows you the final destination. Essential for understanding system configurations and tracking down the real location of commands.",
            common_flags: vec![
                FlagInfo { flag: "-f", description: "Canonicalize: resolve all symlinks to absolute path", example: Some("readlink -f link") },
            ],
            examples: vec![
                Example { command: "readlink /usr/bin/python", description: "See where link points", use_case: "Follow symbolic link" },
                Example { command: "readlink -f link", description: "Get absolute target path", use_case: "Resolve all symlinks" },
            ],
            related_commands: vec!["ln", "realpath"],
        });

        // File search utilities
        self.register(CommandInfo {
            name: "locate".to_string(),
            category: CommandCategory::Search,
            danger_level: DangerLevel::Safe,
            summary: "Find files by name (fast)",
            description: "Blazingly fast file search that uses a pre-built database of all files on your system, making it much quicker than 'find' which searches the filesystem in real-time. Just type 'locate filename' and instantly get all matching files. The catch: the database is typically updated only once a day, so very recent files won't appear until you run 'sudo updatedb' manually. Great for finding config files, programs, or documents when you know the name but not the location. Use -i for case-insensitive search. The speed difference is dramatic on large filesystems!",
            common_flags: vec![
                FlagInfo { flag: "-i", description: "Ignore case: case-insensitive search", example: Some("locate -i filename") },
            ],
            examples: vec![
                Example { command: "locate filename", description: "Find file quickly", use_case: "Fast file search" },
                Example { command: "sudo updatedb && locate file", description: "Update DB and search", use_case: "Search with fresh index" },
            ],
            related_commands: vec!["find", "which", "updatedb"],
        });

        self.register(CommandInfo {
            name: "whereis".to_string(),
            category: CommandCategory::Search,
            danger_level: DangerLevel::Safe,
            summary: "Locate binary, source, and manual pages",
            description: "Locates not just where a command is (like 'which'), but also its source code and documentation if available. For example, 'whereis ls' might show you the binary at /bin/ls, the source at /usr/src/coreutils/ls.c, and the man page at /usr/share/man/man1/ls.1.gz. More comprehensive than 'which' for understanding where all the pieces of a program live on your system. Useful for developers who need to find source code or when you want to see where documentation is stored. Searches standard system directories quickly.",
            common_flags: vec![],
            examples: vec![
                Example { command: "whereis ls", description: "Find ls locations", use_case: "Locate command and docs" },
            ],
            related_commands: vec!["which", "locate", "find"],
        });

        // More utilities
        self.register(CommandInfo {
            name: "exit".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Exit the shell",
            description: "Closes your current terminal session or shell. Just type 'exit' and press Enter to quit. In scripts, you can specify an exit code like 'exit 0' (success) or 'exit 1' (error) to indicate how the script finished - other programs can check this code. If you're in an SSH session, exit logs you out and closes the connection. If it's your last terminal window, the window closes. You can also usually use Ctrl+D as a keyboard shortcut for exit. Simple but essential for cleanly closing shells and scripts.",
            common_flags: vec![],
            examples: vec![
                Example { command: "exit", description: "Close shell", use_case: "End terminal session" },
                Example { command: "exit 1", description: "Exit with error code", use_case: "Signal script failure" },
            ],
            related_commands: vec!["logout", "Ctrl+D"],
        });

        self.register(CommandInfo {
            name: "source".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Caution,
            summary: "Execute commands from file in current shell",
            description: "Executes a script file in your CURRENT shell session, unlike running it normally which creates a new shell. This is crucial - any variables, functions, or environment changes the script makes will persist in your current session. Most commonly used with 'source ~/.bashrc' to reload your bash configuration after editing it, making changes take effect without logging out. The dot command '.' is a shorthand for source. Essential for applying configuration changes, loading environment variables, or running setup scripts that need to modify your current shell environment.",
            common_flags: vec![],
            examples: vec![
                Example { command: "source ~/.bashrc", description: "Reload bash config", use_case: "Apply config changes" },
                Example { command: ". ./script.sh", description: "Same as source (dot command)", use_case: "Run in current shell" },
            ],
            related_commands: vec![".", "bash"],
        });

        self.register(CommandInfo {
            name: "id".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Print user and group IDs",
            description: "Displays detailed identity information including your user ID (UID), primary group ID (GID), and all groups you belong to with their numeric IDs. More comprehensive than 'whoami' or 'groups' alone. The UID and GID numbers are what Linux actually uses internally for permissions - usernames are just friendly labels. Use 'id -u' to get just your UID (useful in scripts), or 'id username' to check another user's info. Essential for understanding permissions, troubleshooting access issues, or verifying what account a process will run as.",
            common_flags: vec![
                FlagInfo { flag: "-u", description: "User ID: print only UID", example: Some("id -u") },
                FlagInfo { flag: "-g", description: "Group ID: print only primary GID", example: Some("id -g") },
            ],
            examples: vec![
                Example { command: "id", description: "Show user/group info", use_case: "Check user identity" },
                Example { command: "id username", description: "Show info for user", use_case: "Check another user's IDs" },
            ],
            related_commands: vec!["whoami", "groups"],
        });

        self.register(CommandInfo {
            name: "who".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Show who is logged in",
            description: "Lists all users currently logged into the system, showing their username, terminal, login time, and where they logged in from (if remote). Useful on multi-user systems or servers to see who else is online. For example, you might see other administrators connected via SSH. Simple but informative - just type 'who' with no arguments. On a single-user desktop you'll usually only see yourself, but on servers it's important for coordination and security auditing. The 'w' command shows similar info plus what each user is doing.",
            common_flags: vec![],
            examples: vec![
                Example { command: "who", description: "List logged in users", use_case: "See active users" },
            ],
            related_commands: vec!["w", "whoami", "users"],
        });

        self.register(CommandInfo {
            name: "w".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Show who is logged in and what they're doing",
            description: "An enhanced version of 'who' that shows not just who's logged in, but also what command they're currently running, how long they've been idle, and system load averages. Like looking over everyone's shoulder to see what they're doing. The first line shows uptime and load, then each user gets a line showing their login time, idle time, CPU usage, and current command. Perfect for system administrators monitoring server activity or seeing if that colleague is actually working or just has a terminal window open. More informative than 'who', less detailed than 'top'.",
            common_flags: vec![],
            examples: vec![
                Example { command: "w", description: "Show users and activity", use_case: "Monitor user activity" },
            ],
            related_commands: vec!["who", "uptime", "top"],
        });

        self.register(CommandInfo {
            name: "last".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Show listing of last logged in users",
            description: "Shows a historical log of all user logins and logouts, plus system reboots - like a visitor log for your computer. Each line shows who logged in, from where, when they logged in, when they logged out, and how long they were connected. Essential for security auditing (was my account accessed while I was away?), troubleshooting (when did the server last reboot?), or just curiosity. Use 'last -n 10' to see just the 10 most recent entries. The data comes from /var/log/wtmp, which keeps weeks or months of history depending on your system configuration.",
            common_flags: vec![],
            examples: vec![
                Example { command: "last", description: "Show login history", use_case: "Audit user access" },
                Example { command: "last -n 10", description: "Show last 10 entries", use_case: "Recent logins" },
            ],
            related_commands: vec!["who", "w", "lastlog"],
        });

        self.register(CommandInfo {
            name: "chgrp".to_string(),
            category: CommandCategory::Permissions,
            danger_level: DangerLevel::Caution,
            summary: "Change group ownership",
            description: "Changes which group owns a file or directory, affecting which users in that group can access it based on group permissions. Every file has both a user owner AND a group owner - chgrp changes the latter. For example, 'chgrp developers project.txt' makes the 'developers' group the owner, so anyone in that group gets the group permissions (typically read access). Use -R to change group ownership recursively for entire directories. Common when setting up shared project folders where multiple team members need access. Less drastic than chown which changes the user owner too.",
            common_flags: vec![
                FlagInfo { flag: "-R", description: "Recursive: change group recursively", example: Some("chgrp -R group folder/") },
            ],
            examples: vec![
                Example { command: "chgrp developers file.txt", description: "Change file group", use_case: "Set group ownership" },
            ],
            related_commands: vec!["chown", "chmod", "groups"],
        });

        self.register(CommandInfo {
            name: "umask".to_string(),
            category: CommandCategory::Permissions,
            danger_level: DangerLevel::Safe,
            summary: "Set default file permissions",
            description: "Sets the default permissions for newly created files and directories by specifying which permissions to REMOVE (it's a mask). A bit counterintuitive - umask 022 means \"remove write permission for group and others\", resulting in files created with 644 (rw-r--r--) permissions. Run 'umask' alone to see your current setting, usually 022 or 002. Use 'umask 077' for maximum privacy (only you can read your new files). The setting only lasts for your current shell session unless you add it to your .bashrc. Important for security-conscious users who want to control default file access.",
            common_flags: vec![],
            examples: vec![
                Example { command: "umask", description: "Show current umask", use_case: "Check permission defaults" },
                Example { command: "umask 022", description: "Set umask to 022", use_case: "Set file creation defaults" },
            ],
            related_commands: vec!["chmod", "mkdir", "touch"],
        });

        // Hardware detection (essential for Arch installation)
        self.register(CommandInfo {
            name: "lspci".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "List all PCI devices",
            description: "Lists all PCI devices connected to your system - graphics cards, network cards, sound cards, USB controllers, etc. Like Device Manager on Windows but for the command line. Use 'lspci -k' to also see which kernel drivers are loaded for each device - crucial when installing Arch Linux to verify hardware is detected and has the right drivers. The -v flag gives verbose details about each device. Essential for troubleshooting hardware issues, checking if your GPU is recognized, or verifying that network hardware is present before installing drivers.",
            common_flags: vec![
                FlagInfo { flag: "-v", description: "Verbose: detailed information", example: Some("lspci -v") },
                FlagInfo { flag: "-k", description: "Kernel: show kernel drivers", example: Some("lspci -k") },
            ],
            examples: vec![
                Example { command: "lspci", description: "List PCI devices", use_case: "See hardware components" },
                Example { command: "lspci -k", description: "Show with kernel drivers", use_case: "Check if drivers loaded" },
            ],
            related_commands: vec!["lsusb", "lshw"],
        });

        self.register(CommandInfo {
            name: "lsusb".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "List USB devices",
            description: "Displays all USB devices currently connected to your computer - mice, keyboards, external drives, webcams, phones, and more. Each line shows the bus number, device number, USB ID (vendor:product), and device name. Great for troubleshooting when a USB device isn't working - if it doesn't appear in lsusb, it's a hardware/connection problem. Use -v for verbose output with detailed technical specs. Simple command that answers the question \"did my computer even detect this USB thing I just plugged in?\" Essential for debugging USB issues.",
            common_flags: vec![
                FlagInfo { flag: "-v", description: "Verbose: detailed information", example: Some("lsusb -v") },
            ],
            examples: vec![
                Example { command: "lsusb", description: "List USB devices", use_case: "See connected USB hardware" },
            ],
            related_commands: vec!["lspci", "dmesg"],
        });

        self.register(CommandInfo {
            name: "lshw".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "List hardware configuration",
            description: "A comprehensive hardware information tool that shows detailed specs for ALL your system hardware - CPU, RAM, motherboard, storage, network interfaces, graphics, and more. More thorough than lspci or lsusb alone. Use 'sudo lshw -short' for a nice summary table, or just 'sudo lshw' for the full detailed tree. The output is extensive, showing capabilities, configurations, and drivers for each component. Perfect for creating a complete hardware inventory, verifying specs before buying RAM, or diagnosing hardware issues. Requires sudo for full information.",
            common_flags: vec![
                FlagInfo { flag: "-short", description: "Brief output", example: Some("lshw -short") },
            ],
            examples: vec![
                Example { command: "sudo lshw -short", description: "Show hardware summary", use_case: "Quick hardware overview" },
            ],
            related_commands: vec!["lspci", "lsusb", "hwinfo"],
        });

        // Filesystem tools (critical for Arch installation)
        self.register(CommandInfo {
            name: "mkfs".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Critical,
            summary: "Build a Linux filesystem",
            description: "CRITICAL: Creates a new filesystem on a partition, PERMANENTLY ERASING ALL EXISTING DATA! Like formatting a drive in Windows but from the command line. Essential during Arch Linux installation when you need to format partitions as ext4, FAT32 (for EFI), or other filesystem types. Use 'sudo mkfs.ext4 /dev/sda1' to format partition sda1 as ext4, or 'sudo mkfs.fat -F32 /dev/sda1' for FAT32. TRIPLE-CHECK the device name before running - one typo and you could erase the wrong drive! Always unmount the partition first. This is a point of no return command.",
            common_flags: vec![
                FlagInfo { flag: "-t", description: "Type: specify filesystem type", example: Some("mkfs -t ext4 /dev/sda1") },
            ],
            examples: vec![
                Example { command: "sudo mkfs.ext4 /dev/sda1", description: "Create ext4 filesystem", use_case: "Format partition for Arch" },
                Example { command: "sudo mkfs.fat -F32 /dev/sda1", description: "Create FAT32 (for EFI)", use_case: "Format EFI partition" },
            ],
            related_commands: vec!["fdisk", "parted", "fsck"],
        });

        self.register(CommandInfo {
            name: "fsck".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Dangerous,
            summary: "Check and repair filesystem",
            description: "Careful: File System ChecK - scans a partition for errors and attempts to repair them, like running CHKDSK on Windows. Critically important: NEVER run fsck on a mounted (in-use) filesystem - you must unmount it first or you'll cause more damage! Usually run automatically during boot if the system wasn't shut down cleanly. Manual use is for when a partition won't mount or you suspect corruption. Use 'sudo fsck /dev/sda1' to check a partition, add -y to automatically fix errors. Essential recovery tool when filesystems get corrupted, but use carefully and always on unmounted partitions.",
            common_flags: vec![
                FlagInfo { flag: "-a", description: "Automatic: automatically repair", example: Some("fsck -a /dev/sda1") },
            ],
            examples: vec![
                Example { command: "sudo fsck /dev/sda1", description: "Check filesystem", use_case: "Repair corrupted partition" },
            ],
            related_commands: vec!["mkfs", "e2fsck"],
        });

        self.register(CommandInfo {
            name: "blkid".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Locate/print block device attributes",
            description: "Displays critical information about block devices (hard drives and partitions) including their UUIDs (unique identifiers), filesystem types, and labels. UUIDs are super important because they never change even if you reorder drives, unlike device names (/dev/sda1 might become /dev/sdb1). Essential when setting up /etc/fstab during Arch installation - you need the UUID to tell the system which partitions to mount at boot. Just run 'blkid' to see all devices, or 'blkid /dev/sda1' for a specific partition. Copy those UUIDs when configuring your system!",
            common_flags: vec![],
            examples: vec![
                Example { command: "blkid", description: "Show all block device UUIDs", use_case: "Get UUIDs for fstab" },
                Example { command: "blkid /dev/sda1", description: "Show specific partition info", use_case: "Check partition UUID" },
            ],
            related_commands: vec!["lsblk", "fdisk"],
        });

        // Arch-specific tools
        self.register(CommandInfo {
            name: "yay".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Caution,
            summary: "AUR helper (Arch User Repository)",
            description: "The most popular AUR helper for Arch Linux, making it easy to install packages from both the official Arch repositories AND the AUR (Arch User Repository) - a huge collection of community-maintained packages. Use 'yay package-name' to search and install with an interactive menu, or 'yay -Syu' to update your entire system including AUR packages. It's like pacman but friendlier and with access to way more software. Essential for Arch users who want easy access to the latest software that isn't in official repos. Note: builds packages from source, so installation can take longer.",
            common_flags: vec![
                FlagInfo { flag: "-S", description: "Sync: install package", example: Some("yay -S package") },
                FlagInfo { flag: "-Syu", description: "System upgrade (AUR + official)", example: Some("yay -Syu") },
            ],
            examples: vec![
                Example { command: "yay -Syu", description: "Full system update", use_case: "Update all packages including AUR" },
                Example { command: "yay package", description: "Search and install from AUR", use_case: "Install AUR package" },
            ],
            related_commands: vec!["pacman", "paru"],
        });

        self.register(CommandInfo {
            name: "paru".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Caution,
            summary: "AUR helper (modern alternative to yay)",
            description: "A modern, feature-rich AUR helper written in Rust (making it fast and reliable), positioned as the next-generation replacement for yay. Has all of yay's functionality plus additional features like better package review, bat integration for syntax highlighting, and more efficient updating. Use the same commands as yay: 'paru -Syu' for updates, 'paru package-name' to install. Many Arch users are switching from yay to paru for its speed and extra features. If you're starting fresh with Arch, paru is probably the better choice for the future.",
            common_flags: vec![
                FlagInfo { flag: "-S", description: "Sync: install package", example: Some("paru -S package") },
                FlagInfo { flag: "-Syu", description: "System upgrade", example: Some("paru -Syu") },
            ],
            examples: vec![
                Example { command: "paru -Syu", description: "Full system update", use_case: "Update Arch system" },
            ],
            related_commands: vec!["pacman", "yay"],
        });

        self.register(CommandInfo {
            name: "mkinitcpio".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Dangerous,
            summary: "Create initial ramdisk (Arch)",
            description: "Careful: Generates the initramfs (initial RAM filesystem) - a crucial boot component that loads necessary drivers and prepares your system before the real root filesystem mounts. On Arch Linux, you run this after kernel updates or when changing hardware drivers in /etc/mkinitcpio.conf. Use 'sudo mkinitcpio -P' to regenerate all initramfs images for all installed kernels. If you forget to run this after a kernel update, your system might not boot! The initramfs is what makes Linux boot fast by loading only essential drivers first. Critical for Arch maintenance.",
            common_flags: vec![
                FlagInfo { flag: "-P", description: "Preset: use preset from /etc/mkinitcpio.d/", example: Some("mkinitcpio -P") },
                FlagInfo { flag: "-p", description: "Preset: generate for specific preset", example: Some("mkinitcpio -p linux") },
            ],
            examples: vec![
                Example { command: "sudo mkinitcpio -P", description: "Regenerate all initramfs", use_case: "After kernel update" },
                Example { command: "sudo mkinitcpio -p linux", description: "Regenerate for linux kernel", use_case: "Rebuild initramfs" },
            ],
            related_commands: vec!["pacman", "grub-mkconfig"],
        });

        // Boot tools
        self.register(CommandInfo {
            name: "grub-install".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Critical,
            summary: "Install GRUB bootloader",
            description: "CRITICAL: Installs the GRUB bootloader to your disk - the program that runs when you turn on your computer and lets you choose which operating system to boot. Essential during Arch installation! For UEFI systems use 'sudo grub-install --target=x86_64-efi --efi-directory=/boot --bootloader-id=GRUB'. For BIOS systems use 'sudo grub-install --target=i386-pc /dev/sda'. Get this wrong and your computer won't boot! GRUB is what shows that menu when you start your computer. After installing, run grub-mkconfig to generate the configuration.",
            common_flags: vec![
                FlagInfo { flag: "--target", description: "Target platform (x86_64-efi, i386-pc)", example: Some("grub-install --target=x86_64-efi") },
                FlagInfo { flag: "--efi-directory", description: "EFI directory mount point", example: Some("grub-install --efi-directory=/boot") },
            ],
            examples: vec![
                Example { command: "sudo grub-install --target=x86_64-efi --efi-directory=/boot", description: "Install GRUB (UEFI)", use_case: "Set up bootloader on UEFI system" },
            ],
            related_commands: vec!["grub-mkconfig", "efibootmgr"],
        });

        self.register(CommandInfo {
            name: "grub-mkconfig".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Caution,
            summary: "Generate GRUB configuration",
            description: "Generates the GRUB configuration file that tells GRUB which operating systems and kernels are available to boot. Scans your system, finds all installed kernels and OSes, and creates the boot menu. Run 'sudo grub-mkconfig -o /boot/grub/grub.cfg' after installing a new kernel, updating GRUB, or dual-booting with Windows. On Ubuntu/Debian, you might use 'update-grub' instead which is a wrapper for this command. This is what makes new kernels appear in your boot menu. Essential after system updates that install new kernels.",
            common_flags: vec![
                FlagInfo { flag: "-o", description: "Output: specify output file", example: Some("grub-mkconfig -o /boot/grub/grub.cfg") },
            ],
            examples: vec![
                Example { command: "sudo grub-mkconfig -o /boot/grub/grub.cfg", description: "Regenerate GRUB config", use_case: "Update bootloader menu" },
            ],
            related_commands: vec!["grub-install", "update-grub"],
        });

        // Compiler tools (for building from source)
        self.register(CommandInfo {
            name: "gcc".to_string(),
            category: CommandCategory::Build,
            danger_level: DangerLevel::Safe,
            summary: "GNU C Compiler",
            description: "The GNU C Compiler - the standard compiler for C programming on Linux and a cornerstone of open-source development. Turns your C source code into executable programs. Use 'gcc program.c -o program' to compile program.c into an executable called 'program'. Essential for building software from source - many programs you download come as C code that needs compiling. The -O2 flag enables optimization for faster code, -Wall shows all warnings (highly recommended!). If you're learning C or building Linux from source, you'll use gcc constantly. It's been around since 1987 and powers much of the software world.",
            common_flags: vec![
                FlagInfo { flag: "-o", description: "Output: specify output filename", example: Some("gcc program.c -o program") },
                FlagInfo { flag: "-O2", description: "Optimize: optimization level 2", example: Some("gcc -O2 program.c") },
            ],
            examples: vec![
                Example { command: "gcc program.c -o program", description: "Compile C program", use_case: "Build from source" },
                Example { command: "gcc -Wall -O2 program.c", description: "Compile with warnings and optimization", use_case: "Production build" },
            ],
            related_commands: vec!["g++", "clang", "make"],
        });

        self.register(CommandInfo {
            name: "g++".to_string(),
            category: CommandCategory::Build,
            danger_level: DangerLevel::Safe,
            summary: "GNU C++ Compiler",
            description: "The GNU C++ Compiler - gcc's counterpart for C++ programming. Compiles C++ code which includes object-oriented features, templates, and the STL that C doesn't have. Use 'g++ program.cpp -o program' to compile. The -std flag lets you choose the C++ standard version (c++11, c++17, c++20, c++23) - newer standards have more features. Essential for building C++ software, game engines, performance-critical applications, and much of the modern software stack. Like gcc but for when you need classes, templates, and modern C++ features.",
            common_flags: vec![
                FlagInfo { flag: "-std", description: "Standard: C++ standard (c++11, c++17, c++20)", example: Some("g++ -std=c++17 program.cpp") },
            ],
            examples: vec![
                Example { command: "g++ program.cpp -o program", description: "Compile C++ program", use_case: "Build C++ code" },
            ],
            related_commands: vec!["gcc", "clang++"],
        });

        // Debugging and analysis tools
        self.register(CommandInfo {
            name: "strace".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "Trace system calls and signals",
            description: "A powerful debugging tool that shows every system call (request to the kernel) a program makes - file opens, network connections, memory allocations, everything. Like x-ray vision for programs. Use 'strace command' to see what a program is doing under the hood, or 'strace -p PID' to attach to a running process. Essential for debugging when a program mysteriously fails - you can see exactly where it's trying to open files, what permissions it needs, or why it's hanging. The output is verbose but incredibly informative. Every systems programmer's secret weapon.",
            common_flags: vec![
                FlagInfo { flag: "-f", description: "Follow: trace child processes", example: Some("strace -f command") },
            ],
            examples: vec![
                Example { command: "strace ls", description: "Trace ls command", use_case: "See what a program does" },
                Example { command: "strace -p 1234", description: "Attach to running process", use_case: "Debug running program" },
            ],
            related_commands: vec!["ltrace", "gdb"],
        });

        self.register(CommandInfo {
            name: "lsof".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "List open files",
            description: "Lists all open files and which processes have them open - remember, in Linux \"everything is a file\" including network connections and devices! Super useful for answering questions like \"which process is using port 80?\" ('lsof -i :80'), \"why can't I unmount this drive?\" ('lsof /mnt'), or \"what files is this process accessing?\" The name means \"list open files\". Essential for troubleshooting locked files, finding processes listening on ports, or investigating what a suspicious process is accessing. One of the most versatile debugging tools in Linux.",
            common_flags: vec![
                FlagInfo { flag: "-i", description: "Internet: show network connections", example: Some("lsof -i :80") },
            ],
            examples: vec![
                Example { command: "lsof /var/log/syslog", description: "Who's using this file", use_case: "Find process accessing file" },
                Example { command: "lsof -i :80", description: "What's using port 80", use_case: "Find process on port" },
            ],
            related_commands: vec!["netstat", "ps", "fuser"],
        });

        self.register(CommandInfo {
            name: "gdb".to_string(),
            category: CommandCategory::Build,
            danger_level: DangerLevel::Safe,
            summary: "GNU Debugger",
            description: "The GNU Debugger - an incredibly powerful interactive debugger for C/C++ programs that lets you step through code line-by-line, set breakpoints, examine variables, and figure out exactly where bugs are. Load a program with 'gdb ./program', set breakpoints with 'break main', run with 'run', and step through with 'step' or 'next'. You can examine memory, modify variables mid-execution, and get stack traces when crashes occur. Learning curve is steep but it's the gold standard for debugging compiled code. Every serious C/C++ developer needs to know gdb basics.",
            common_flags: vec![],
            examples: vec![
                Example { command: "gdb program", description: "Debug program", use_case: "Interactive debugging" },
                Example { command: "gdb -p 1234", description: "Attach to process", use_case: "Debug running program" },
            ],
            related_commands: vec!["lldb", "strace"],
        });

        // Advanced networking
        self.register(CommandInfo {
            name: "ss".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Safe,
            summary: "Socket statistics (modern netstat)",
            description: "The modern, faster replacement for the classic 'netstat' command, showing socket statistics and network connections. Use 'ss -tuln' to see all TCP/UDP listening ports (what services are waiting for connections), or 'ss -tulnp' to also show which process is listening. Much faster than netstat, especially on busy servers with thousands of connections. Essential for network troubleshooting - check what ports are open, see active connections, diagnose why something can't bind to a port. If you're learning network commands, learn 'ss' instead of the aging 'netstat'.",
            common_flags: vec![
                FlagInfo { flag: "-tuln", description: "TCP/UDP listening with numbers", example: Some("ss -tuln") },
                FlagInfo { flag: "-p", description: "Processes: show process using socket", example: Some("ss -tulnp") },
            ],
            examples: vec![
                Example { command: "ss -tuln", description: "Show listening ports", use_case: "Check open ports" },
                Example { command: "ss -tulnp", description: "Show with processes", use_case: "See what's listening" },
            ],
            related_commands: vec!["netstat", "lsof"],
        });

        self.register(CommandInfo {
            name: "iptables".to_string(),
            category: CommandCategory::Networking,
            danger_level: DangerLevel::Critical,
            summary: "IPv4 packet filtering and NAT",
            description: "CRITICAL: The low-level Linux firewall configuration tool - extremely powerful but complex and dangerous if used incorrectly. Controls which network packets are allowed in or out of your system. One wrong command can lock you out of a remote server permanently! Use 'iptables -L' to list current rules safely. Most users should use friendlier front-ends like 'ufw' (Ubuntu) or 'firewalld' (RedHat) instead of raw iptables. Only use this directly if you know what you're doing or are following exact instructions. Being replaced by 'nftables' on modern systems but still widely used.",
            common_flags: vec![
                FlagInfo { flag: "-L", description: "List: show all rules", example: Some("iptables -L") },
                FlagInfo { flag: "-A", description: "Append: add new rule", example: Some("iptables -A INPUT -p tcp --dport 22 -j ACCEPT") },
            ],
            examples: vec![
                Example { command: "sudo iptables -L", description: "List firewall rules", use_case: "See current firewall config" },
            ],
            related_commands: vec!["ufw", "firewalld", "nftables"],
        });

        // Text editors we might have missed
        self.register(CommandInfo {
            name: "emacs".to_string(),
            category: CommandCategory::TextProcessing,
            danger_level: DangerLevel::Safe,
            summary: "Extensible text editor",
            description: "A legendary, incredibly powerful and extensible text editor that's also a complete computing environment - some people run email, calendars, git, terminals, and more all within Emacs! Famously rivals Vim in the \"editor wars\". Unlike modal editors, you can start typing immediately, but it uses extensive keyboard shortcuts (Ctrl+X Ctrl+S to save, Ctrl+X Ctrl+C to quit). Infinitely customizable with Emacs Lisp. The learning curve is steep and it's large/slow compared to Vim, but devotees swear by its power and consistency. More than an editor - it's a way of life for some developers!",
            common_flags: vec![],
            examples: vec![
                Example { command: "emacs file.txt", description: "Edit file", use_case: "Advanced text editing" },
            ],
            related_commands: vec!["vim", "nano"],
        });

        // System information we might have missed
        self.register(CommandInfo {
            name: "dmidecode".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Safe,
            summary: "DMI table decoder (hardware info)",
            description: "Reads your system's DMI (Desktop Management Interface) tables from BIOS/UEFI to show detailed hardware information - motherboard model, RAM specs, CPU details, serial numbers, and more. Like reading the hardware database that your BIOS maintains. Use 'sudo dmidecode -t memory' to see RAM details (type, speed, slots), or 'sudo dmidecode -t system' for motherboard info. Great for finding out exact hardware specs without opening the case, verifying compatible RAM before upgrading, or getting serial numbers for warranty claims. Requires sudo to access the DMI tables.",
            common_flags: vec![
                FlagInfo { flag: "-t", description: "Type: show specific type", example: Some("dmidecode -t memory") },
            ],
            examples: vec![
                Example { command: "sudo dmidecode -t system", description: "Show system information", use_case: "Get hardware details" },
                Example { command: "sudo dmidecode -t memory", description: "Show RAM information", use_case: "Check memory specs" },
            ],
            related_commands: vec!["lshw", "hwinfo"],
        });

        self.register(CommandInfo {
            name: "arch-chroot".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Dangerous,
            summary: "Enhanced chroot for Arch installation",
            description: "Careful: An enhanced version of the 'chroot' command specifically designed for Arch Linux installation. Changes your root directory to a mounted Arch installation, automatically mounting necessary filesystems (/proc, /sys, /dev) so you can configure the new system before booting into it. During Arch installation, after running pacstrap, you use 'arch-chroot /mnt' to enter your new Arch system and configure the bootloader, set timezone, create users, etc. It's like stepping into your new OS before it's actually installed. Essential Arch installation tool - makes configuration much easier than manual chroot.",
            common_flags: vec![],
            examples: vec![
                Example { command: "arch-chroot /mnt", description: "Chroot into new system", use_case: "Configure Arch installation" },
            ],
            related_commands: vec!["chroot", "pacstrap"],
        });

        self.register(CommandInfo {
            name: "pacstrap".to_string(),
            category: CommandCategory::Package,
            danger_level: DangerLevel::Caution,
            summary: "Install packages to new root (Arch install)",
            description: "Installs packages into a new Arch Linux system during installation - the command that actually puts Arch on your drive! Use it like 'pacstrap /mnt base linux linux-firmware' to install the base system, kernel, and firmware to your mounted partition. This is the first major step after partitioning and formatting during Arch installation. You specify which packages to include in your base install - minimal Arch just needs 'base', but you typically also want the kernel ('linux'), firmware, and maybe a text editor. Essential Arch installation tool that bootstraps your new system.",
            common_flags: vec![],
            examples: vec![
                Example { command: "pacstrap /mnt base linux linux-firmware", description: "Install base system", use_case: "Bootstrap Arch installation" },
            ],
            related_commands: vec!["arch-chroot", "pacman"],
        });

        self.register(CommandInfo {
            name: "genfstab".to_string(),
            category: CommandCategory::SystemInfo,
            danger_level: DangerLevel::Caution,
            summary: "Generate /etc/fstab file",
            description: "Automatically generates the /etc/fstab file by detecting all currently mounted filesystems and their mount points. The fstab file tells Linux which partitions to mount at boot and where. During Arch installation, after mounting your partitions, run 'genfstab -U /mnt >> /mnt/etc/fstab' to create the fstab using UUIDs (recommended over device names). This saves you from manually writing fstab entries which is error-prone. Always verify the generated file before rebooting - a broken fstab can prevent your system from booting! Essential Arch installation step.",
            common_flags: vec![
                FlagInfo { flag: "-U", description: "UUIDs: use UUIDs instead of device names", example: Some("genfstab -U /mnt") },
            ],
            examples: vec![
                Example { command: "genfstab -U /mnt >> /mnt/etc/fstab", description: "Generate fstab with UUIDs", use_case: "Set up Arch filesystem table" },
            ],
            related_commands: vec!["blkid", "mount"],
        });
    }

    fn register(&mut self, info: CommandInfo) {
        self.commands.insert(info.name.clone(), info);
    }
}

impl Default for CommandAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate Levenshtein distance for typo detection
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();
    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }

    for (i, c1) in s1.chars().enumerate() {
        for (j, c2) in s2.chars().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[len1][len2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_command() {
        let analyzer = CommandAnalyzer::new();
        let cmd = analyzer.parse("ls").unwrap();
        assert_eq!(cmd.program, "ls");
        assert_eq!(cmd.args.len(), 0);
    }

    #[test]
    fn test_parse_command_with_flags() {
        let analyzer = CommandAnalyzer::new();
        let cmd = analyzer.parse("ls -lah /tmp").unwrap();
        assert_eq!(cmd.program, "ls");
        assert_eq!(cmd.flags.len(), 3);
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("cat", "cat"), 0);
        assert_eq!(levenshtein_distance("cat", "bat"), 1);
        assert_eq!(levenshtein_distance("cat", "car"), 1);
    }
}
