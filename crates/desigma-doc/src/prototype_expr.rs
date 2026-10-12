//! A tiny, safe expression language over variables, used by the prototype
//! actions "Set variable" and "Conditional" (Figma's variable expressions).
//!
//! Expressions are stored as an AST ([`Expression`]) and shown/edited as text:
//!
//! ```text
//! {Cart/Count} + 1          arithmetic       + - * / %
//! $count >= 3 && !$done     comparison       == != < <= > >=   logic  && || !
//! "Items: " + $count        string concatenation (either side a string)
//! $on ? "Yes" : "No"        conditional value
//! #FF8800                   colour literal
//! ```
//!
//! Variable references are `$name` (letters, digits and `_`) or `{any name}`.
//! The text form is resolved against a name lookup; the AST references
//! variables by id so renaming never breaks stored expressions.
//!
//! There is no `eval` of code: this is a pure interpreter over a closed
//! grammar, with limits on source length, nesting depth, node count and
//! string size, so a hostile file cannot make Present hang or blow the stack.

use crate::{Color, VariableKind, VariableValue};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Longest expression source the parser accepts, in bytes.
pub const MAX_SOURCE_LEN: usize = 1024;
/// Deepest nesting of an expression tree (parser and evaluator).
pub const MAX_DEPTH: usize = 64;
/// Most nodes an expression tree may have.
pub const MAX_NODES: usize = 256;
/// Longest string an evaluation may produce, in bytes.
pub const MAX_STRING_LEN: usize = 4096;

/// A runtime value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExprValue {
    Number(f64),
    String(String),
    Boolean(bool),
    Color(Color),
}

/// The static type of an expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExprType {
    Number,
    String,
    Boolean,
    Color,
}

impl ExprType {
    pub fn name(self) -> &'static str {
        match self {
            ExprType::Number => "number",
            ExprType::String => "text",
            ExprType::Boolean => "boolean",
            ExprType::Color => "color",
        }
    }
}

impl From<VariableKind> for ExprType {
    fn from(k: VariableKind) -> Self {
        match k {
            VariableKind::Number => ExprType::Number,
            VariableKind::String => ExprType::String,
            VariableKind::Boolean => ExprType::Boolean,
            VariableKind::Color => ExprType::Color,
        }
    }
}

impl ExprValue {
    pub fn ty(&self) -> ExprType {
        match self {
            ExprValue::Number(_) => ExprType::Number,
            ExprValue::String(_) => ExprType::String,
            ExprValue::Boolean(_) => ExprType::Boolean,
            ExprValue::Color(_) => ExprType::Color,
        }
    }

    /// The value as a variable value of `kind`, converting numbers and
    /// booleans to text for string variables. `None` for other mismatches.
    pub fn to_variable_value(&self, kind: VariableKind) -> Option<VariableValue> {
        Some(match (kind, self) {
            (VariableKind::Number, ExprValue::Number(n)) => VariableValue::Number(*n as f32),
            (VariableKind::Boolean, ExprValue::Boolean(b)) => VariableValue::Boolean(*b),
            (VariableKind::Color, ExprValue::Color(c)) => VariableValue::Color(*c),
            (VariableKind::String, v) => VariableValue::String(v.display()),
            _ => return None,
        })
    }

    /// Text form used by string concatenation.
    pub fn display(&self) -> String {
        match self {
            ExprValue::Number(n) => format_number(*n),
            ExprValue::String(s) => s.clone(),
            ExprValue::Boolean(b) => b.to_string(),
            ExprValue::Color(c) => format!("#{}", c.to_hex()),
        }
    }
}

impl From<&VariableValue> for ExprValue {
    fn from(v: &VariableValue) -> Self {
        match v {
            VariableValue::Number(n) => ExprValue::Number(*n as f64),
            VariableValue::String(s) => ExprValue::String(s.clone()),
            VariableValue::Boolean(b) => ExprValue::Boolean(*b),
            VariableValue::Color(c) => ExprValue::Color(*c),
            // aliases are resolved by the caller before they get here
            VariableValue::Alias(id) => ExprValue::String(id.clone()),
        }
    }
}

/// `3` not `3.0`; at most 6 decimals.
pub fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    let s = format!("{n:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Rem => "%",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        }
    }

    /// Binding strength for printing (higher binds tighter).
    fn prec(self) -> u8 {
        match self {
            BinaryOp::Or => 2,
            BinaryOp::And => 3,
            BinaryOp::Eq | BinaryOp::Ne => 4,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 5,
            BinaryOp::Add | BinaryOp::Sub => 6,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => 7,
        }
    }
}

/// An expression over variables (referenced by variable id).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    Literal(ExprValue),
    /// A variable, by id.
    Variable(String),
    Unary {
        op: UnaryOp,
        arg: Box<Expression>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    Ternary {
        cond: Box<Expression>,
        then: Box<Expression>,
        otherwise: Box<Expression>,
    },
}

impl Expression {
    pub fn number(n: f64) -> Self {
        Expression::Literal(ExprValue::Number(n))
    }
    pub fn text(s: impl Into<String>) -> Self {
        Expression::Literal(ExprValue::String(s.into()))
    }
    pub fn boolean(b: bool) -> Self {
        Expression::Literal(ExprValue::Boolean(b))
    }
    pub fn var(id: impl Into<String>) -> Self {
        Expression::Variable(id.into())
    }
    pub fn binary(op: BinaryOp, lhs: Expression, rhs: Expression) -> Self {
        Expression::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }
    }
    pub fn logical_not(arg: Expression) -> Self {
        Expression::Unary { op: UnaryOp::Not, arg: Box::new(arg) }
    }

    /// Number of nodes in the tree.
    pub fn node_count(&self) -> usize {
        match self {
            Expression::Literal(_) | Expression::Variable(_) => 1,
            Expression::Unary { arg, .. } => 1 + arg.node_count(),
            Expression::Binary { lhs, rhs, .. } => 1 + lhs.node_count() + rhs.node_count(),
            Expression::Ternary { cond, then, otherwise } => 1 + cond.node_count() + then.node_count() + otherwise.node_count(),
        }
    }

    /// Depth of the tree (a leaf is 1).
    pub fn depth(&self) -> usize {
        match self {
            Expression::Literal(_) | Expression::Variable(_) => 1,
            Expression::Unary { arg, .. } => 1 + arg.depth(),
            Expression::Binary { lhs, rhs, .. } => 1 + lhs.depth().max(rhs.depth()),
            Expression::Ternary { cond, then, otherwise } => 1 + cond.depth().max(then.depth()).max(otherwise.depth()),
        }
    }

    /// Whether the tree is within [`MAX_DEPTH`] and [`MAX_NODES`]. Trees from
    /// untrusted files are checked before they are evaluated; the check walks
    /// iteratively so a deeply nested tree cannot overflow the stack.
    pub fn within_limits(&self) -> bool {
        let mut stack = vec![(self, 1usize)];
        let mut nodes = 0usize;
        while let Some((e, d)) = stack.pop() {
            nodes += 1;
            if d > MAX_DEPTH || nodes > MAX_NODES {
                return false;
            }
            match e {
                Expression::Literal(_) | Expression::Variable(_) => {}
                Expression::Unary { arg, .. } => stack.push((arg, d + 1)),
                Expression::Binary { lhs, rhs, .. } => {
                    stack.push((lhs, d + 1));
                    stack.push((rhs, d + 1));
                }
                Expression::Ternary { cond, then, otherwise } => {
                    stack.push((cond, d + 1));
                    stack.push((then, d + 1));
                    stack.push((otherwise, d + 1));
                }
            }
        }
        true
    }

    /// Ids of the variables the expression reads (each once, first use first).
    pub fn variables(&self) -> Vec<String> {
        fn rec(e: &Expression, out: &mut Vec<String>) {
            match e {
                Expression::Literal(_) => {}
                Expression::Variable(v) => {
                    if !out.contains(v) {
                        out.push(v.clone());
                    }
                }
                Expression::Unary { arg, .. } => rec(arg, out),
                Expression::Binary { lhs, rhs, .. } => {
                    rec(lhs, out);
                    rec(rhs, out);
                }
                Expression::Ternary { cond, then, otherwise } => {
                    rec(cond, out);
                    rec(then, out);
                    rec(otherwise, out);
                }
            }
        }
        let mut out = Vec::new();
        if self.within_limits() {
            rec(self, &mut out);
        }
        out
    }

    /// Static type of the expression; `kind_of` gives the type of a variable
    /// (`None` = unknown variable).
    pub fn type_of(&self, kind_of: &dyn Fn(&str) -> Option<VariableKind>) -> Result<ExprType, String> {
        if !self.within_limits() {
            return Err("Expression is too complex".into());
        }
        self.ty_rec(kind_of)
    }

    fn ty_rec(&self, kind_of: &dyn Fn(&str) -> Option<VariableKind>) -> Result<ExprType, String> {
        use ExprType::*;
        Ok(match self {
            Expression::Literal(v) => v.ty(),
            Expression::Variable(id) => kind_of(id).map(ExprType::from).ok_or_else(|| format!("Unknown variable {id}"))?,
            Expression::Unary { op: UnaryOp::Not, arg } => match arg.ty_rec(kind_of)? {
                Boolean => Boolean,
                t => return Err(format!("'!' needs a boolean, not {}", t.name())),
            },
            Expression::Unary { op: UnaryOp::Neg, arg } => match arg.ty_rec(kind_of)? {
                Number => Number,
                t => return Err(format!("'-' needs a number, not {}", t.name())),
            },
            Expression::Binary { op, lhs, rhs } => {
                let (a, b) = (lhs.ty_rec(kind_of)?, rhs.ty_rec(kind_of)?);
                match op {
                    BinaryOp::Add => match (a, b) {
                        (Number, Number) => Number,
                        (String, _) | (_, String) => String,
                        _ => return Err(format!("Cannot add {} and {}", a.name(), b.name())),
                    },
                    BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => match (a, b) {
                        (Number, Number) => Number,
                        _ => return Err(format!("'{}' needs numbers, not {} and {}", op.symbol(), a.name(), b.name())),
                    },
                    BinaryOp::Eq | BinaryOp::Ne => Boolean,
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => match (a, b) {
                        (Number, Number) | (String, String) => Boolean,
                        _ => return Err(format!("Cannot compare {} with {}", a.name(), b.name())),
                    },
                    BinaryOp::And | BinaryOp::Or => match (a, b) {
                        (Boolean, Boolean) => Boolean,
                        _ => return Err(format!("'{}' needs booleans, not {} and {}", op.symbol(), a.name(), b.name())),
                    },
                }
            }
            Expression::Ternary { cond, then, otherwise } => {
                if cond.ty_rec(kind_of)? != Boolean {
                    return Err("The condition before '?' must be a boolean".into());
                }
                let (a, b) = (then.ty_rec(kind_of)?, otherwise.ty_rec(kind_of)?);
                if a != b {
                    return Err(format!("Both branches must have the same type, not {} and {}", a.name(), b.name()));
                }
                a
            }
        })
    }

    /// Evaluates against `env` (variable id -> current value).
    pub fn eval(&self, env: &dyn Fn(&str) -> Option<ExprValue>) -> Result<ExprValue, EvalError> {
        if !self.within_limits() {
            return Err(EvalError::TooComplex);
        }
        self.eval_rec(env)
    }

    fn eval_rec(&self, env: &dyn Fn(&str) -> Option<ExprValue>) -> Result<ExprValue, EvalError> {
        use ExprValue as V;
        Ok(match self {
            Expression::Literal(v) => v.clone(),
            Expression::Variable(id) => env(id).ok_or_else(|| EvalError::UnknownVariable(id.clone()))?,
            Expression::Unary { op, arg } => match (op, arg.eval_rec(env)?) {
                (UnaryOp::Not, V::Boolean(b)) => V::Boolean(!b),
                (UnaryOp::Neg, V::Number(n)) => V::Number(-n),
                (op, v) => return Err(EvalError::Type(format!("{op:?} does not apply to {}", v.ty().name()))),
            },
            Expression::Binary { op: BinaryOp::And, lhs, rhs } => match lhs.eval_rec(env)? {
                V::Boolean(false) => V::Boolean(false),
                V::Boolean(true) => match rhs.eval_rec(env)? {
                    b @ V::Boolean(_) => b,
                    v => return Err(EvalError::Type(format!("'&&' needs booleans, not {}", v.ty().name()))),
                },
                v => return Err(EvalError::Type(format!("'&&' needs booleans, not {}", v.ty().name()))),
            },
            Expression::Binary { op: BinaryOp::Or, lhs, rhs } => match lhs.eval_rec(env)? {
                V::Boolean(true) => V::Boolean(true),
                V::Boolean(false) => match rhs.eval_rec(env)? {
                    b @ V::Boolean(_) => b,
                    v => return Err(EvalError::Type(format!("'||' needs booleans, not {}", v.ty().name()))),
                },
                v => return Err(EvalError::Type(format!("'||' needs booleans, not {}", v.ty().name()))),
            },
            Expression::Binary { op, lhs, rhs } => binary(*op, lhs.eval_rec(env)?, rhs.eval_rec(env)?)?,
            Expression::Ternary { cond, then, otherwise } => match cond.eval_rec(env)? {
                V::Boolean(true) => then.eval_rec(env)?,
                V::Boolean(false) => otherwise.eval_rec(env)?,
                v => return Err(EvalError::Type(format!("Condition must be a boolean, not {}", v.ty().name()))),
            },
        })
    }

    /// Evaluates to a boolean (for conditions); anything else is a type error.
    pub fn eval_bool(&self, env: &dyn Fn(&str) -> Option<ExprValue>) -> Result<bool, EvalError> {
        match self.eval(env)? {
            ExprValue::Boolean(b) => Ok(b),
            v => Err(EvalError::Type(format!("Condition must be a boolean, not {}", v.ty().name()))),
        }
    }
}

fn binary(op: BinaryOp, a: ExprValue, b: ExprValue) -> Result<ExprValue, EvalError> {
    use ExprValue as V;
    let ty_err = |a: &V, b: &V| EvalError::Type(format!("'{}' does not apply to {} and {}", op.symbol(), a.ty().name(), b.ty().name()));
    let num = |n: f64| if n.is_finite() { Ok(V::Number(n)) } else { Err(EvalError::NotFinite) };
    Ok(match op {
        BinaryOp::Add => match (&a, &b) {
            (V::Number(x), V::Number(y)) => num(x + y)?,
            (V::String(_), _) | (_, V::String(_)) => {
                let s = format!("{}{}", a.display(), b.display());
                if s.len() > MAX_STRING_LEN {
                    return Err(EvalError::TooComplex);
                }
                V::String(s)
            }
            _ => return Err(ty_err(&a, &b)),
        },
        BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => match (&a, &b) {
            (V::Number(x), V::Number(y)) => match op {
                BinaryOp::Sub => num(x - y)?,
                BinaryOp::Mul => num(x * y)?,
                _ if *y == 0.0 => return Err(EvalError::DivideByZero),
                BinaryOp::Div => num(x / y)?,
                _ => num(x % y)?,
            },
            _ => return Err(ty_err(&a, &b)),
        },
        BinaryOp::Eq => V::Boolean(values_equal(&a, &b)),
        BinaryOp::Ne => V::Boolean(!values_equal(&a, &b)),
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            let ord = match (&a, &b) {
                (V::Number(x), V::Number(y)) => x.partial_cmp(y),
                (V::String(x), V::String(y)) => Some(x.cmp(y)),
                _ => return Err(ty_err(&a, &b)),
            };
            let Some(ord) = ord else { return Ok(V::Boolean(false)) };
            V::Boolean(match op {
                BinaryOp::Lt => ord.is_lt(),
                BinaryOp::Le => ord.is_le(),
                BinaryOp::Gt => ord.is_gt(),
                _ => ord.is_ge(),
            })
        }
        BinaryOp::And | BinaryOp::Or => unreachable!("short-circuit operators are handled by the caller"),
    })
}

fn values_equal(a: &ExprValue, b: &ExprValue) -> bool {
    match (a, b) {
        (ExprValue::Number(x), ExprValue::Number(y)) => (x - y).abs() < 1e-9,
        (ExprValue::Color(x), ExprValue::Color(y)) => x.to_hex() == y.to_hex(),
        _ => a == b,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EvalError {
    UnknownVariable(String),
    Type(String),
    DivideByZero,
    NotFinite,
    TooComplex,
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::UnknownVariable(v) => write!(f, "Unknown variable {v}"),
            EvalError::Type(m) => f.write_str(m),
            EvalError::DivideByZero => f.write_str("Division by zero"),
            EvalError::NotFinite => f.write_str("Result is not a finite number"),
            EvalError::TooComplex => f.write_str("Expression is too complex"),
        }
    }
}

impl std::error::Error for EvalError {}

// -------------------------------------------------------------------- parser

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    /// Byte offset into the source.
    pub pos: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at {})", self.message, self.pos)
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Var(String),
    Color(Color),
    True,
    False,
    Op(&'static str),
}

struct Lexer<'a> {
    s: &'a str,
    pos: usize,
}

fn err<T>(pos: usize, m: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError { pos, message: m.into() })
}

const OPS: [&str; 19] = ["==", "!=", "<=", ">=", "&&", "||", "+", "-", "*", "/", "%", "<", ">", "!", "?", ":", "(", ")", ","];

impl Lexer<'_> {
    fn tokens(mut self) -> Result<Vec<(usize, Tok)>, ParseError> {
        let mut out = Vec::new();
        loop {
            let rest = &self.s[self.pos..];
            let trimmed = rest.trim_start();
            self.pos += rest.len() - trimmed.len();
            let Some(c) = trimmed.chars().next() else { return Ok(out) };
            let start = self.pos;
            if out.len() >= MAX_NODES * 2 {
                return err(start, "Expression is too long");
            }
            if c.is_ascii_digit() || (c == '.' && trimmed[1..].starts_with(|d: char| d.is_ascii_digit())) {
                let end = trimmed.find(|d: char| !(d.is_ascii_digit() || d == '.')).unwrap_or(trimmed.len());
                let n: f64 = trimmed[..end].parse().map_err(|_| ParseError { pos: start, message: format!("Bad number '{}'", &trimmed[..end]) })?;
                self.pos += end;
                out.push((start, Tok::Num(n)));
            } else if c == '"' || c == '\'' {
                let mut s = String::new();
                let mut it = trimmed[1..].char_indices();
                let mut closed = None;
                while let Some((i, ch)) = it.next() {
                    match ch {
                        '\\' => match it.next() {
                            Some((_, 'n')) => s.push('\n'),
                            Some((_, 't')) => s.push('\t'),
                            Some((_, e)) => s.push(e),
                            None => return err(start, "Unterminated string"),
                        },
                        ch if ch == c => {
                            closed = Some(i + 2);
                            break;
                        }
                        ch => s.push(ch),
                    }
                }
                let Some(len) = closed else { return err(start, "Unterminated string") };
                self.pos += len;
                out.push((start, Tok::Str(s)));
            } else if c == '$' {
                let body = &trimmed[1..];
                let end = body.find(|d: char| !(d.is_alphanumeric() || d == '_')).unwrap_or(body.len());
                if end == 0 {
                    return err(start, "Expected a variable name after '$'");
                }
                out.push((start, Tok::Var(body[..end].to_string())));
                self.pos += 1 + end;
            } else if c == '{' {
                let Some(end) = trimmed.find('}') else { return err(start, "Missing '}'") };
                let name = trimmed[1..end].trim();
                if name.is_empty() {
                    return err(start, "Empty variable name");
                }
                out.push((start, Tok::Var(name.to_string())));
                self.pos += end + 1;
            } else if c == '#' {
                let body = &trimmed[1..];
                let end = body.find(|d: char| !d.is_ascii_hexdigit()).unwrap_or(body.len());
                let Some(col) = Color::from_hex(&body[..end]) else { return err(start, format!("Bad color '#{}'", &body[..end])) };
                out.push((start, Tok::Color(col)));
                self.pos += 1 + end;
            } else if c.is_alphabetic() {
                let end = trimmed.find(|d: char| !(d.is_alphanumeric() || d == '_')).unwrap_or(trimmed.len());
                let word = &trimmed[..end];
                match word {
                    "true" => out.push((start, Tok::True)),
                    "false" => out.push((start, Tok::False)),
                    _ => return err(start, format!("Unknown word '{word}'; write variables as $name or {{name}}")),
                }
                self.pos += end;
            } else if let Some(op) = OPS.iter().find(|o| trimmed.starts_with(**o)) {
                out.push((start, Tok::Op(op)));
                self.pos += op.len();
            } else {
                return err(start, format!("Unexpected '{c}'"));
            }
        }
    }
}

struct Parser<'a> {
    toks: Vec<(usize, Tok)>,
    i: usize,
    end: usize,
    resolve: &'a dyn Fn(&str) -> Option<String>,
    nodes: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|t| &t.1)
    }
    fn pos(&self) -> usize {
        self.toks.get(self.i).map(|t| t.0).unwrap_or(self.end)
    }
    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn count(&mut self) -> Result<(), ParseError> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return err(self.pos(), "Expression is too long");
        }
        Ok(())
    }

    fn expr(&mut self, depth: usize) -> Result<Expression, ParseError> {
        if depth > MAX_DEPTH {
            return err(self.pos(), "Expression is nested too deeply");
        }
        let cond = self.binary(1, depth)?;
        if self.eat("?") {
            let then = self.expr(depth + 1)?;
            if !self.eat(":") {
                return err(self.pos(), "Expected ':' in the '?:' expression");
            }
            let otherwise = self.expr(depth + 1)?;
            self.count()?;
            return Ok(Expression::Ternary { cond: Box::new(cond), then: Box::new(then), otherwise: Box::new(otherwise) });
        }
        Ok(cond)
    }

    fn op_at(&self, min: u8) -> Option<BinaryOp> {
        let Some(Tok::Op(o)) = self.peek() else { return None };
        let op = match *o {
            "||" => BinaryOp::Or,
            "&&" => BinaryOp::And,
            "==" => BinaryOp::Eq,
            "!=" => BinaryOp::Ne,
            "<" => BinaryOp::Lt,
            "<=" => BinaryOp::Le,
            ">" => BinaryOp::Gt,
            ">=" => BinaryOp::Ge,
            "+" => BinaryOp::Add,
            "-" => BinaryOp::Sub,
            "*" => BinaryOp::Mul,
            "/" => BinaryOp::Div,
            "%" => BinaryOp::Rem,
            _ => return None,
        };
        (op.prec() >= min).then_some(op)
    }

    /// Precedence climbing; all binary operators are left associative.
    fn binary(&mut self, min: u8, depth: usize) -> Result<Expression, ParseError> {
        if depth > MAX_DEPTH {
            return err(self.pos(), "Expression is nested too deeply");
        }
        let mut lhs = self.unary(depth)?;
        while let Some(op) = self.op_at(min) {
            self.i += 1;
            let rhs = self.binary(op.prec() + 1, depth)?;
            self.count()?;
            lhs = Expression::binary(op, lhs, rhs);
            if lhs.depth() > MAX_DEPTH {
                return err(self.pos(), "Expression is nested too deeply");
            }
        }
        Ok(lhs)
    }

    fn unary(&mut self, depth: usize) -> Result<Expression, ParseError> {
        if depth > MAX_DEPTH {
            return err(self.pos(), "Expression is nested too deeply");
        }
        if self.eat("!") {
            let arg = self.unary(depth + 1)?;
            self.count()?;
            return Ok(Expression::Unary { op: UnaryOp::Not, arg: Box::new(arg) });
        }
        if self.eat("-") {
            let arg = self.unary(depth + 1)?;
            self.count()?;
            return Ok(Expression::Unary { op: UnaryOp::Neg, arg: Box::new(arg) });
        }
        self.primary(depth)
    }

    fn primary(&mut self, depth: usize) -> Result<Expression, ParseError> {
        let pos = self.pos();
        let Some(tok) = self.toks.get(self.i).map(|t| t.1.clone()) else { return err(pos, "Unexpected end of expression") };
        self.i += 1;
        self.count()?;
        Ok(match tok {
            Tok::Num(n) => Expression::number(n),
            Tok::Str(s) => Expression::text(s),
            Tok::True => Expression::boolean(true),
            Tok::False => Expression::boolean(false),
            Tok::Color(c) => Expression::Literal(ExprValue::Color(c)),
            Tok::Var(name) => match (self.resolve)(&name) {
                Some(id) => Expression::Variable(id),
                None => return err(pos, format!("Unknown variable '{name}'")),
            },
            Tok::Op("(") => {
                let e = self.expr(depth + 1)?;
                if !self.eat(")") {
                    return err(self.pos(), "Missing ')'");
                }
                e
            }
            Tok::Op(o) => return err(pos, format!("Unexpected '{o}'")),
        })
    }
}

/// Parses `src`. `resolve` maps a variable name as written (`$count` gives
/// "count", `{Cart/Total}` gives "Cart/Total") to a variable id.
pub fn parse_expression(src: &str, resolve: &dyn Fn(&str) -> Option<String>) -> Result<Expression, ParseError> {
    if src.len() > MAX_SOURCE_LEN {
        return err(MAX_SOURCE_LEN, "Expression is too long");
    }
    if src.trim().is_empty() {
        return err(0, "Enter a value");
    }
    let toks = Lexer { s: src, pos: 0 }.tokens()?;
    let mut p = Parser { toks, i: 0, end: src.len(), resolve, nodes: 0 };
    let e = p.expr(1)?;
    if p.i < p.toks.len() {
        return err(p.pos(), "Unexpected input after the expression");
    }
    Ok(e)
}

impl Expression {
    /// The text form; `name_of` gives a variable's display name by id (the id
    /// itself is shown when it returns `None`).
    pub fn to_source(&self, name_of: &dyn Fn(&str) -> Option<String>) -> String {
        if !self.within_limits() {
            return String::new();
        }
        let mut out = String::new();
        self.write(&mut out, 0, name_of);
        out
    }

    fn write(&self, out: &mut String, min: u8, name_of: &dyn Fn(&str) -> Option<String>) {
        match self {
            Expression::Literal(ExprValue::Number(n)) => out.push_str(&format_number(*n)),
            Expression::Literal(ExprValue::Boolean(b)) => out.push_str(&b.to_string()),
            Expression::Literal(ExprValue::Color(c)) => {
                out.push('#');
                out.push_str(&c.to_hex());
            }
            Expression::Literal(ExprValue::String(s)) => {
                out.push('"');
                for ch in s.chars() {
                    match ch {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\t' => out.push_str("\\t"),
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Expression::Variable(id) => {
                let name = name_of(id).unwrap_or_else(|| id.clone());
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    out.push('$');
                    out.push_str(&name);
                } else {
                    out.push('{');
                    out.push_str(&name);
                    out.push('}');
                }
            }
            Expression::Unary { op, arg } => {
                out.push(if *op == UnaryOp::Not { '!' } else { '-' });
                let paren = !matches!(**arg, Expression::Literal(_) | Expression::Variable(_) | Expression::Unary { .. });
                if paren {
                    out.push('(');
                }
                arg.write(out, 0, name_of);
                if paren {
                    out.push(')');
                }
            }
            Expression::Binary { op, lhs, rhs } => {
                let p = op.prec();
                let paren = p < min;
                if paren {
                    out.push('(');
                }
                lhs.write(out, p, name_of);
                out.push(' ');
                out.push_str(op.symbol());
                out.push(' ');
                // left associative: an equal-precedence right operand needs parentheses
                rhs.write(out, p + 1, name_of);
                if paren {
                    out.push(')');
                }
            }
            Expression::Ternary { cond, then, otherwise } => {
                let paren = min > 0;
                if paren {
                    out.push('(');
                }
                cond.write(out, 2, name_of);
                out.push_str(" ? ");
                then.write(out, 0, name_of);
                out.push_str(" : ");
                otherwise.write(out, 0, name_of);
                if paren {
                    out.push(')');
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(name: &str) -> Option<String> {
        match name {
            "count" | "Cart/Count" => Some("v:count".into()),
            "name" => Some("v:name".into()),
            "on" => Some("v:on".into()),
            "x" => Some("v:x".into()),
            _ => None,
        }
    }

    fn env(id: &str) -> Option<ExprValue> {
        Some(match id {
            "v:count" => ExprValue::Number(3.0),
            "v:name" => ExprValue::String("Ada".into()),
            "v:on" => ExprValue::Boolean(true),
            "v:x" => ExprValue::Number(0.0),
            _ => return None,
        })
    }

    fn run(src: &str) -> Result<ExprValue, String> {
        let e = parse_expression(src, &ids).map_err(|e| e.to_string())?;
        e.eval(&env).map_err(|e| e.to_string())
    }

    fn num(src: &str) -> f64 {
        match run(src) {
            Ok(ExprValue::Number(n)) => n,
            other => panic!("{src}: {other:?}"),
        }
    }

    fn bool_(src: &str) -> bool {
        match run(src) {
            Ok(ExprValue::Boolean(b)) => b,
            other => panic!("{src}: {other:?}"),
        }
    }

    #[test]
    fn arithmetic_follows_precedence_and_left_associativity() {
        assert_eq!(num("1 + 2 * 3"), 7.0);
        assert_eq!(num("(1 + 2) * 3"), 9.0);
        assert_eq!(num("10 - 4 - 3"), 3.0);
        assert_eq!(num("100 / 10 / 5"), 2.0);
        assert_eq!(num("7 % 4"), 3.0);
        assert_eq!(num("-2 * -3"), 6.0);
        assert_eq!(num("--4"), 4.0);
        assert_eq!(num("$count + 1"), 4.0);
        assert_eq!(num("{Cart/Count} * 2"), 6.0);
        assert_eq!(num(".5 + 1.25"), 1.75);
    }

    #[test]
    fn comparisons_and_logic() {
        assert!(bool_("$count >= 3"));
        assert!(!bool_("$count > 3"));
        assert!(bool_("$count == 3 && $on"));
        assert!(bool_("$count != 3 || $on"));
        assert!(bool_("!($count < 2)"));
        assert!(bool_("!!true"));
        assert!(bool_("1 + 1 == 2"));
        assert!(bool_("\"a\" < \"b\""));
        assert!(bool_("$name == 'Ada'"));
        assert!(!bool_("$name == 3"), "different types are never equal");
        assert!(bool_("1 < 2 == true"), "comparison binds tighter than equality");
        assert!(bool_("false || true && true"), "&& binds tighter than ||");
    }

    #[test]
    fn short_circuit_skips_the_dead_side() {
        assert!(!bool_("false && 1 / 0 > 1"));
        assert!(bool_("true || 1 / 0 > 1"));
        assert!(run("true && 1 / 0 > 1").is_err());
    }

    #[test]
    fn strings_concatenate_with_anything() {
        assert_eq!(run("\"Items: \" + $count"), Ok(ExprValue::String("Items: 3".into())));
        assert_eq!(run("$name + \"!\""), Ok(ExprValue::String("Ada!".into())));
        assert_eq!(run("1 + \"x\" + 2.5"), Ok(ExprValue::String("1x2.5".into())));
        assert_eq!(run("\"on=\" + $on"), Ok(ExprValue::String("on=true".into())));
        assert_eq!(run("\"q\\\"uote\\n\""), Ok(ExprValue::String("q\"uote\n".into())));
        assert!(run("'it''s'").is_err(), "adjacent strings are not joined");
    }

    #[test]
    fn ternary_picks_a_branch() {
        assert_eq!(run("$on ? \"yes\" : \"no\""), Ok(ExprValue::String("yes".into())));
        assert_eq!(num("$count > 5 ? 1 : $count > 2 ? 2 : 3"), 2.0, "right associative");
        assert!(run("1 ? 2 : 3").is_err());
    }

    #[test]
    fn colour_literals() {
        assert_eq!(run("#FF0000"), Ok(ExprValue::Color(Color::rgb(255, 0, 0))));
        assert!(bool_("#ff0000 == #FF0000"));
        assert!(parse_expression("#12", &ids).is_err());
        assert!(parse_expression("#xyz", &ids).is_err());
    }

    #[test]
    fn runtime_errors() {
        assert_eq!(run("1 / 0"), Err("Division by zero".into()));
        assert_eq!(run("5 % 0"), Err("Division by zero".into()));
        assert_eq!(run("$count / $x"), Err("Division by zero".into()));
        assert!(run("\"a\" - 1").unwrap_err().contains("does not apply"));
        assert!(run("true + true").is_err());
        assert!(run("1 < \"a\"").is_err());
        assert!(run("!1").is_err());
        assert!(run("-\"a\"").is_err());
        assert!(run("1 && true").is_err());
        let e = parse_expression("$count", &|_| Some("missing".into())).unwrap();
        assert_eq!(e.eval(&env), Err(EvalError::UnknownVariable("missing".into())));
        assert_eq!(Expression::number(1e308).eval(&env), Ok(ExprValue::Number(1e308)));
        let big = Expression::binary(BinaryOp::Mul, Expression::number(1e308), Expression::number(10.0));
        assert_eq!(big.eval(&env), Err(EvalError::NotFinite));
    }

    #[test]
    fn parse_errors_carry_positions() {
        let e = parse_expression("1 + ", &ids).unwrap_err();
        assert!(e.message.contains("end"), "{e}");
        assert_eq!(parse_expression("1 + * 2", &ids).unwrap_err().pos, 4);
        assert_eq!(parse_expression("(1 + 2", &ids).unwrap_err().message, "Missing ')'");
        assert!(parse_expression("1 2", &ids).unwrap_err().message.contains("after"));
        assert!(parse_expression("$nope", &ids).unwrap_err().message.contains("Unknown variable 'nope'"));
        assert!(parse_expression("foo", &ids).unwrap_err().message.contains("$name"));
        assert!(parse_expression("\"open", &ids).unwrap_err().message.contains("Unterminated"));
        assert!(parse_expression("{open", &ids).is_err());
        assert!(parse_expression("{}", &ids).is_err());
        assert!(parse_expression("$", &ids).is_err());
        assert!(parse_expression("", &ids).is_err());
        assert!(parse_expression("   ", &ids).is_err());
        assert!(parse_expression("1 @ 2", &ids).is_err());
        assert!(parse_expression("1 ? 2", &ids).is_err());
        assert!(parse_expression("1.2.3", &ids).is_err());
    }

    #[test]
    fn limits_stop_hostile_input() {
        // deeply nested parentheses must be rejected, not overflow the stack
        let deep = format!("{}1{}", "(".repeat(5000), ")".repeat(5000));
        assert!(parse_expression(&deep, &ids).is_err());
        let moderate = format!("{}1{}", "(".repeat(20), ")".repeat(20));
        assert_eq!(parse_expression(&moderate, &ids).map(|e| e.depth() <= MAX_DEPTH), Ok(true));
        let nots = format!("{}true", "!".repeat(5000));
        assert!(parse_expression(&nots, &ids).is_err());
        // a long chain is bounded by the node limit
        let chain = vec!["1"; 400].join(" + ");
        assert!(parse_expression(&chain, &ids).is_err());
        assert!(parse_expression(&"1".repeat(MAX_SOURCE_LEN + 1), &ids).is_err());
        // a hand-built oversized tree (from a file) is refused at evaluation
        let mut e = Expression::number(1.0);
        for _ in 0..200 {
            e = Expression::logical_not(e);
        }
        assert!(!e.within_limits());
        assert_eq!(e.eval(&env), Err(EvalError::TooComplex));
        assert!(e.variables().is_empty());
        assert_eq!(e.to_source(&|_| None), "");
        assert!(e.type_of(&|_| None).is_err());
        // a string cannot grow without bound
        let s = Expression::text("x".repeat(MAX_STRING_LEN));
        let two = Expression::binary(BinaryOp::Add, s.clone(), s);
        assert_eq!(two.eval(&env), Err(EvalError::TooComplex));
    }

    #[test]
    fn source_round_trips() {
        let name_of = |id: &str| match id {
            "v:count" => Some("Cart/Count".to_string()),
            "v:name" => Some("name".to_string()),
            "v:on" => Some("on".to_string()),
            _ => None,
        };
        for src in [
            "{Cart/Count} + 1",
            "($name + \"!\") + \"?\"",
            "1 - (2 - 3)",
            "(1 + 2) * 3",
            "$on && ($count > 1 || $count < 0)",
            "!($count == 3)",
            "-(1 + 2)",
            "$on ? 1 : 2",
            "1 + ($on ? 1 : 2)",
            "\"a \\\"b\\\" \\n\"",
            "#FF8800",
            "2.5 * 4",
        ] {
            let resolve = |n: &str| if n == "Cart/Count" { Some("v:count".to_string()) } else { ids(n) };
            let e = parse_expression(src, &resolve).unwrap_or_else(|e| panic!("{src}: {e}"));
            let printed = e.to_source(&name_of);
            let again = parse_expression(&printed, &resolve).unwrap_or_else(|er| panic!("{printed}: {er}"));
            assert_eq!(e, again, "{src} -> {printed}");
            assert_eq!(printed, again.to_source(&name_of), "printing is stable");
        }
        let e = parse_expression("1 - (2 - 3)", &ids).unwrap();
        assert_eq!(e.to_source(&|_| None), "1 - (2 - 3)");
        assert_eq!(Expression::var("VariableID:1:2").to_source(&|_| None), "{VariableID:1:2}");
    }

    #[test]
    fn static_types() {
        let kind = |id: &str| match id {
            "v:count" => Some(VariableKind::Number),
            "v:name" => Some(VariableKind::String),
            "v:on" => Some(VariableKind::Boolean),
            _ => None,
        };
        let ty = |src: &str| parse_expression(src, &ids).unwrap().type_of(&kind);
        assert_eq!(ty("$count + 1"), Ok(ExprType::Number));
        assert_eq!(ty("$count + \"x\""), Ok(ExprType::String));
        assert_eq!(ty("$count > 1 && $on"), Ok(ExprType::Boolean));
        assert_eq!(ty("$on ? $count : 0"), Ok(ExprType::Number));
        assert_eq!(ty("$name == 1"), Ok(ExprType::Boolean));
        assert!(ty("$name - 1").unwrap_err().contains("needs numbers"));
        assert!(ty("$on + $on").is_err());
        assert!(ty("$count && $on").is_err());
        assert!(ty("$on ? 1 : \"a\"").unwrap_err().contains("same type"));
        assert!(ty("$count ? 1 : 2").is_err());
        assert!(ty("!$count").is_err());
        assert!(ty("-$name").is_err());
        assert!(ty("$x").is_err(), "x resolves to an id the type map lacks");
    }

    #[test]
    fn variables_listed_once() {
        let e = parse_expression("$count + $count * $x > 2 && $on", &ids).unwrap();
        assert_eq!(e.variables(), ["v:count", "v:x", "v:on"]);
    }

    #[test]
    fn values_convert_to_variable_values() {
        assert_eq!(ExprValue::Number(2.5).to_variable_value(VariableKind::Number), Some(VariableValue::Number(2.5)));
        assert_eq!(ExprValue::Number(2.0).to_variable_value(VariableKind::String), Some(VariableValue::String("2".into())));
        assert_eq!(ExprValue::Boolean(true).to_variable_value(VariableKind::Boolean), Some(VariableValue::Boolean(true)));
        assert_eq!(ExprValue::Boolean(true).to_variable_value(VariableKind::Number), None);
        assert_eq!(ExprValue::String("a".into()).to_variable_value(VariableKind::Color), None);
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(0.1 + 0.2), "0.3");
        assert_eq!(format_number(-1.5), "-1.5");
    }

    #[test]
    fn serde_round_trip() {
        let e = parse_expression("$count > 1 ? \"many\" : \"few\"", &ids).unwrap();
        let j = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<Expression>(&j).unwrap(), e);
    }
}
