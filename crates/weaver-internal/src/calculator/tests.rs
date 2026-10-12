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
