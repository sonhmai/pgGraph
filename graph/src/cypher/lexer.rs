//! Lightweight token scanning for the openCypher compatibility boundary.

use crate::gql::errors::Span;

/// Keyword token used to preflight openCypher-only constructs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CypherKeyword {
    /// `ALTER`
    Alter,
    /// `CALL`
    Call,
    /// `CONSTRAINT`
    Constraint,
    /// `CREATE`
    Create,
    /// `DATABASE`
    Database,
    /// `DROP`
    Drop,
    /// `FOREACH`
    Foreach,
    /// `INDEX`
    Index,
    /// `LOAD`
    Load,
    /// `START`
    Start,
    /// `UNION`
    Union,
    /// `UNWIND`
    Unwind,
    /// `YIELD`
    Yield,
}

impl CypherKeyword {
    pub(crate) fn feature_name(self) -> &'static str {
        match self {
            Self::Call | Self::Yield => "CALL/YIELD procedures",
            Self::Alter
            | Self::Constraint
            | Self::Create
            | Self::Database
            | Self::Drop
            | Self::Index => "Cypher DDL",
            Self::Foreach => "FOREACH",
            Self::Load => "LOAD CSV",
            Self::Start => "START",
            Self::Union => "UNION",
            Self::Unwind => "UNWIND",
        }
    }
}

/// One word occurrence outside quoted strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WordToken {
    /// Uppercase word text.
    pub(crate) text: String,
    /// Recognized openCypher compatibility keyword, when relevant.
    pub(crate) keyword: Option<CypherKeyword>,
    /// Source span.
    pub(crate) span: Span,
}

/// Find words outside quoted strings.
pub(crate) fn words(input: &str) -> Vec<WordToken> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        match bytes[offset] {
            b'\'' | b'"' => {
                offset = skip_quoted(input, offset);
            }
            byte if is_ident_start(byte) => {
                let start = offset;
                offset += 1;
                while offset < bytes.len() && is_ident_continue(bytes[offset]) {
                    offset += 1;
                }
                let text = input[start..offset].to_ascii_uppercase();
                tokens.push(WordToken {
                    keyword: keyword(&text),
                    text,
                    span: Span::new(start, offset),
                });
            }
            _ => offset += 1,
        }
    }
    tokens
}

fn skip_quoted(input: &str, start: usize) -> usize {
    let quote = input.as_bytes()[start];
    let mut offset = start + 1;
    let bytes = input.as_bytes();
    while offset < bytes.len() {
        if bytes[offset] == quote {
            if offset + 1 < bytes.len() && bytes[offset + 1] == quote {
                offset += 2;
            } else {
                return offset + 1;
            }
        } else {
            offset += 1;
        }
    }
    offset
}

fn keyword(text: &str) -> Option<CypherKeyword> {
    match text {
        "ALTER" => Some(CypherKeyword::Alter),
        "CALL" => Some(CypherKeyword::Call),
        "CONSTRAINT" => Some(CypherKeyword::Constraint),
        "CREATE" => Some(CypherKeyword::Create),
        "DATABASE" => Some(CypherKeyword::Database),
        "DROP" => Some(CypherKeyword::Drop),
        "FOREACH" => Some(CypherKeyword::Foreach),
        "INDEX" => Some(CypherKeyword::Index),
        "LOAD" => Some(CypherKeyword::Load),
        "START" => Some(CypherKeyword::Start),
        "UNION" => Some(CypherKeyword::Union),
        "UNWIND" => Some(CypherKeyword::Unwind),
        "YIELD" => Some(CypherKeyword::Yield),
        _ => None,
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_ident_continue(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
}
