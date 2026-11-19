//! The structure an expression may have before it is parsed. The parser, the program it builds
//! and every walk over it (the fast path's translation, `reads_only`, evaluation, dropping the
//! tree) recurse, one frame a level: a pack's expression must not nest deep enough to overflow
//! the stack, which aborts the process rather than unwinding. This counts on the tokens alone,
//! in one pass, without recursing.
use super::lex::Token;

/// The most tokens an expression may have: the work its parse and each evaluation do.
pub const MAX_TOKENS: usize = 1024;
/// The deepest an expression may nest: each open bracket, and each operator at a bracket's level
/// (unary, binary, `?`, `^` and a method call's `.`: each nests the tree, or the parser, one
/// level), counts one. A walk takes about 3 KiB of stack a level in a debug build.
pub const MAX_DEPTH: usize = 64;

/// Why an expression is refused before it is parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverBudget {
    Tokens(usize),
    Depth,
}

impl std::fmt::Display for OverBudget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tokens(n) => write!(
                f,
                "{n} tokens, more than the {MAX_TOKENS} an expression may have"
            ),
            Self::Depth => write!(
                f,
                "nests deeper than the {MAX_DEPTH} levels an expression may have"
            ),
        }
    }
}

/// Whether `tokens` fit the budget. Over-counts nesting rather than under-counting it: a level's
/// operators all count until a `,` or `;` ends its operand or its statement.
pub(super) fn check(tokens: &[(Token, usize)]) -> Result<(), OverBudget> {
    if tokens.len() > MAX_TOKENS {
        return Err(OverBudget::Tokens(tokens.len()));
    }
    // Each open level's own count, and their sum with the levels themselves.
    let mut levels: Vec<usize> = vec![0];
    let mut depth = 0usize;
    for (token, _) in tokens {
        let Token::Punct(p) = token else {
            continue;
        };
        match *p {
            "(" | "{" => {
                levels.push(0);
                depth += 1;
            }
            ")" | "}" => {
                if levels.len() > 1 {
                    depth -= 1 + levels.pop().unwrap_or(0);
                }
            }
            "," | ";" => {
                let level = levels.last_mut().expect("the outermost level stays");
                depth -= *level;
                *level = 0;
            }
            // An assignment and a ternary's `:` nest nothing.
            "=" | "+=" | "-=" | "*=" | "/=" | ":" => {}
            // Unary, binary, `?`, `^` and `.`: one level each.
            _ => {
                *levels.last_mut().expect("the outermost level stays") += 1;
                depth += 1;
            }
        }
        if depth > MAX_DEPTH {
            return Err(OverBudget::Depth);
        }
    }
    Ok(())
}
