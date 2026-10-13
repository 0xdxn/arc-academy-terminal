//! Educational content and explanation system

use crate::command::{Command, CommandAnalyzer, DangerLevel};
use crate::types::{Result, Severity};
use serde::{Deserialize, Serialize};

/// Comprehensive explanation for a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    pub summary: String,
    pub description: String,
    pub flag_explanations: Vec<FlagExplanation>,
    pub tips: Vec<LearningTip>,
    pub warnings: Vec<Warning>,
    pub examples: Vec<String>,
    pub related_commands: Vec<String>,
}

/// Explanation for a specific flag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagExplanation {
    pub flag: String,
    pub description: String,
    pub effect: String,
}

/// Educational tip or insight
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningTip {
    pub title: String,
    pub content: String,
    pub category: TipCategory,
}

/// Categories of learning tips
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipCategory {
    BestPractice,
    CommonMistake,
    ProTip,
    DidYouKnow,
    SafetyWarning,
}

/// Warning about dangerous operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub message: String,
    pub severity: Severity,
    pub suggestion: Option<String>,
}

/// Main educator that generates explanations
pub struct Educator {
    analyzer: CommandAnalyzer,
    #[allow(dead_code)] // Reserved for future tip deduplication feature
    tip_history: Vec<String>,
}

impl Educator {
    pub fn new() -> Self {
        Self {
            analyzer: CommandAnalyzer::new(),
            tip_history: Vec::new(),
        }
    }

    /// Generate a comprehensive explanation for a command
    pub fn explain(&mut self, command: &Command) -> Result<Explanation> {
        let info = self.analyzer.get_command_info(&command.program);

        let (summary, description, related_commands) = if let Some(info) = info {
            (
                info.summary.to_string(),
                info.description.to_string(),
                info.related_commands.iter().map(|s| s.to_string()).collect(),
            )
        } else {
            (
                format!("Execute '{}'", command.program),
                "This command is not in our knowledge base yet.".to_string(),
                Vec::new(),
            )
        };

        let flag_explanations = self.explain_flags(command);
        let tips = self.generate_tips(command);
        let warnings = self.generate_warnings(command);
        let examples = self.get_examples(&command.program);

        Ok(Explanation {
            summary,
            description,
            flag_explanations,
            tips,
            warnings,
            examples,
            related_commands,
        })
    }

    /// Generate explanations for all flags in the command
    fn explain_flags(&self, command: &Command) -> Vec<FlagExplanation> {
        let mut explanations = Vec::new();

        if let Some(info) = self.analyzer.get_command_info(&command.program) {
            for flag in &command.flags {
                let flag_str = if let Some(ch) = flag.short {
                    format!("-{}", ch)
                } else if let Some(ref long) = flag.long {
                    format!("--{}", long)
                } else {
                    continue;
                };

                // Find matching flag info
                for flag_info in &info.common_flags {
                    if flag_info.flag == flag_str {
                        explanations.push(FlagExplanation {
                            flag: flag_str.clone(),
                            description: flag_info.description.to_string(),
                            effect: format!("This modifies how {} behaves", command.program),
                        });
                        break;
                    }
                }
            }
        }

        explanations
    }

    /// Generate contextual learning tips
    fn generate_tips(&mut self, command: &Command) -> Vec<LearningTip> {
        let mut tips = Vec::new();

        // Add category-specific tips
        match command.category {
            crate::command::CommandCategory::Navigation => {
                tips.push(LearningTip {
                    title: "Navigation Pro Tip".to_string(),
                    content: "Use 'cd -' to toggle between your current and previous directory quickly!".to_string(),
                    category: TipCategory::ProTip,
                });
            }
            crate::command::CommandCategory::FileManagement => {
                if command.danger_level >= DangerLevel::Dangerous {
                    tips.push(LearningTip {
                        title: "Safety First".to_string(),
                        content: "Always use the -i flag for interactive confirmation with destructive commands.".to_string(),
                        category: TipCategory::SafetyWarning,
                    });
                }
            }
            _ => {}
        }

        // Add tips based on flags used
        if command.flags.iter().any(|f| f.short == Some('r')) && command.program == "rm" {
            tips.push(LearningTip {
                title: "Recursive Deletion".to_string(),
                content: "The -r flag removes directories and all their contents. Double-check the path before executing!".to_string(),
                category: TipCategory::CommonMistake,
            });
        }

        tips
    }

    /// Generate warnings for dangerous operations
    fn generate_warnings(&self, command: &Command) -> Vec<Warning> {
        let mut warnings = Vec::new();

        match command.danger_level {
            DangerLevel::Dangerous | DangerLevel::Critical => {
                warnings.push(Warning {
                    message: format!("'{}' can cause permanent data loss or system damage", command.program),
                    severity: Severity::Warning,
                    suggestion: Some("Consider using the -i flag for interactive confirmation".to_string()),
                });
            }
            _ => {}
        }

        // Specific dangerous patterns
        if command.program == "rm"
            && command.flags.iter().any(|f| f.short == Some('r'))
            && command.flags.iter().any(|f| f.short == Some('f'))
        {
            warnings.push(Warning {
                message: "Careful: 'rm -rf' is extremely dangerous! This will permanently delete everything without confirmation.".to_string(),
                severity: Severity::Critical,
                suggestion: Some("Remove the -f flag and use -i for safety, or specify the exact path you want to delete.".to_string()),
            });
        }

        if command.program == "chmod" && command.args.contains(&"777".to_string()) {
            warnings.push(Warning {
                message: "chmod 777 makes files readable, writable, and executable by everyone - a security risk!".to_string(),
                severity: Severity::Warning,
                suggestion: Some("Use more restrictive permissions like 755 or 644".to_string()),
            });
        }

        warnings
    }

    /// Get example commands
    fn get_examples(&self, program: &str) -> Vec<String> {
        if let Some(info) = self.analyzer.get_command_info(program) {
            info.examples
                .iter()
                .map(|ex| format!("{} - {}", ex.command, ex.description))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get pre-execution hint (shown before command runs)
    pub fn get_hint(&self, command: &Command) -> Option<String> {
        match command.danger_level {
            DangerLevel::Dangerous | DangerLevel::Critical => {
                Some(format!(
                    "Careful: This command can modify or delete data. Use with caution!"
                ))
            }
            _ => None,
        }
    }
}

impl Default for Educator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_educator_explain() {
        let mut educator = Educator::new();
        let analyzer = CommandAnalyzer::new();
        let cmd = analyzer.parse("ls -la").unwrap();
        let explanation = educator.explain(&cmd).unwrap();

        assert!(!explanation.summary.is_empty());
        assert!(!explanation.flag_explanations.is_empty());
    }

    #[test]
    fn test_dangerous_command_warning() {
        let mut educator = Educator::new();
        let analyzer = CommandAnalyzer::new();
        let cmd = analyzer.parse("rm -rf /tmp/test").unwrap();
        let explanation = educator.explain(&cmd).unwrap();

        assert!(!explanation.warnings.is_empty());
    }
}
