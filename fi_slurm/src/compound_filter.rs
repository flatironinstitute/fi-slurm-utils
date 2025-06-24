use crate::nodes::Node;
use std::collections::VecDeque;

//  1. Token Representation 

/// Represents the individual "words" or symbols in the user's filter expression
/// The first step of parsing is to turn the raw string into a list of these tokens
#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Term(String),   // A feature name, e.g., "gpu" or "icelake"
    And,            // The "&&" or "and" operator
    Or,             // The "||" or "or" operator
    Not,            // The "!" or "not" operator
    LParen,         // A left parenthesis "("
    RParen,         // A right parenthesis ")"
}


// 2. Abstract Syntax Tree (AST) Representation 

/// Represents the logical structure of the parsed filter expression
/// This tree structure correctly captures operator precedence and grouping
#[derive(Debug, PartialEq, Clone)]
pub enum FeatureExpression {
    /// A leaf node representing a single feature term
    Term(String),

    /// A node that negates its child expression (e.g., !icelake)
    Not(Box<FeatureExpression>),

    /// A node representing a logical AND of all its children
    And(Vec<FeatureExpression>),

    /// A node representing a logical OR of all its children
    Or(Vec<FeatureExpression>),
}


// 3. Parsing Logic (To be implemented) 

/// Tokenizes a raw filter string into a sequence of `Token` enums.
fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    
    while let Some(&c) = chars.peek() {
        match c {
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            '!' => {
                tokens.push(Token::Not);
                chars.next();
            }
            '&' => {
                chars.next(); // Consume the first '&'
                if chars.peek() == Some(&'&') {
                    chars.next(); // Consume the second '&'
                    tokens.push(Token::And);
                } else {
                    return Err("Expected '&&' for AND operator, found single '&'".to_string());
                }
            }
            '|' => {
                chars.next(); // Consume the first '|'
                if chars.peek() == Some(&'|') {
                    chars.next(); // Consume the second '|'
                    tokens.push(Token::Or);
                } else {
                    return Err("Expected '||' for OR operator, found single '|'".to_string());
                }
            }
            c if c.is_whitespace() => {
                // Skip whitespace
                chars.next();
            }
            _ => {
                // Parse a Term (feature name or keyword)
                let mut term = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' || c == '-' {
                        term.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }

                match term.to_lowercase().as_str() {
                    "and" => tokens.push(Token::And),
                    "or" => tokens.push(Token::Or),
                    "not" => tokens.push(Token::Not),
                    _ => {
                        if !term.is_empty() {
                            tokens.push(Token::Term(term));
                        } else {
                           return Err(format!("Unexpected character: {}", c));
                        }
                    }
                }
            }
        }
    }
    Ok(tokens)
}


/// Parses a raw filter string into a structured `FeatureExpression` AST.
///
/// This is a placeholder for the full parsing logic. A real implementation
/// would involve tokenizing the string and then using an algorithm like
/// shunting-yard or recursive descent to build the AST.
///
/// # Arguments
///
/// * `input` - The user-provided filter string.
///
/// # Returns
///
/// A `Result` containing the root of the AST on success, or a parsing error.

pub fn parse_expression(input: &str) -> Result<FeatureExpression, String> {
    let tokens = tokenize(input)?;
    let mut parser = Parser::new(tokens);
    let ast = parser.parse_precedence(0)?;
    
    // Check for any leftover tokens, which would indicate a syntax error.
    if parser.peek() != &Token::Eof {
        return Err("Unexpected token at end of expression.".to_string());
    }

    Ok(ast)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Token {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        self.tokens[self.pos - 1].clone()
    }

    fn parse_prefix(&mut self) -> Result<FeatureExpression, String> {
        match self.advance() {
            Token::Term(s) => Ok(FeatureExpression::Term(s)),
            Token::Not => {
                let expr = self.parse_precedence(self::Precedence::Prefix as u8)?;
                Ok(FeatureExpression::Not(Box::new(expr)))
            }
            Token::LParen => {
                let expr = self.parse_precedence(0)?;
                if self.advance() != Token::RParen {
                    return Err("Expected ')' after expression".to_string());
                }
                Ok(expr)
            }
            other => Err(format!("Expected a term, '!' or '(', but found {:?}", other)),
        }
    }

    fn parse_precedence(&mut self, precedence: u8) -> Result<FeatureExpression, String> {
        let mut left = self.parse_prefix()?;

        while precedence < self.get_precedence(self.peek()) {
            let op = self.advance();
            let right = self.parse_precedence(self.get_precedence(&op))?;
            left = match op {
                Token::And => FeatureExpression::And(vec![Box::new(left), Box::new(right)]),
                Token::Or => FeatureExpression::Or(vec![Box::new(left), Box::new(right)]),
                _ => unreachable!(),
            };
        }
        Ok(left)
    }

    fn get_precedence(&self, token: &Token) -> u8 {
        match token {
            Token::Or => Precedence::Or as u8,
            Token::And => Precedence::And as u8,
            _ => 0,
        }
    }
}

enum Precedence {
    _None,
    Or,    // or, ||
    And,   // and, &&
    Prefix, // !, not
}


// 4. Evaluation Logic (To be implemented) 

/// Evaluates a parsed `FeatureExpression` AST against a single node's features
///
/// This function recursively walks the AST and returns `true` if the node's
/// features satisfy the expression
///
/// # Arguments
///
/// * `expr` - A reference to the `FeatureExpression` AST to evaluate
/// * `node` - A reference to the `Node` whose features will be checked
/// * `exact_match` - A boolean to control matching behavior
///
/// # Returns
///
/// `true` if the node matches the expression, `false` otherwise
pub fn evaluate(
    expr: &FeatureExpression,
    node: &Node,
    exact_match: bool,
) -> bool {
    match expr {
        FeatureExpression::Term(required_feat) => {
            // This is the base case of the recursion.
            // Check if any of the node's features match the term.
            if exact_match {
                node.features.contains(required_feat)
            } else {
                node.features
                    .iter()
                    .any(|actual_feat| actual_feat.contains(required_feat))
            }
        }
        FeatureExpression::Not(sub_expr) => {
            // Recursively evaluate the inner expression and return the opposite.
            !evaluate(sub_expr, node, exact_match)
        }
        FeatureExpression::And(expressions) => {
            // Recursively evaluate all children. Return `true` only if ALL are true.
            expressions
                .iter()
                .all(|sub_expr| evaluate(sub_expr, node, exact_match))
        }
        FeatureExpression::Or(expressions) => {
            // Recursively evaluate all children. Return `true` if ANY are true.
            expressions
                .iter()
                .any(|sub_expr| evaluate(sub_expr, node, exact_match))
        }
    }
}

