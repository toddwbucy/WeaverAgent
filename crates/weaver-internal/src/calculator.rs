//! conforms: internal-calculator-power-conventions
//! conforms: internal-calculator-refuses-in-its-own-words
//! conforms: internal-calculator-depth-bounded
//!
//! The first framework member: a scientific expression evaluator, a pure
//! function from an expression string to a rendered value or a refusal in
//! its own words, per `weaver-internal-Spec` section 3. Arithmetic is IEEE
//! 754 binary64 throughout. The apex places this member at
//! `weaver-agents-PRD` section 4: a model-elected call whose result the
//! harness supplies deterministically, and an operator control loop may
//! fire it autonomically once the ladder clears - either way the loop is
//! the caller, and no model ever addresses it by name.

/// Evaluate one expression to its rendered value.
///
/// **The rendering is the shortest decimal that round-trips the answer
/// under binary64, without exponent notation**, an integer-valued answer
/// rendering with no fractional part - "1591" is what a model quotes, and
/// "1591.0" invites it to copy the artifact. A negative zero keeps its
/// sign. The refusal is this member's own words and names its position,
/// zero-based, in Unicode scalar values over the whitespace-stripped
/// expression.
pub fn evaluate(expression: &str) -> Result<String, String> {
    let value = evaluate_value(expression)?;
    // A negative zero is integer-valued and still not the integer branch's:
    // `-0.0 as i64` is 0, which drops the sign the Spec says survives.
    let negative_zero = value == 0.0 && value.is_sign_negative();
    Ok(
        if value.fract() == 0.0 && value.abs() < 1e15 && !negative_zero {
            format!("{}", value as i64)
        } else {
            format!("{value}")
        },
    )
}

const MAX_DEPTH: usize = 64;

fn evaluate_value(expression: &str) -> Result<f64, String> {
    let tokens: Vec<char> = expression.chars().filter(|c| !c.is_whitespace()).collect();
    let mut position = 0usize;
    let value = parse_sum(&tokens, &mut position, 0)?;
    if position != tokens.len() {
        return Err(format!("unexpected character at position {position}"));
    }
    if !value.is_finite() {
        return Err("the result is not a finite number".to_string());
    }
    Ok(value)
}

fn parse_sum(tokens: &[char], position: &mut usize, depth: usize) -> Result<f64, String> {
    if depth >= MAX_DEPTH {
        return Err(format!("the expression nests deeper than {MAX_DEPTH}"));
    }
    let mut value = parse_product(tokens, position, depth)?;
    while let Some(&op) = tokens.get(*position) {
        match op {
            '+' => {
                *position += 1;
                value += parse_product(tokens, position, depth)?;
            }
            '-' => {
                *position += 1;
                value -= parse_product(tokens, position, depth)?;
            }
            _ => break,
        }
    }
    Ok(value)
}

fn parse_product(tokens: &[char], position: &mut usize, depth: usize) -> Result<f64, String> {
    let mut value = parse_power(tokens, position, depth)?;
    while let Some(&op) = tokens.get(*position) {
        match op {
            '*' => {
                *position += 1;
                value *= parse_power(tokens, position, depth + 1)?;
            }
            '/' => {
                *position += 1;
                let divisor = parse_power(tokens, position, depth + 1)?;
                if divisor == 0.0 {
                    return Err("division by zero".to_string());
                }
                value /= divisor;
            }
            _ => break,
        }
    }
    Ok(value)
}

/// `^` binds tighter than `*` and `/` and associates right, so `2^3^2` is
/// `2^(3^2)` and `-2^2` is `-(2^2)`, the conventions a scientific reader
/// expects.
fn parse_power(tokens: &[char], position: &mut usize, depth: usize) -> Result<f64, String> {
    // The bound holds here as well as at `parse_sum`: this level recurses
    // into itself for the unary minus and for the exponent, and a chain of
    // either reaches no `parse_sum` on the way down.
    if depth >= MAX_DEPTH {
        return Err(format!("the expression nests deeper than {MAX_DEPTH}"));
    }
    // Unary minus lives at this level and binds looser than `^`, so `-2^2`
    // is `-(2^2)` and an exponent's own sign, `2^-3`, recurses through here.
    if tokens.get(*position) == Some(&'-') {
        *position += 1;
        return Ok(-parse_power(tokens, position, depth + 1)?);
    }
    let base = parse_atom(tokens, position, depth)?;
    if tokens.get(*position) == Some(&'^') {
        *position += 1;
        let exponent = parse_power(tokens, position, depth + 1)?;
        return Ok(base.powf(exponent));
    }
    Ok(base)
}

fn parse_atom(tokens: &[char], position: &mut usize, depth: usize) -> Result<f64, String> {
    match tokens.get(*position) {
        Some('(') => {
            *position += 1;
            let value = parse_sum(tokens, position, depth + 1)?;
            if tokens.get(*position) != Some(&')') {
                return Err(format!("unclosed parenthesis at position {position}"));
            }
            *position += 1;
            Ok(value)
        }
        Some(c) if c.is_ascii_alphabetic() => {
            let start = *position;
            while tokens
                .get(*position)
                .is_some_and(|c| c.is_ascii_alphabetic())
            {
                *position += 1;
            }
            let word: String = tokens[start..*position].iter().collect();
            match word.as_str() {
                "pi" => return Ok(std::f64::consts::PI),
                "e" => return Ok(std::f64::consts::E),
                _ => {}
            }
            if tokens.get(*position) != Some(&'(') {
                return Err(format!("unknown name {word} at position {start}"));
            }
            *position += 1;
            let argument = parse_sum(tokens, position, depth + 1)?;
            if tokens.get(*position) != Some(&')') {
                return Err(format!("unclosed call to {word} at position {position}"));
            }
            *position += 1;
            match word.as_str() {
                "sin" => Ok(argument.sin()),
                "cos" => Ok(argument.cos()),
                "tan" => Ok(argument.tan()),
                "asin" => Ok(argument.asin()),
                "acos" => Ok(argument.acos()),
                "atan" => Ok(argument.atan()),
                "sqrt" => {
                    if argument < 0.0 {
                        return Err("square root of a negative number".to_string());
                    }
                    Ok(argument.sqrt())
                }
                "ln" => {
                    if argument <= 0.0 {
                        return Err("logarithm of a non-positive number".to_string());
                    }
                    Ok(argument.ln())
                }
                "log" => {
                    if argument <= 0.0 {
                        return Err("logarithm of a non-positive number".to_string());
                    }
                    Ok(argument.log10())
                }
                "exp" => Ok(argument.exp()),
                "abs" => Ok(argument.abs()),
                _ => Err(format!("unknown function {word} at position {start}")),
            }
        }
        Some(c) if c.is_ascii_digit() || *c == '.' => {
            let start = *position;
            while tokens
                .get(*position)
                .is_some_and(|c| c.is_ascii_digit() || *c == '.')
            {
                *position += 1;
            }
            let text: String = tokens[start..*position].iter().collect();
            text.parse::<f64>()
                .map_err(|_| format!("not a number at position {start}"))
        }
        _ => Err(format!("expected a value at position {position}")),
    }
}

#[cfg(test)]
mod tests {
    use super::evaluate;

    /// **Parentheses, precedence, associativity, the function set, and the
    /// conventions the Spec asserts** - `^` right-associative and tighter
    /// than `*` and `/`, unary minus looser than `^` - each case one claim.
    #[test]
    fn the_grammar_covers_its_claims() {
        for (expression, expected) in [
            ("1 + 2 * 3", "7"),
            ("(1 + 2) * 3", "9"),
            ("-4 + 6", "2"),
            ("7 / 2", "3.5"),
            ("10 - 2 - 3", "5"),
            ("2^10", "1024"),
            ("2^3^2", "512"),
            ("-2^2", "-4"),
            ("2^-3", "0.125"),
            ("sqrt(16) + 2", "6"),
            ("cos(0)", "1"),
            ("ln(e)", "1"),
            ("log(1000)", "3"),
            ("abs(-3) * exp(0)", "3"),
            ("sin(pi / 2)", "1"),
            ("-0", "-0"),
        ] {
            assert_eq!(
                evaluate(expression).as_deref(),
                Ok(expected),
                "for {expression}"
            );
        }
    }

    /// **Every refusal is the member's own words and names its defect**,
    /// the domain edges and the grammar misses of Spec section 3, with the
    /// depth bound refusing at entry: sixty-three nested parentheses
    /// evaluate and a sixty-fourth refuses, a minus chain governed by the
    /// same count.
    #[test]
    fn a_bad_expression_refuses_in_the_members_own_words() {
        let deep = format!("{}1", "(".repeat(90));
        let minus_chain = format!("{}1", "-".repeat(90));
        let power_chain = format!("1{}", "^1".repeat(90));
        for (expression, expected) in [
            ("1 +", "expected a value at position 2"),
            ("bogus(1)", "unknown function bogus at position 0"),
            ("x + 1", "unknown name x at position 0"),
            ("sqrt(-1)", "square root of a negative number"),
            ("ln(0)", "logarithm of a non-positive number"),
            ("1/0", "division by zero"),
            (deep.as_str(), "the expression nests deeper than 64"),
            (minus_chain.as_str(), "the expression nests deeper than 64"),
            (power_chain.as_str(), "the expression nests deeper than 64"),
        ] {
            let refusal = evaluate(expression).expect_err(expression);
            assert!(refusal.contains(expected), "for {expression}: {refusal}");
        }
    }

    /// The boundary the Spec states concretely: sixty-three nested
    /// parentheses evaluate, the sixty-fourth refuses at entry.
    #[test]
    fn the_depth_bound_refuses_at_entry_and_not_before() {
        let at_the_bound = format!("{}1{}", "(".repeat(63), ")".repeat(63));
        assert_eq!(evaluate(&at_the_bound).as_deref(), Ok("1"), "63 nests fit");
        let past_it = format!("{}1{}", "(".repeat(64), ")".repeat(64));
        assert!(
            evaluate(&past_it)
                .expect_err("the 64th refuses")
                .contains("nests deeper than 64")
        );
    }
}
