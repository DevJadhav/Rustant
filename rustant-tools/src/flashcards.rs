//! Flashcard tool — spaced repetition learning with SM-2 algorithm.

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Flashcard {
    id: usize,
    deck: String,
    front: String,
    back: String,
    // SM-2 fields
    easiness_factor: f64,
    interval_days: f64,
    repetitions: u32,
    next_review: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

impl Flashcard {
    fn new(id: usize, deck: &str, front: &str, back: &str) -> Self {
        Self {
            id,
            deck: deck.to_string(),
            front: front.to_string(),
            back: back.to_string(),
            easiness_factor: 2.5,
            interval_days: 1.0,
            repetitions: 0,
            next_review: Utc::now(),
            created_at: Utc::now(),
        }
    }

    /// Apply SM-2 algorithm with quality 0-5.
    fn review(&mut self, quality: u32) {
        let q = quality.min(5) as f64;
        // Update easiness factor
        self.easiness_factor =
            (self.easiness_factor + 0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02)).max(1.3);

        if quality < 3 {
            // Failed — reset
            self.interval_days = 1.0;
            self.repetitions = 0;
        } else {
            self.repetitions += 1;
            match self.repetitions {
                1 => self.interval_days = 1.0,
                2 => self.interval_days = 6.0,
                _ => self.interval_days *= self.easiness_factor,
            }
        }
        self.next_review =
            Utc::now() + ChronoDuration::seconds((self.interval_days * 86400.0) as i64);
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FlashcardState {
    cards: Vec<Flashcard>,
    next_id: usize,
}

pub struct FlashcardsTool {
    workspace: PathBuf,
}

impl FlashcardsTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("flashcards")
            .join("cards.json")
    }

    fn load_state(&self) -> FlashcardState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            FlashcardState {
                cards: Vec::new(),
                next_id: 1,
            }
        }
    }

    fn save_state(&self, state: &FlashcardState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "flashcards".to_string(),
                message: e.to_string(),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "flashcards".to_string(),
            message: e.to_string(),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "flashcards".to_string(),
            message: e.to_string(),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "flashcards".to_string(),
            message: e.to_string(),
        })?;
        Ok(())
    }
}

#[async_trait]
impl Tool for FlashcardsTool {
    fn name(&self) -> &str {
        "flashcards"
    }
    fn description(&self) -> &str {
        "Spaced repetition flashcards with SM-2 algorithm. Actions: add_card, study, answer, list_decks, stats."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["add_card", "study", "answer", "list_decks", "stats"] },
                "deck": { "type": "string", "description": "Deck name" },
                "front": { "type": "string", "description": "Card front (question)" },
                "back": { "type": "string", "description": "Card back (answer)" },
                "card_id": { "type": "integer", "description": "Card ID (for answer)" },
                "quality": { "type": "integer", "description": "Answer quality 0-5 (0=forgot, 3=correct with difficulty, 5=easy)" }
            },
            "required": ["action"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let mut state = self.load_state();

        match action {
            "add_card" => {
                let deck = args
                    .get("deck")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                let front = args.get("front").and_then(|v| v.as_str()).unwrap_or("");
                let back = args.get("back").and_then(|v| v.as_str()).unwrap_or("");
                if front.is_empty() || back.is_empty() {
                    return Ok(ToolOutput::text(
                        "Provide both front and back for the card.",
                    ));
                }
                let id = state.next_id;
                state.next_id += 1;
                state.cards.push(Flashcard::new(id, deck, front, back));
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Added card #{id} to deck '{deck}'."
                )))
            }
            "study" => {
                let deck_filter = args.get("deck").and_then(|v| v.as_str());
                let now = Utc::now();
                let due: Vec<&Flashcard> = state
                    .cards
                    .iter()
                    .filter(|c| c.next_review <= now)
                    .filter(|c| deck_filter.map(|d| c.deck == d).unwrap_or(true))
                    .take(1)
                    .collect();
                if due.is_empty() {
                    return Ok(ToolOutput::text("No cards due for review. Great job!"));
                }
                let card = due[0];
                Ok(ToolOutput::text(format!(
                    "Card #{} [{}]\n\nQ: {}\n\n(Use answer action with card_id={} and quality=0-5 to respond)",
                    card.id, card.deck, card.front, card.id
                )))
            }
            "answer" => {
                let card_id = args.get("card_id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let quality = args.get("quality").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
                if let Some(card) = state.cards.iter_mut().find(|c| c.id == card_id) {
                    let answer = card.back.clone();
                    card.review(quality);
                    let next = card.next_review.format("%Y-%m-%d");
                    self.save_state(&state)?;
                    let feedback = match quality {
                        0..=2 => "Keep studying! Card will appear again soon.",
                        3 => "Correct! Next review in a day.",
                        4 => "Good! Interval extended.",
                        _ => "Perfect! Long interval set.",
                    };
                    Ok(ToolOutput::text(format!(
                        "A: {answer}\n\n{feedback}\nNext review: {next}"
                    )))
                } else {
                    Ok(ToolOutput::text(format!("Card #{card_id} not found.")))
                }
            }
            "list_decks" => {
                let mut decks: std::collections::HashMap<&str, (usize, usize)> =
                    std::collections::HashMap::new();
                let now = Utc::now();
                for card in &state.cards {
                    let entry = decks.entry(&card.deck).or_insert((0, 0));
                    entry.0 += 1;
                    if card.next_review <= now {
                        entry.1 += 1;
                    }
                }
                if decks.is_empty() {
                    return Ok(ToolOutput::text("No flashcard decks yet."));
                }
                let lines: Vec<String> = decks
                    .iter()
                    .map(|(d, (total, due))| format!("  {d} — {total} cards ({due} due)"))
                    .collect();
                Ok(ToolOutput::text(format!("Decks:\n{}", lines.join("\n"))))
            }
            "stats" => {
                let total = state.cards.len();
                let now = Utc::now();
                let due = state.cards.iter().filter(|c| c.next_review <= now).count();
                let avg_ef: f64 = if total > 0 {
                    state.cards.iter().map(|c| c.easiness_factor).sum::<f64>() / total as f64
                } else {
                    0.0
                };
                Ok(ToolOutput::text(format!(
                    "Flashcard stats:\n  Total cards: {total}\n  Due now: {due}\n  Average EF: {avg_ef:.2}"
                )))
            }
            _ => Ok(ToolOutput::text(format!("Unknown action: {action}."))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sm2_easy_increases_interval() {
        let mut card = Flashcard::new(1, "test", "Q", "A");
        assert_eq!(card.interval_days, 1.0);
        card.review(5); // Perfect
        assert_eq!(card.repetitions, 1);
        card.review(5); // Perfect again
        assert_eq!(card.repetitions, 2);
        assert!(card.interval_days >= 6.0);
        card.review(5); // Third time
        assert!(card.interval_days > 6.0); // Should grow
    }

    #[test]
    fn test_sm2_hard_resets() {
        let mut card = Flashcard::new(1, "test", "Q", "A");
        card.review(5);
        card.review(5);
        assert!(card.interval_days >= 6.0);
        card.review(1); // Failed
        assert_eq!(card.repetitions, 0);
        assert_eq!(card.interval_days, 1.0);
    }

    #[test]
    fn test_sm2_easiness_floor() {
        let mut card = Flashcard::new(1, "test", "Q", "A");
        for _ in 0..20 {
            card.review(0); // Worst quality
        }
        assert!(card.easiness_factor >= 1.3);
    }

    #[tokio::test]
    async fn test_flashcards_add_study() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = FlashcardsTool::new(workspace);
        tool.execute(json!({"action": "add_card", "deck": "rust", "front": "What is ownership?", "back": "A memory management system"})).await.unwrap();
        let result = tool.execute(json!({"action": "study"})).await.unwrap();
        assert!(result.content.contains("ownership"));
    }

    #[tokio::test]
    async fn test_flashcards_schema() {
        let dir = TempDir::new().unwrap();
        let tool = FlashcardsTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "flashcards");
    }
}
