//! The calculator: evaluates what's typed, like `1200*12`, `(4+5)^2`,
//! `15% of 80` or `sqrt(2)`.
//!
//! Supports `+ - * / ^`, `x`/`×`/`÷` as multiply and divide, `%` (divide by
//! 100), `of` (multiply), parentheses, the constants `pi` and `e`, and the
//! functions `sqrt abs round floor ceil ln log exp sin cos tan asin acos
//! atan min max`. Trig functions use radians.

/// Evaluates `input`. With `require_operator`, plain numbers like `2024`
/// don't count as math, so typing one doesn't show a calculator result.
/// Returns `None` for anything that isn't a valid expression or doesn't
/// give a finite number.
pub fn evaluate(input: &str, require_operator: bool) -> Option<f64> {
    let tokens = tokenize(input)?;
    if require_operator && !tokens.iter().any(Token::is_math) {
        return None;
    }
    let mut parser = Parser { tokens, pos: 0 };
    let value = parser.expression()?;
    let finished = parser.pos == parser.tokens.len();
    (finished && value.is_finite()).then_some(value)
}

/// A result ready to show and to copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatted {
    /// With thousands separators: `14,400`.
    pub display: String,
    /// Without them, for pasting elsewhere: `14400`.
    pub copy: String,
}

/// Formats a result to at most 10 significant digits, which also hides
/// floating point noise (`0.1 + 0.2` shows as `0.3`). Very large and very
/// small numbers use scientific notation.
pub fn format(value: f64) -> Formatted {
    let abs = value.abs();
    if abs != 0.0 && !(1e-9..1e15).contains(&abs) {
        let text = format!("{value:.9e}");
        let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
        let text = format!("{}e{exponent}", trim_zeros(mantissa));
        return Formatted {
            display: text.clone(),
            copy: text,
        };
    }

    // Negative for small numbers: 0.001 has -2 digits before its point.
    let digits_before_point = if abs == 0.0 {
        1
    } else {
        abs.log10().floor() as i32 + 1
    };
    let decimals = (10 - digits_before_point).clamp(0, 15) as usize;
    let mut copy = trim_zeros(&format!("{value:.decimals$}")).to_string();
    if copy == "-0" {
        copy = "0".into();
    }
    Formatted {
        display: group_thousands(&copy),
        copy,
    }
}

fn trim_zeros(number: &str) -> &str {
    if number.contains('.') {
        number.trim_end_matches('0').trim_end_matches('.')
    } else {
        number
    }
}

fn group_thousands(number: &str) -> String {
    let (sign, rest) = match number.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", number),
    };
    let (int, frac) = match rest.split_once('.') {
        Some((int, frac)) => (int, Some(frac)),
        None => (rest, None),
    };
    let mut grouped = String::new();
    for (i, digit) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    match frac {
        Some(frac) => format!("{sign}{grouped}.{frac}"),
        None => format!("{sign}{grouped}"),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Word(String),
    Op(char),
    Open,
    Close,
    Comma,
}

impl Token {
    /// Whether this token makes the input clearly math. A lone number or
    /// constant isn't: typing `e` to find Edge shouldn't show 2.718.
    fn is_math(&self) -> bool {
        matches!(self, Token::Op(_) | Token::Open)
    }
}

fn tokenize(input: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            '0'..='9' | '.' => {
                let mut number = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() || d == '.' {
                        number.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Number(number.parse().ok()?));
            }
            c if c.is_alphabetic() => {
                let mut word = String::new();
                while let Some(&l) = chars.peek() {
                    if l.is_alphabetic() {
                        word.extend(l.to_lowercase());
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(match word.as_str() {
                    "x" => Token::Op('*'),
                    "π" => Token::Word("pi".into()),
                    _ => Token::Word(word),
                });
            }
            _ => {
                chars.next();
                tokens.push(match c {
                    '+' | '-' | '*' | '/' | '^' | '%' => Token::Op(c),
                    '×' => Token::Op('*'),
                    '÷' => Token::Op('/'),
                    '(' => Token::Open,
                    ')' => Token::Close,
                    ',' => Token::Comma,
                    _ => return None,
                });
            }
        }
    }
    (!tokens.is_empty()).then_some(tokens)
}

/// Recursive descent, lowest precedence first:
/// sum → product → unary → power → percent → atom.
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn eat(&mut self, token: &Token) -> bool {
        let found = self.peek() == Some(token);
        if found {
            self.pos += 1;
        }
        found
    }

    fn expression(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        loop {
            if self.eat(&Token::Op('+')) {
                value += self.product()?;
            } else if self.eat(&Token::Op('-')) {
                value -= self.product()?;
            } else {
                return Some(value);
            }
        }
    }

    fn product(&mut self) -> Option<f64> {
        let mut value = self.unary()?;
        loop {
            if self.eat(&Token::Op('*')) || self.eat(&Token::Word("of".into())) {
                value *= self.unary()?;
            } else if self.eat(&Token::Op('/')) {
                value /= self.unary()?;
            } else {
                return Some(value);
            }
        }
    }

    fn unary(&mut self) -> Option<f64> {
        if self.eat(&Token::Op('-')) {
            Some(-self.unary()?)
        } else if self.eat(&Token::Op('+')) {
            self.unary()
        } else {
            self.power()
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.percent()?;
        if self.eat(&Token::Op('^')) {
            // Right-associative, and binds tighter than a leading minus:
            // -2^2 is -4, 2^-1 is 0.5.
            Some(base.powf(self.unary()?))
        } else {
            Some(base)
        }
    }

    fn percent(&mut self) -> Option<f64> {
        let mut value = self.atom()?;
        while self.eat(&Token::Op('%')) {
            value /= 100.0;
        }
        Some(value)
    }

    fn atom(&mut self) -> Option<f64> {
        match self.tokens.get(self.pos).cloned()? {
            Token::Number(n) => {
                self.pos += 1;
                Some(n)
            }
            Token::Open => {
                self.pos += 1;
                let value = self.expression()?;
                self.eat(&Token::Close).then_some(value)
            }
            Token::Word(word) => {
                self.pos += 1;
                match word.as_str() {
                    "pi" => Some(std::f64::consts::PI),
                    "e" => Some(std::f64::consts::E),
                    _ => {
                        let args = self.arguments()?;
                        call(&word, &args)
                    }
                }
            }
            _ => None,
        }
    }

    fn arguments(&mut self) -> Option<Vec<f64>> {
        if !self.eat(&Token::Open) {
            return None;
        }
        let mut args = vec![self.expression()?];
        while self.eat(&Token::Comma) {
            args.push(self.expression()?);
        }
        self.eat(&Token::Close).then_some(args)
    }
}

fn call(function: &str, args: &[f64]) -> Option<f64> {
    let one = || (args.len() == 1).then(|| args[0]);
    Some(match function {
        "sqrt" => one()?.sqrt(),
        "abs" => one()?.abs(),
        "round" => one()?.round(),
        "floor" => one()?.floor(),
        "ceil" => one()?.ceil(),
        "ln" => one()?.ln(),
        "log" => one()?.log10(),
        "exp" => one()?.exp(),
        "sin" => one()?.sin(),
        "cos" => one()?.cos(),
        "tan" => one()?.tan(),
        "asin" => one()?.asin(),
        "acos" => one()?.acos(),
        "atan" => one()?.atan(),
        "min" => args.iter().copied().reduce(f64::min)?,
        "max" => args.iter().copied().reduce(f64::max)?,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(input: &str) -> Option<f64> {
        evaluate(input, true)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn basic_arithmetic() {
        assert_eq!(eval("1200*12"), Some(14400.0));
        assert_eq!(eval("1 + 2 * 3"), Some(7.0));
        assert_eq!(eval("(1 + 2) * 3"), Some(9.0));
        assert_eq!(eval("10 / 4"), Some(2.5));
        assert_eq!(eval("10 - 2 - 3"), Some(5.0));
        assert_eq!(eval("2 ^ 10"), Some(1024.0));
    }

    #[test]
    fn alternative_operators() {
        assert_eq!(eval("5x3"), Some(15.0));
        assert_eq!(eval("5 × 3"), Some(15.0));
        assert_eq!(eval("9 ÷ 3"), Some(3.0));
    }

    #[test]
    fn powers_and_signs() {
        assert_eq!(eval("-2^2"), Some(-4.0));
        assert_eq!(eval("2^-1"), Some(0.5));
        assert_eq!(eval("2^3^2"), Some(512.0));
        assert_eq!(eval("--3"), Some(3.0));
        assert_eq!(eval("(4+5)^2"), Some(81.0));
    }

    #[test]
    fn percentages() {
        assert_eq!(eval("15% of 80"), Some(12.0));
        assert_eq!(eval("50%"), Some(0.5));
        assert_eq!(eval("200 * 10%"), Some(20.0));
    }

    #[test]
    fn functions_and_constants() {
        assert_eq!(eval("sqrt(16)"), Some(4.0));
        assert!(close(eval("pi * 2").unwrap(), std::f64::consts::TAU));
        assert!(close(eval("π + 0").unwrap(), std::f64::consts::PI));
        assert_eq!(eval("max(1, 5, 3)"), Some(5.0));
        assert_eq!(eval("log(1000)"), Some(3.0));
        assert!(close(eval("sin(pi / 2)").unwrap(), 1.0));
        assert_eq!(eval("ROUND(2.6)"), Some(3.0));
    }

    #[test]
    fn plain_numbers_need_an_operator_unless_asked() {
        assert_eq!(eval("2024"), None);
        assert_eq!(eval("e"), None);
        assert_eq!(eval("pi"), None);
        assert_eq!(evaluate("2024", false), Some(2024.0));
        assert!(close(evaluate("pi", false).unwrap(), std::f64::consts::PI));
        assert_eq!(eval("-5"), Some(-5.0));
    }

    #[test]
    fn app_names_are_not_math() {
        for input in ["7-Zip", "Notepad++", "1Password", "Paint 3D", "x"] {
            assert_eq!(eval(input), None, "{input:?}");
        }
    }

    #[test]
    fn rejects_non_math() {
        for input in [
            "",
            "chrome",
            "note pad",
            "2 +",
            "(1 + 2",
            "1 + 2)",
            "sqrt 4",
            "foo(1)",
            "1..2",
            "3 $ 4",
            "hello world 2",
        ] {
            assert_eq!(eval(input), None, "{input:?}");
        }
    }

    #[test]
    fn rejects_non_finite_results() {
        assert_eq!(eval("1 / 0"), None);
        assert_eq!(eval("sqrt(-1)"), None);
    }

    #[test]
    fn formats_with_separators_for_display_only() {
        let f = format(14400.0);
        assert_eq!(f.display, "14,400");
        assert_eq!(f.copy, "14400");
        assert_eq!(format(-1234567.5).display, "-1,234,567.5");
        assert_eq!(format(999.0).display, "999");
    }

    #[test]
    fn formats_away_floating_point_noise() {
        assert_eq!(format(0.1 + 0.2).copy, "0.3");
        assert_eq!(format(1.0 / 3.0).copy, "0.3333333333");
        assert_eq!(format(2.0f64.sqrt()).copy, "1.414213562");
        assert_eq!(format(-0.0).copy, "0");
        assert_eq!(format(0.0).copy, "0");
        assert_eq!(format(0.001234).copy, "0.001234");
    }

    #[test]
    fn formats_extremes_in_scientific_notation() {
        assert_eq!(format(1e20).copy, "1e20");
        assert_eq!(format(1.5e-12).copy, "1.5e-12");
        assert_eq!(format(123456789012345.0).display, "123,456,789,012,345");
    }
}
