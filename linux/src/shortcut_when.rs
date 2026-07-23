use regex::Regex;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutContextValue {
    Bool(bool),
    String(String),
    Int(i64),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShortcutContext {
    values: HashMap<String, ShortcutContextValue>,
}

impl ShortcutContext {
    pub fn set_bool(&mut self, key: impl Into<String>, value: bool) {
        self.values
            .insert(key.into(), ShortcutContextValue::Bool(value));
    }

    pub fn set_string(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values
            .insert(key.into(), ShortcutContextValue::String(value.into()));
    }

    pub fn set_int(&mut self, key: impl Into<String>, value: i64) {
        self.values
            .insert(key.into(), ShortcutContextValue::Int(value));
    }

    pub fn bool(&self, key: &str) -> bool {
        matches!(self.values.get(key), Some(ShortcutContextValue::Bool(true)))
    }

    fn string(&self, key: &str) -> Option<&str> {
        match self.values.get(key) {
            Some(ShortcutContextValue::String(value)) => Some(value),
            _ => None,
        }
    }

    fn int(&self, key: &str) -> Option<i64> {
        match self.values.get(key) {
            Some(ShortcutContextValue::Int(value)) => Some(*value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ShortcutWhenClause {
    Always,
    Key(String),
    Compare {
        key: String,
        op: ComparisonOperator,
        operand: ContextOperand,
    },
    Not(Box<ShortcutWhenClause>),
    And(Box<ShortcutWhenClause>, Box<ShortcutWhenClause>),
    Or(Box<ShortcutWhenClause>, Box<ShortcutWhenClause>),
}

impl ShortcutWhenClause {
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.trim().is_empty() {
            return Some(Self::Always);
        }
        let mut parser = Parser::new(raw);
        let clause = parser.parse_expression()?;
        parser.is_at_end().then_some(clause)
    }

    pub fn evaluate(&self, context: &ShortcutContext) -> bool {
        match self {
            Self::Always => true,
            Self::Key(key) => context.bool(key),
            Self::Compare { key, op, operand } => evaluate_comparison(key, *op, operand, context),
            Self::Not(clause) => !clause.evaluate(context),
            Self::And(left, right) => left.evaluate(context) && right.evaluate(context),
            Self::Or(left, right) => left.evaluate(context) || right.evaluate(context),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOperator {
    Equals,
    NotEquals,
    Matches,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    InList,
}

#[derive(Debug, Clone)]
pub enum ContextOperand {
    String(String),
    Int(i64),
    Regex(Regex),
    List(Vec<ContextOperand>),
}

fn evaluate_comparison(
    key: &str,
    op: ComparisonOperator,
    operand: &ContextOperand,
    context: &ShortcutContext,
) -> bool {
    match op {
        ComparisonOperator::Equals => comparison_equals(key, operand, context),
        ComparisonOperator::NotEquals => !comparison_equals(key, operand, context),
        ComparisonOperator::Matches => match operand {
            ContextOperand::Regex(regex) => context
                .string(key)
                .is_some_and(|value| regex.is_match(value)),
            _ => false,
        },
        ComparisonOperator::LessThan
        | ComparisonOperator::LessThanOrEqual
        | ComparisonOperator::GreaterThan
        | ComparisonOperator::GreaterThanOrEqual => {
            let (Some(left), ContextOperand::Int(right)) = (context.int(key), operand) else {
                return false;
            };
            match op {
                ComparisonOperator::LessThan => left < *right,
                ComparisonOperator::LessThanOrEqual => left <= *right,
                ComparisonOperator::GreaterThan => left > *right,
                ComparisonOperator::GreaterThanOrEqual => left >= *right,
                _ => false,
            }
        }
        ComparisonOperator::InList => match operand {
            ContextOperand::List(items) => items
                .iter()
                .any(|item| comparison_equals(key, item, context)),
            _ => false,
        },
    }
}

fn comparison_equals(key: &str, operand: &ContextOperand, context: &ShortcutContext) -> bool {
    match operand {
        ContextOperand::String(expected) => context.string(key) == Some(expected),
        ContextOperand::Int(expected) => context.int(key) == Some(*expected),
        ContextOperand::Regex(_) | ContextOperand::List(_) => false,
    }
}

#[derive(Debug, Clone)]
enum Token {
    Not,
    And,
    Or,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,
    Equals,
    NotEquals,
    Matches,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Identifier(String),
    Number(i64),
    String(String),
    Regex(Regex),
    Invalid,
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        use Token::*;
        match (self, other) {
            (Not, Not)
            | (And, And)
            | (Or, Or)
            | (LeftParen, LeftParen)
            | (RightParen, RightParen)
            | (LeftBracket, LeftBracket)
            | (RightBracket, RightBracket)
            | (Comma, Comma)
            | (Equals, Equals)
            | (NotEquals, NotEquals)
            | (Matches, Matches)
            | (LessThan, LessThan)
            | (LessThanOrEqual, LessThanOrEqual)
            | (GreaterThan, GreaterThan)
            | (GreaterThanOrEqual, GreaterThanOrEqual)
            | (Invalid, Invalid) => true,
            (Identifier(left), Identifier(right)) | (String(left), String(right)) => left == right,
            (Number(left), Number(right)) => left == right,
            (Regex(left), Regex(right)) => left.as_str() == right.as_str(),
            _ => false,
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn new(raw: &str) -> Self {
        Self {
            tokens: tokenize(raw),
            index: 0,
        }
    }

    fn is_at_end(&self) -> bool {
        self.index >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index)?.clone();
        self.index += 1;
        Some(token)
    }

    fn parse_expression(&mut self) -> Option<ShortcutWhenClause> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Option<ShortcutWhenClause> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Token::Or) {
            self.advance();
            left = ShortcutWhenClause::Or(Box::new(left), Box::new(self.parse_and()?));
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<ShortcutWhenClause> {
        let mut left = self.parse_comparison()?;
        while self.peek() == Some(&Token::And) {
            self.advance();
            left = ShortcutWhenClause::And(Box::new(left), Box::new(self.parse_comparison()?));
        }
        Some(left)
    }

    fn parse_comparison(&mut self) -> Option<ShortcutWhenClause> {
        let left = self.parse_unary()?;
        let Some(op) = comparison_operator(self.peek()) else {
            return Some(left);
        };
        let ShortcutWhenClause::Key(key) = &left else {
            return None;
        };
        let key = key.clone();
        self.advance();
        let operand = self.parse_operand(op)?;
        if matches!(
            op,
            ComparisonOperator::Equals | ComparisonOperator::NotEquals
        ) {
            if let ContextOperand::String(value) = &operand {
                if matches!(value.as_str(), "true" | "false") {
                    let wants_true = (value == "true") == (op == ComparisonOperator::Equals);
                    return Some(if wants_true {
                        left
                    } else {
                        ShortcutWhenClause::Not(Box::new(left))
                    });
                }
            }
        }
        Some(ShortcutWhenClause::Compare { key, op, operand })
    }

    fn parse_unary(&mut self) -> Option<ShortcutWhenClause> {
        if self.peek() == Some(&Token::Not) {
            self.advance();
            return Some(ShortcutWhenClause::Not(Box::new(self.parse_unary()?)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<ShortcutWhenClause> {
        match self.advance()? {
            Token::LeftParen => {
                let inner = self.parse_expression()?;
                if self.advance()? != Token::RightParen {
                    return None;
                }
                Some(inner)
            }
            Token::Identifier(name) if name == "true" => Some(ShortcutWhenClause::Always),
            Token::Identifier(name) if name == "false" => Some(ShortcutWhenClause::Not(Box::new(
                ShortcutWhenClause::Always,
            ))),
            Token::Identifier(name) => Some(ShortcutWhenClause::Key(name)),
            _ => None,
        }
    }

    fn parse_operand(&mut self, op: ComparisonOperator) -> Option<ContextOperand> {
        match op {
            ComparisonOperator::Matches => match self.advance()? {
                Token::Regex(regex) => Some(ContextOperand::Regex(regex)),
                _ => None,
            },
            ComparisonOperator::LessThan
            | ComparisonOperator::LessThanOrEqual
            | ComparisonOperator::GreaterThan
            | ComparisonOperator::GreaterThanOrEqual => match self.advance()? {
                Token::Number(value) => Some(ContextOperand::Int(value)),
                _ => None,
            },
            ComparisonOperator::InList => self.parse_list(),
            ComparisonOperator::Equals | ComparisonOperator::NotEquals => match self.advance()? {
                Token::Number(value) => Some(ContextOperand::Int(value)),
                Token::String(value) | Token::Identifier(value) => {
                    Some(ContextOperand::String(value))
                }
                _ => None,
            },
        }
    }

    fn parse_list(&mut self) -> Option<ContextOperand> {
        if self.advance()? != Token::LeftBracket {
            return None;
        }
        let mut values = Vec::new();
        if self.peek() == Some(&Token::RightBracket) {
            self.advance();
            return Some(ContextOperand::List(values));
        }
        loop {
            values.push(match self.advance()? {
                Token::Number(value) => ContextOperand::Int(value),
                Token::String(value) | Token::Identifier(value) => ContextOperand::String(value),
                _ => return None,
            });
            match self.advance()? {
                Token::Comma => {}
                Token::RightBracket => return Some(ContextOperand::List(values)),
                _ => return None,
            }
        }
    }
}

fn comparison_operator(token: Option<&Token>) -> Option<ComparisonOperator> {
    match token {
        Some(Token::Equals) => Some(ComparisonOperator::Equals),
        Some(Token::NotEquals) => Some(ComparisonOperator::NotEquals),
        Some(Token::Matches) => Some(ComparisonOperator::Matches),
        Some(Token::LessThan) => Some(ComparisonOperator::LessThan),
        Some(Token::LessThanOrEqual) => Some(ComparisonOperator::LessThanOrEqual),
        Some(Token::GreaterThan) => Some(ComparisonOperator::GreaterThan),
        Some(Token::GreaterThanOrEqual) => Some(ComparisonOperator::GreaterThanOrEqual),
        Some(Token::Identifier(value)) if value == "in" => Some(ComparisonOperator::InList),
        _ => None,
    }
}

fn tokenize(raw: &str) -> Vec<Token> {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            ch if ch.is_whitespace() => index += 1,
            '(' => {
                tokens.push(Token::LeftParen);
                index += 1;
            }
            ')' => {
                tokens.push(Token::RightParen);
                index += 1;
            }
            '[' => {
                tokens.push(Token::LeftBracket);
                index += 1;
            }
            ']' => {
                tokens.push(Token::RightBracket);
                index += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                index += 1;
            }
            '!' => {
                index += 1;
                if chars.get(index) == Some(&'=') {
                    index += 1;
                    tokens.push(Token::NotEquals);
                } else {
                    tokens.push(Token::Not);
                }
            }
            '=' => {
                index += 1;
                if chars.get(index) == Some(&'=') {
                    index += 1;
                    tokens.push(Token::Equals);
                } else if chars.get(index) == Some(&'~') {
                    index += 1;
                    tokens.push(Token::Matches);
                } else {
                    tokens.push(Token::Invalid);
                }
            }
            '<' => {
                index += 1;
                if chars.get(index) == Some(&'=') {
                    index += 1;
                    tokens.push(Token::LessThanOrEqual);
                } else {
                    tokens.push(Token::LessThan);
                }
            }
            '>' => {
                index += 1;
                if chars.get(index) == Some(&'=') {
                    index += 1;
                    tokens.push(Token::GreaterThanOrEqual);
                } else {
                    tokens.push(Token::GreaterThan);
                }
            }
            '&' => {
                index += 1;
                if chars.get(index) == Some(&'&') {
                    index += 1;
                }
                tokens.push(Token::And);
            }
            '|' => {
                index += 1;
                if chars.get(index) == Some(&'|') {
                    index += 1;
                }
                tokens.push(Token::Or);
            }
            '\'' => {
                index += 1;
                let start = index;
                while index < chars.len() && chars[index] != '\'' {
                    index += 1;
                }
                if index < chars.len() {
                    tokens.push(Token::String(chars[start..index].iter().collect()));
                    index += 1;
                } else {
                    tokens.push(Token::Invalid);
                }
            }
            '/' => {
                index += 1;
                let mut pattern = String::new();
                let mut terminated = false;
                while index < chars.len() {
                    if chars[index] == '\\' && chars.get(index + 1) == Some(&'/') {
                        pattern.push('/');
                        index += 2;
                    } else if chars[index] == '/' {
                        terminated = true;
                        index += 1;
                        break;
                    } else {
                        pattern.push(chars[index]);
                        index += 1;
                    }
                }
                if terminated {
                    tokens.push(Regex::new(&pattern).map_or(Token::Invalid, Token::Regex));
                } else {
                    tokens.push(Token::Invalid);
                }
            }
            ch if ch.is_ascii_digit() => {
                let start = index;
                while chars.get(index).is_some_and(|ch| ch.is_ascii_digit()) {
                    index += 1;
                }
                let value = chars[start..index].iter().collect::<String>();
                tokens.push(value.parse::<i64>().map_or(Token::Invalid, Token::Number));
            }
            ch if ch.is_alphanumeric() || ch == '_' => {
                let start = index;
                while chars
                    .get(index)
                    .is_some_and(|ch| ch.is_alphanumeric() || *ch == '_')
                {
                    index += 1;
                }
                tokens.push(Token::Identifier(chars[start..index].iter().collect()));
            }
            _ => {
                tokens.push(Token::Invalid);
                index += 1;
            }
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ShortcutContext {
        let mut context = ShortcutContext::default();
        context.set_bool("sidebarFocus", false);
        context.set_bool("browserFocus", true);
        context.set_bool("markdownFocus", false);
        context.set_bool("terminalFocus", false);
        context.set_bool("commandPaletteVisible", true);
        context.set_bool("terminalFindVisible", false);
        context.set_string("sidebarMode", "find");
        context.set_int("paneCount", 2);
        context.set_int("workspaceCount", 4);
        context
    }

    fn evaluate(raw: &str) -> bool {
        ShortcutWhenClause::parse(raw)
            .unwrap_or_else(|| panic!("failed to parse {raw:?}"))
            .evaluate(&context())
    }

    #[test]
    fn parses_boolean_precedence_parentheses_and_literals() {
        assert!(evaluate("browserFocus && !sidebarFocus"));
        assert!(evaluate(
            "terminalFocus || browserFocus && markdownFocus || commandPaletteVisible"
        ));
        assert!(!evaluate(
            "(terminalFocus || browserFocus) && markdownFocus"
        ));
        assert!(evaluate("true"));
        assert!(!evaluate("false"));
        assert!(evaluate("sidebarFocus == false"));
        assert!(evaluate("browserFocus != false"));
        assert!(!evaluate("unknownKey"));
        assert!(evaluate("!unknownKey"));
    }

    #[test]
    fn evaluates_typed_comparisons_regex_and_lists() {
        assert!(evaluate("paneCount > 1 && workspaceCount <= 4"));
        assert!(evaluate("sidebarMode == 'find'"));
        assert!(evaluate("sidebarMode != files"));
        assert!(evaluate("sidebarMode =~ /fi.*/"));
        assert!(evaluate("sidebarMode in [files, 'find', dock]"));
        assert!(evaluate("paneCount in [1, 2, 3]"));
        assert!(!evaluate("paneCount == '2'"));
        assert!(evaluate("missing != value"));
    }

    #[test]
    fn rejects_malformed_clauses() {
        for raw in [
            "sidebarFocus &&",
            "(sidebarFocus",
            "!",
            "paneCount >",
            "paneCount > nope",
            "sidebarMode = find",
            "sidebarMode =~ /[/",
            "sidebarMode in [files,]",
        ] {
            assert!(ShortcutWhenClause::parse(raw).is_none(), "parsed {raw:?}");
        }
        assert!(ShortcutWhenClause::parse("  ").is_some());
    }
}
