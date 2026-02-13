//! Personal finance tool — track transactions and budgets.

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Transaction {
    id: usize,
    amount: f64,
    category: String,
    description: String,
    date: DateTime<Utc>,
    #[serde(default)]
    is_income: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Budget {
    category: String,
    limit: f64,
    period: String, // "monthly", "weekly"
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FinanceState {
    transactions: Vec<Transaction>,
    budgets: Vec<Budget>,
    next_id: usize,
}

pub struct FinanceTool {
    workspace: PathBuf,
}

impl FinanceTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("finance")
            .join("data.json")
    }

    fn load_state(&self) -> FinanceState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            FinanceState {
                transactions: Vec::new(),
                budgets: Vec::new(),
                next_id: 1,
            }
        }
    }

    fn save_state(&self, state: &FinanceState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "finance".to_string(),
                message: format!("Create dir failed: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "finance".to_string(),
            message: format!("Serialize failed: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "finance".to_string(),
            message: e.to_string(),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "finance".to_string(),
            message: e.to_string(),
        })?;
        Ok(())
    }
}

#[async_trait]
impl Tool for FinanceTool {
    fn name(&self) -> &str {
        "finance"
    }
    fn description(&self) -> &str {
        "Personal finance tracker. Actions: add_transaction, list, summary, budget_check, set_budget, export_csv."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add_transaction", "list", "summary", "budget_check", "set_budget", "export_csv"],
                    "description": "Action to perform"
                },
                "amount": { "type": "number", "description": "Transaction amount" },
                "category": { "type": "string", "description": "Category (e.g., food, transport, income)" },
                "description": { "type": "string", "description": "Transaction description" },
                "is_income": { "type": "boolean", "description": "Whether this is income (default: false)" },
                "limit": { "type": "number", "description": "Budget limit amount" },
                "period": { "type": "string", "description": "Budget period (monthly/weekly)" },
                "output": { "type": "string", "description": "Output file path for export" }
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
            "add_transaction" => {
                let amount = args.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if amount == 0.0 {
                    return Ok(ToolOutput::text("Please provide a non-zero amount."));
                }
                let category = args.get("category").and_then(|v| v.as_str()).unwrap_or("other");
                let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
                let is_income = args.get("is_income").and_then(|v| v.as_bool()).unwrap_or(false);
                let id = state.next_id;
                state.next_id += 1;
                state.transactions.push(Transaction {
                    id, amount, category: category.to_string(),
                    description: description.to_string(), date: Utc::now(), is_income,
                });
                self.save_state(&state)?;
                let kind = if is_income { "income" } else { "expense" };
                Ok(ToolOutput::text(format!("Added {} #{}: ${:.2} ({})", kind, id, amount, category)))
            }
            "list" => {
                if state.transactions.is_empty() {
                    return Ok(ToolOutput::text("No transactions recorded."));
                }
                let lines: Vec<String> = state.transactions.iter().rev().take(20).map(|t| {
                    let sign = if t.is_income { "+" } else { "-" };
                    format!("  #{} {} ${:.2} [{}] {} — {}", t.id, sign, t.amount, t.category,
                        t.description, t.date.format("%Y-%m-%d"))
                }).collect();
                Ok(ToolOutput::text(format!("Recent transactions:\n{}", lines.join("\n"))))
            }
            "summary" => {
                let mut income = 0.0;
                let mut expenses = 0.0;
                let mut by_category: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
                for t in &state.transactions {
                    if t.is_income {
                        income += t.amount;
                    } else {
                        expenses += t.amount;
                        *by_category.entry(t.category.clone()).or_insert(0.0) += t.amount;
                    }
                }
                let mut cats: Vec<_> = by_category.iter().collect();
                cats.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
                let cat_lines: Vec<String> = cats.iter().map(|(c, a)| format!("  {}: ${:.2}", c, a)).collect();
                Ok(ToolOutput::text(format!(
                    "Finance summary:\n  Income:   ${:.2}\n  Expenses: ${:.2}\n  Balance:  ${:.2}\n\nBy category:\n{}",
                    income, expenses, income - expenses, cat_lines.join("\n")
                )))
            }
            "budget_check" => {
                if state.budgets.is_empty() {
                    return Ok(ToolOutput::text("No budgets set. Use set_budget to create one."));
                }
                let now = Utc::now();
                let month_start = now.date_naive().with_day(1).unwrap_or(now.date_naive());
                let mut output = String::from("Budget status:\n");
                for budget in &state.budgets {
                    let spent: f64 = state.transactions.iter()
                        .filter(|t| !t.is_income && t.category == budget.category && t.date.date_naive() >= month_start)
                        .map(|t| t.amount)
                        .sum();
                    let pct = if budget.limit > 0.0 { (spent / budget.limit * 100.0) as u32 } else { 0 };
                    let status = if pct >= 100 { "OVER" } else if pct >= 80 { "WARNING" } else { "OK" };
                    output.push_str(&format!("  {} [{}]: ${:.2} / ${:.2} ({}%)\n",
                        budget.category, status, spent, budget.limit, pct));
                }
                Ok(ToolOutput::text(output))
            }
            "set_budget" => {
                let category = args.get("category").and_then(|v| v.as_str()).unwrap_or("");
                let limit = args.get("limit").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if category.is_empty() || limit <= 0.0 {
                    return Ok(ToolOutput::text("Provide category and limit > 0."));
                }
                let period = args.get("period").and_then(|v| v.as_str()).unwrap_or("monthly");
                // Update existing or add new
                if let Some(b) = state.budgets.iter_mut().find(|b| b.category == category) {
                    b.limit = limit;
                    b.period = period.to_string();
                } else {
                    state.budgets.push(Budget { category: category.to_string(), limit, period: period.to_string() });
                }
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!("Budget set: {} ${:.2}/{}", category, limit, period)))
            }
            "export_csv" => {
                let output_str = args.get("output").and_then(|v| v.as_str()).unwrap_or("finance_export.csv");
                let path = self.workspace.join(output_str);
                let mut csv = String::from("id,date,type,amount,category,description\n");
                for t in &state.transactions {
                    let kind = if t.is_income { "income" } else { "expense" };
                    csv.push_str(&format!("{},{},{},{:.2},{},{}\n",
                        t.id, t.date.format("%Y-%m-%d"), kind, t.amount, t.category,
                        t.description.replace(',', ";")));
                }
                std::fs::write(&path, &csv).map_err(|e| ToolError::ExecutionFailed {
                    name: "finance".to_string(),
                    message: e.to_string(),
                })?;
                Ok(ToolOutput::text(format!("Exported {} transactions to {}.", state.transactions.len(), output_str)))
            }
            _ => Ok(ToolOutput::text(format!("Unknown action: {}. Use: add_transaction, list, summary, budget_check, set_budget, export_csv", action))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_finance_add_list() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = FinanceTool::new(workspace);
        tool.execute(json!({"action": "add_transaction", "amount": 50.0, "category": "food", "description": "groceries"})).await.unwrap();
        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(result.content.contains("groceries"));
        assert!(result.content.contains("food"));
    }

    #[tokio::test]
    async fn test_finance_budget_check() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = FinanceTool::new(workspace);
        tool.execute(json!({"action": "set_budget", "category": "food", "limit": 200.0}))
            .await
            .unwrap();
        tool.execute(json!({"action": "add_transaction", "amount": 50.0, "category": "food"}))
            .await
            .unwrap();
        let result = tool
            .execute(json!({"action": "budget_check"}))
            .await
            .unwrap();
        assert!(result.content.contains("food"));
        assert!(result.content.contains("OK"));
    }

    #[tokio::test]
    async fn test_finance_summary() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = FinanceTool::new(workspace);
        tool.execute(json!({"action": "add_transaction", "amount": 1000.0, "category": "salary", "is_income": true})).await.unwrap();
        tool.execute(json!({"action": "add_transaction", "amount": 50.0, "category": "food"}))
            .await
            .unwrap();
        let result = tool.execute(json!({"action": "summary"})).await.unwrap();
        assert!(result.content.contains("Income"));
        assert!(result.content.contains("Balance"));
    }

    #[tokio::test]
    async fn test_finance_schema() {
        let dir = TempDir::new().unwrap();
        let tool = FinanceTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "finance");
    }
}
