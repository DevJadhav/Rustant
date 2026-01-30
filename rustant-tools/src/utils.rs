//! Utility tools — simple built-in tools for echo, datetime, and calculation.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use std::time::Duration;

use crate::registry::Tool;

/// Echo tool — returns the input text unchanged.
pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echoes the input text back. Useful for testing and confirming values."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "The text to echo back"
                }
            },
            "required": ["text"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let text = args["text"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "echo".to_string(),
                reason: "missing required 'text' parameter".to_string(),
            })?;
        Ok(ToolOutput::text(text.to_string()))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

/// DateTime tool — returns the current date and time.
pub struct DateTimeTool;

#[async_trait]
impl Tool for DateTimeTool {
    fn name(&self) -> &str {
        "datetime"
    }

    fn description(&self) -> &str {
        "Returns the current date and time in the specified format (default: RFC 3339)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "format": {
                    "type": "string",
                    "description": "strftime format string (default: RFC 3339)",
                    "default": "%Y-%m-%dT%H:%M:%S%z"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let now = chrono::Utc::now();
        let formatted = if let Some(fmt) = args.get("format").and_then(|f| f.as_str()) {
            now.format(fmt).to_string()
        } else {
            now.to_rfc3339()
        };
        Ok(ToolOutput::text(formatted))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

/// Calculator tool — evaluates simple arithmetic expressions.
pub struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Evaluates a simple arithmetic expression. Supports +, -, *, /, and parentheses."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "The arithmetic expression to evaluate, e.g. '2 + 3 * (4 - 1)'"
                }
            },
            "required": ["expression"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let expr = args["expression"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "calculator".to_string(),
                reason: "missing required 'expression' parameter".to_string(),
            })?;

        match eval_expression(expr) {
            Ok(result) => {
                // Format nicely: if integer result, show without decimals
                let formatted = if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
                    format!("{}", result as i64)
                } else {
                    format!("{}", result)
                };
                Ok(ToolOutput::text(formatted))
            }
            Err(e) => Err(ToolError::ExecutionFailed {
                name: "calculator".to_string(),
                message: e,
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

// --- Simple expression evaluator (recursive descent parser) ---

/// Evaluate an arithmetic expression string.
fn eval_expression(input: &str) -> Result<f64, String> {
    let tokens = tokenize(input)?;
    let mut pos = 0;
    let result = parse_expr(&tokens, &mut pos)?;
    if pos < tokens.len() {
        return Err(format!(
            "Unexpected token at position {}: {:?}",
            pos, tokens[pos]
        ));
    }
    Ok(result)
}

#[derive(Debug, Clone)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' => {
                chars.next();
            }
            '0'..='9' | '.' => {
                let mut num_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == '.' {
                        num_str.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let num: f64 = num_str
                    .parse()
                    .map_err(|_| format!("Invalid number: {}", num_str))?;
                tokens.push(Token::Number(num));
            }
            '+' => {
                tokens.push(Token::Plus);
                chars.next();
            }
            '-' => {
                // Handle unary minus
                let is_unary = tokens.is_empty()
                    || matches!(
                        tokens.last(),
                        Some(
                            Token::Plus | Token::Minus | Token::Star | Token::Slash | Token::LParen
                        )
                    );
                chars.next();
                if is_unary {
                    // Parse the number after the unary minus
                    // Skip whitespace
                    while let Some(&c) = chars.peek() {
                        if c == ' ' || c == '\t' {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if let Some(&c) = chars.peek() {
                        if c.is_ascii_digit() || c == '.' {
                            let mut num_str = String::new();
                            while let Some(&c) = chars.peek() {
                                if c.is_ascii_digit() || c == '.' {
                                    num_str.push(c);
                                    chars.next();
                                } else {
                                    break;
                                }
                            }
                            let num: f64 = num_str
                                .parse()
                                .map_err(|_| format!("Invalid number: {}", num_str))?;
                            tokens.push(Token::Number(-num));
                        } else if c == '(' {
                            // Unary minus before parenthesized expression:
                            // push -1 * (...)
                            tokens.push(Token::Number(-1.0));
                            tokens.push(Token::Star);
                        } else {
                            return Err(format!("Unexpected character after unary minus: {}", c));
                        }
                    } else {
                        return Err("Unexpected end of expression after minus".to_string());
                    }
                } else {
                    tokens.push(Token::Minus);
                }
            }
            '*' => {
                tokens.push(Token::Star);
                chars.next();
            }
            '/' => {
                tokens.push(Token::Slash);
                chars.next();
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            _ => {
                return Err(format!("Unexpected character: '{}'", ch));
            }
        }
    }

    Ok(tokens)
}

// expr = term (('+' | '-') term)*
fn parse_expr(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_term(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Plus => {
                *pos += 1;
                let right = parse_term(tokens, pos)?;
                left += right;
            }
            Token::Minus => {
                *pos += 1;
                let right = parse_term(tokens, pos)?;
                left -= right;
            }
            _ => break,
        }
    }
    Ok(left)
}

// term = factor (('*' | '/') factor)*
fn parse_term(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_factor(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Star => {
                *pos += 1;
                let right = parse_factor(tokens, pos)?;
                left *= right;
            }
            Token::Slash => {
                *pos += 1;
                let right = parse_factor(tokens, pos)?;
                if right == 0.0 {
                    return Err("Division by zero".to_string());
                }
                left /= right;
            }
            _ => break,
        }
    }
    Ok(left)
}

// factor = NUMBER | '(' expr ')'
fn parse_factor(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of expression".to_string());
    }
    match &tokens[*pos] {
        Token::Number(n) => {
            let val = *n;
            *pos += 1;
            Ok(val)
        }
        Token::LParen => {
            *pos += 1; // consume '('
            let val = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() {
                return Err("Missing closing parenthesis".to_string());
            }
            match &tokens[*pos] {
                Token::RParen => {
                    *pos += 1;
                    Ok(val)
                }
                _ => Err("Expected closing parenthesis".to_string()),
            }
        }
        other => Err(format!("Unexpected token: {:?}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- EchoTool tests ---

    #[tokio::test]
    async fn test_echo_tool_basic() {
        let tool = EchoTool;
        let result = tool
            .execute(serde_json::json!({"text": "hello world"}))
            .await
            .unwrap();
        assert_eq!(result.content, "hello world");
    }

    #[tokio::test]
    async fn test_echo_tool_empty_string() {
        let tool = EchoTool;
        let result = tool.execute(serde_json::json!({"text": ""})).await.unwrap();
        assert_eq!(result.content, "");
    }

    #[tokio::test]
    async fn test_echo_tool_missing_param() {
        let tool = EchoTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_echo_tool_properties() {
        let tool = EchoTool;
        assert_eq!(tool.name(), "echo");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(tool.parameters_schema().is_object());
    }

    // --- DateTimeTool tests ---

    #[tokio::test]
    async fn test_datetime_tool_default_format() {
        let tool = DateTimeTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        // RFC 3339 format should contain 'T' and include timezone
        assert!(result.content.contains('T'));
    }

    #[tokio::test]
    async fn test_datetime_tool_custom_format() {
        let tool = DateTimeTool;
        let result = tool
            .execute(serde_json::json!({"format": "%Y-%m-%d"}))
            .await
            .unwrap();
        // Should be in YYYY-MM-DD format
        assert_eq!(result.content.len(), 10);
        assert!(result.content.contains('-'));
    }

    #[test]
    fn test_datetime_tool_properties() {
        let tool = DateTimeTool;
        assert_eq!(tool.name(), "datetime");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    // --- CalculatorTool tests ---

    #[tokio::test]
    async fn test_calculator_simple_addition() {
        let tool = CalculatorTool;
        let result = tool
            .execute(serde_json::json!({"expression": "2 + 3"}))
            .await
            .unwrap();
        assert_eq!(result.content, "5");
    }

    #[tokio::test]
    async fn test_calculator_multiplication() {
        let tool = CalculatorTool;
        let result = tool
            .execute(serde_json::json!({"expression": "4 * 5"}))
            .await
            .unwrap();
        assert_eq!(result.content, "20");
    }

    #[tokio::test]
    async fn test_calculator_operator_precedence() {
        let tool = CalculatorTool;
        let result = tool
            .execute(serde_json::json!({"expression": "2 + 3 * 4"}))
            .await
            .unwrap();
        assert_eq!(result.content, "14");
    }

    #[tokio::test]
    async fn test_calculator_parentheses() {
        let tool = CalculatorTool;
        let result = tool
            .execute(serde_json::json!({"expression": "(2 + 3) * 4"}))
            .await
            .unwrap();
        assert_eq!(result.content, "20");
    }

    #[tokio::test]
    async fn test_calculator_nested_parentheses() {
        let tool = CalculatorTool;
        let result = tool
            .execute(serde_json::json!({"expression": "((1 + 2) * (3 + 4))"}))
            .await
            .unwrap();
        assert_eq!(result.content, "21");
    }

    #[tokio::test]
    async fn test_calculator_division() {
        let tool = CalculatorTool;
        let result = tool
            .execute(serde_json::json!({"expression": "10 / 4"}))
            .await
            .unwrap();
        assert_eq!(result.content, "2.5");
    }

    #[tokio::test]
    async fn test_calculator_division_by_zero() {
        let tool = CalculatorTool;
        let result = tool
            .execute(serde_json::json!({"expression": "5 / 0"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_calculator_negative_numbers() {
        let tool = CalculatorTool;
        let result = tool
            .execute(serde_json::json!({"expression": "-3 + 5"}))
            .await
            .unwrap();
        assert_eq!(result.content, "2");
    }

    #[tokio::test]
    async fn test_calculator_decimal_numbers() {
        let tool = CalculatorTool;
        let result = tool
            .execute(serde_json::json!({"expression": "3.5 * 2"}))
            .await
            .unwrap();
        assert_eq!(result.content, "7");
    }

    #[tokio::test]
    async fn test_calculator_missing_param() {
        let tool = CalculatorTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_calculator_invalid_expression() {
        let tool = CalculatorTool;
        let result = tool.execute(serde_json::json!({"expression": "abc"})).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_calculator_tool_properties() {
        let tool = CalculatorTool;
        assert_eq!(tool.name(), "calculator");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    // --- Expression evaluator tests ---

    #[test]
    fn test_eval_simple() {
        assert_eq!(eval_expression("1 + 1").unwrap(), 2.0);
        assert_eq!(eval_expression("10 - 3").unwrap(), 7.0);
        assert_eq!(eval_expression("6 * 7").unwrap(), 42.0);
        assert_eq!(eval_expression("15 / 3").unwrap(), 5.0);
    }

    #[test]
    fn test_eval_precedence() {
        assert_eq!(eval_expression("2 + 3 * 4").unwrap(), 14.0);
        assert_eq!(eval_expression("2 * 3 + 4").unwrap(), 10.0);
    }

    #[test]
    fn test_eval_parentheses() {
        assert_eq!(eval_expression("(2 + 3) * 4").unwrap(), 20.0);
        assert_eq!(eval_expression("2 * (3 + 4)").unwrap(), 14.0);
    }

    #[test]
    fn test_eval_unary_minus() {
        assert_eq!(eval_expression("-5").unwrap(), -5.0);
        assert_eq!(eval_expression("-5 + 10").unwrap(), 5.0);
    }

    #[test]
    fn test_eval_errors() {
        assert!(eval_expression("").is_err());
        assert!(eval_expression("1 +").is_err());
        assert!(eval_expression("(1 + 2").is_err());
        assert!(eval_expression("1 / 0").is_err());
    }
}
