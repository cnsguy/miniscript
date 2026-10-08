use crate::symbol::Symbol;
use crate::util::fmt_simple_sequence;
use chumsky::prelude::*;
use num::BigInt;
use std::fmt::{self, Debug, Display, Formatter};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Syntax {
    Nil,
    Bool(bool),
    Int(BigInt),
    Float(f64),
    String(Box<str>),
    Symbol(Symbol),
    List(Vec<Self>),
    Vector(Vec<Self>),
    Table(Vec<(Self, Self)>),
    Quote(Box<Self>),
    Keyword(Symbol),
}

impl Display for Syntax {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::String(value) => write!(f, "{value:?}"),
            Self::Symbol(value) => write!(f, "{value}"),
            Self::Quote(value) => write!(f, "'{value}"),
            Self::Keyword(value) => write!(f, ":{value}"),
            Self::List(value) => {
                write!(f, "(")?;
                fmt_simple_sequence(f, value.iter(), " ")?;
                write!(f, ")")
            }
            Self::Vector(value) => {
                write!(f, "[")?;
                fmt_simple_sequence(f, value.iter(), " ")?;
                write!(f, "]")
            }
            Self::Table(value) => {
                let mut peekable = value.iter().peekable();

                write!(f, "{{")?;

                while let Some((key, val)) = peekable.next() {
                    Display::fmt(key, f)?;
                    write!(f, " ")?;
                    Display::fmt(val, f)?;

                    if peekable.peek().is_some() {
                        write!(f, " ")?;
                    }
                }

                write!(f, "}}")
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("Syntax error: {0}")]
    SyntaxError(String),
}

fn into_message(err: Rich<'_, char>) -> String {
    err.reason().to_string()
}

type SubParserError<'a> = extra::Err<Rich<'a, char>>;

fn sign<'a>() -> impl Parser<'a, &'a str, (), SubParserError<'a>> + Clone {
    one_of("+-").ignored().labelled("sign")
}

fn int<'a>() -> impl Parser<'a, &'a str, BigInt, SubParserError<'a>> + Clone {
    sign()
        .or_not()
        .then(text::digits(10))
        .labelled("integer")
        .to_slice()
        .try_map(|v, span| str::parse::<BigInt>(v).map_err(|_| Rich::custom(span, "Invalid int")))
}

fn float<'a>() -> impl Parser<'a, &'a str, f64, SubParserError<'a>> + Clone {
    let exponent = one_of("eE")
        .ignore_then(sign().or_not())
        .ignore_then(text::digits(10))
        .ignored();

    let integer = text::digits(10);

    let fractional = choice((
        integer
            .then_ignore(just("."))
            .then(text::digits(10).or_not())
            .ignored(),
        just(".").then(integer).ignored(),
    ))
    .then(exponent.clone().or_not())
    .ignored();

    let float_body = fractional.or(integer.then(exponent).ignored());

    sign()
        .or_not()
        .then(float_body)
        .labelled("float")
        .to_slice()
        .try_map(|v, span| str::parse::<f64>(v).map_err(|_| Rich::custom(span, "Invalid float")))
}

fn string<'a>() -> impl Parser<'a, &'a str, Box<str>, SubParserError<'a>> + Clone {
    let hex_digit = one_of("0123456789abcdefABCDEF").repeated();

    let hex_code_to_char = |digits: String, span| {
        char::from_u32(u32::from_str_radix(&digits, 16).unwrap())
            .ok_or_else(|| Rich::custom(span, "Invalid unicode escape"))
    };

    let hex_sequence = just('x')
        .ignore_then(hex_digit.exactly(2).collect::<String>())
        .try_map(hex_code_to_char);

    let unicode_sequence = just('u')
        .ignore_then(hex_digit.exactly(4).collect::<String>())
        .try_map(hex_code_to_char);

    let large_unicode_sequence = just('U')
        .ignore_then(hex_digit.exactly(8).collect::<String>())
        .try_map(hex_code_to_char);

    let escape_sequence = just('\\').ignore_then(choice((
        just('\\').to('\\'),
        just('"').to('"'),
        just('n').to('\n'),
        just('r').to('\r'),
        just('t').to('\t'),
        hex_sequence,
        unicode_sequence,
        large_unicode_sequence,
    )));

    let string_char = none_of("\\\"").labelled("string characters");

    let string_parts = choice((escape_sequence, string_char))
        .repeated()
        .collect::<String>();

    just('"')
        .labelled("string")
        .ignore_then(string_parts)
        .then_ignore(just('"'))
        .map(|v| v.into_boxed_str())
}

macro_rules! bad_symbol_char {
    () => {
        "\"()[];{}"
    };
}

fn symbol_char<'a>() -> impl Parser<'a, &'a str, char, SubParserError<'a>> + Clone {
    none_of(bad_symbol_char!())
        .filter(|c: &char| !c.is_whitespace())
        .labelled("symbol character")
}

macro_rules! bad_symbol_start_char {
    () => {
        concat!("'", bad_symbol_char!())
    };
}

fn symbol_start_char<'a>() -> impl Parser<'a, &'a str, char, SubParserError<'a>> + Clone {
    none_of(bad_symbol_start_char!())
        .filter(|c: &char| !c.is_whitespace())
        .labelled("symbol start character")
}

fn symbol<'a>() -> impl Parser<'a, &'a str, Symbol, SubParserError<'a>> + Clone {
    symbol_start_char()
        .ignore_then(symbol_char().repeated())
        .to_slice()
        .map(ToOwned::to_owned)
        .map(Symbol::new)
}

fn symbol_like<'a>() -> impl Parser<'a, &'a str, Syntax, SubParserError<'a>> + Clone {
    symbol().map(|v| match v.as_ref() {
        "true" => Syntax::Bool(true),
        "false" => Syntax::Bool(false),
        "nil" => Syntax::Nil,
        _ => Syntax::Symbol(v),
    })
}

fn keyword<'a>() -> impl Parser<'a, &'a str, Symbol, SubParserError<'a>> + Clone {
    just(':').ignore_then(symbol())
}

fn atom<'a>() -> impl Parser<'a, &'a str, Syntax, SubParserError<'a>> + Clone {
    choice((
        string().map(Syntax::String),
        float().map(Syntax::Float),
        int().map(Syntax::Int),
        keyword().map(Syntax::Keyword),
        symbol_like(),
    ))
}

fn comment<'a>() -> impl Parser<'a, &'a str, (), SubParserError<'a>> + Clone {
    just(";")
        .then(none_of("\n").repeated())
        .then(just('\n').or_not())
        .padded()
        .repeated()
        .at_least(1)
        .ignored()
        .labelled("comment")
}

fn skip<'a>() -> impl Parser<'a, &'a str, (), SubParserError<'a>> + Clone {
    choice((comment(), text::whitespace()))
}

fn value<'a>() -> impl Parser<'a, &'a str, Syntax, SubParserError<'a>> {
    recursive(|rec| {
        let list = rec
            .clone()
            .separated_by(skip())
            .allow_trailing()
            .collect::<Vec<_>>()
            .padded_by(skip())
            .delimited_by(just('('), just(')'))
            .map(Syntax::List);

        let vector = rec
            .clone()
            .separated_by(skip())
            .allow_trailing()
            .collect::<Vec<_>>()
            .padded_by(skip())
            .delimited_by(just('['), just(']'))
            .map(Syntax::Vector);

        // XXX skip
        let object_entry = rec.clone().then_ignore(skip()).then(rec.clone());

        let object = object_entry
            .separated_by(skip())
            .allow_trailing()
            .collect::<Vec<_>>()
            .padded_by(skip())
            .delimited_by(just('{'), just('}'))
            .map(Syntax::Table);

        let quote = just('\'')
            .padded_by(skip())
            .ignore_then(rec)
            .map(|v| Syntax::Quote(Box::from(v)));

        choice((quote, list, vector, object, atom()))
    })
    .padded_by(skip())
}

fn toplevel<'a>() -> impl Parser<'a, &'a str, Vec<Syntax>, SubParserError<'a>> {
    skip().ignore_then(value().repeated().collect::<Vec<_>>())
}

pub fn parse(source: &str) -> Result<Box<[Syntax]>, ParserError> {
    toplevel()
        .parse(source)
        .into_result()
        .map(|v| v.into())
        .map_err(|err| {
            ParserError::SyntaxError(into_message(
                err.into_iter()
                    .next()
                    .expect("chumsky failed with zero errrors"),
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_program() {
        for source in [
            "",
            " ",
            "\n\n\n",
            "  \t \n  \r\n",
            "; just a comment\n",
            " ; leading space before the comment\n",
            ";; a\n;; b\n",
            " ; a\n \n ; b\n\n",
            "\n; a\n\n\n; b\n  ",
            ";;;",
            "\r\n; no trailing newline",
        ] {
            assert_eq!(
                Ok(Vec::new()),
                toplevel().parse(source).into_result(),
                "failed on {source:?}"
            );
        }
    }

    #[test]
    fn test_program_padding() {
        assert_eq!(
            Ok(vec![Syntax::Int(1.into())]),
            toplevel().parse(" ; c\n 1 ; trailing\n").into_result()
        );

        assert_eq!(
            Ok(vec![Syntax::Int(1.into()), Syntax::Int(2.into())]),
            toplevel().parse(" 1 \n\n ; between \n 2 ").into_result()
        );
    }

    #[test]
    fn test_program_errors() {
        for source in [
            ")",
            "[",
            "{",
            "'",
            "\"unterminated",
            " (1 2",
            " ; c\n (1 2",
            "(1 2 [3)]",
        ] {
            assert!(
                toplevel().parse(source).into_result().is_err(),
                "unexpectedly parsed {source:?}"
            );
        }
    }

    #[test]
    fn test_int() {
        assert_eq!(BigInt::from(0), int().parse("0").into_result().unwrap());

        assert_eq!(
            BigInt::from(1000),
            int().parse("1000").into_result().unwrap()
        );

        assert_eq!(BigInt::from(11), int().parse("+11").into_result().unwrap());
        assert_eq!(BigInt::from(-1), int().parse("-1").into_result().unwrap());
        assert_eq!(BigInt::from(1), int().parse("0001").into_result().unwrap());

        int().parse("").into_result().unwrap_err();
        int().parse("+").into_result().unwrap_err();
        int().parse("-").into_result().unwrap_err();
    }

    #[test]
    fn test_float() {
        assert_eq!(10., float().parse("10.").into_result().unwrap());
        assert_eq!(1., float().parse("1.").into_result().unwrap());
        assert_eq!(0.1, float().parse(".1").into_result().unwrap());
        assert_eq!(0.001, float().parse(".001").into_result().unwrap());
        assert_eq!(10.0E10, float().parse("10.E10").into_result().unwrap());
        assert_eq!(10.0e10, float().parse("10.e10").into_result().unwrap());
        assert_eq!(10.0e-20, float().parse("10.e-20").into_result().unwrap());
        assert_eq!(10.0e-02, float().parse("10.e-02").into_result().unwrap());
        assert_eq!(2.5, float().parse("0002.5").into_result().unwrap());

        float().parse("0001").into_result().unwrap_err();
        float().parse("").into_result().unwrap_err();
        float().parse("+").into_result().unwrap_err();
        float().parse("-").into_result().unwrap_err();
    }

    #[test]
    fn test_float_exponent() {
        assert_eq!(1e10, float().parse("1e10").into_result().unwrap());
        assert_eq!(1E5, float().parse("1E5").into_result().unwrap());
        assert_eq!(-1e10, float().parse("-1e10").into_result().unwrap());
        assert_eq!(1e10, float().parse("+1e10").into_result().unwrap());
        assert_eq!(1e-10, float().parse("1e-10").into_result().unwrap());
        assert_eq!(1e+10, float().parse("1e+10").into_result().unwrap());
        assert_eq!(1e-10, float().parse("1E-10").into_result().unwrap());
        assert_eq!(500., float().parse(".5e3").into_result().unwrap());
        assert_eq!(1e-10, float().parse("0001e-10").into_result().unwrap());

        // An exponent is not enough on its own: it needs digits to apply to.
        float().parse("1e").into_result().unwrap_err();
        float().parse("1e+").into_result().unwrap_err();
        float().parse("1e-").into_result().unwrap_err();
        float().parse("e10").into_result().unwrap_err();
    }

    #[test]
    fn test_int_is_not_a_float() {
        assert_eq!(
            Ok(vec![Syntax::Int(1.into())]),
            toplevel().parse("1").into_result()
        );
        assert_eq!(
            Ok(vec![Syntax::Int(1.into()), Syntax::Int(2.into())]),
            toplevel().parse("1 2").into_result()
        );

        float().parse("1").into_result().unwrap_err();
        float().parse("0001").into_result().unwrap_err();
    }

    #[test]
    fn test_string() {
        assert_eq!("", string().parse(r#""""#).into_result().unwrap().as_ref());

        assert_eq!(
            "h\\ello",
            string()
                .parse(r#""h\\ello""#)
                .into_result()
                .unwrap()
                .as_ref()
        );

        assert_eq!(
            "\r\n\t",
            string()
                .parse(r#""\r\n\t""#)
                .into_result()
                .unwrap()
                .as_ref()
        );

        assert_eq!(
            "\x1B",
            string().parse(r#""\x1B""#).into_result().unwrap().as_ref()
        );

        assert_eq!(
            "\u{1B}",
            string()
                .parse(r#""\u001B""#)
                .into_result()
                .unwrap()
                .as_ref()
        );

        assert_eq!(
            "\x1B",
            string()
                .parse(r#""\U0000001b""#)
                .into_result()
                .unwrap()
                .as_ref()
        );

        assert_eq!(
            "\u{1000}",
            string()
                .parse(r#""\u1000""#)
                .into_result()
                .unwrap()
                .as_ref()
        );

        assert_eq!(
            "\u{100000}",
            string()
                .parse(r#""\U00100000""#)
                .into_result()
                .unwrap()
                .as_ref()
        );

        string().parse(r#"""#).into_result().unwrap_err();
        string().parse(r#""\x0""#).into_result().unwrap_err();
        string().parse(r#""\u0""#).into_result().unwrap_err();
        string().parse(r#""\U0""#).into_result().unwrap_err();
        string()
            .parse(r#""unterminated with chars"#)
            .into_result()
            .unwrap_err();

        string().parse(r#""\""#).into_result().unwrap_err();
        string().parse(r#""\u123""#).into_result().unwrap_err();
        string().parse(r#""\x1G""#).into_result().unwrap_err();
        string().parse(r#""\U0011000G""#).into_result().unwrap_err();
        string().parse(r#""\UFFFFFFFF""#).into_result().unwrap_err();
    }

    #[test]
    fn test_symbol_like() {
        assert_eq!(
            Syntax::Symbol(Symbol::from("+")),
            symbol_like().parse("+").into_result().unwrap()
        );

        assert_eq!(
            Syntax::Symbol(Symbol::from("0001")),
            symbol_like().parse("0001").into_result().unwrap()
        );

        assert_eq!(
            Syntax::Symbol(Symbol::from("0something")),
            symbol_like().parse("0something").into_result().unwrap()
        );

        assert_eq!(
            Syntax::Symbol(Symbol::from("10something")),
            symbol_like().parse("10something").into_result().unwrap()
        );

        assert_eq!(
            Syntax::Bool(true),
            symbol_like().parse("true").into_result().unwrap()
        );

        assert_eq!(
            Syntax::Bool(false),
            symbol_like().parse("false").into_result().unwrap()
        );

        assert_eq!(
            Syntax::Symbol(Symbol::from("truey")),
            symbol_like().parse("truey").into_result().unwrap()
        );

        assert_eq!(
            Syntax::Symbol(Symbol::from("falsey")),
            symbol_like().parse("falsey").into_result().unwrap()
        );

        symbol_like().parse("").into_result().unwrap_err();
    }

    #[test]
    fn test_comment() {
        comment().parse("; hello world").into_result().unwrap();
    }

    #[test]
    fn test_value() {
        assert_eq!(
            Syntax::List(vec![]),
            value().parse("()").into_result().unwrap()
        );

        assert_eq!(
            Syntax::List(vec![
                Syntax::Int(1.into()),
                Syntax::Int(2.into()),
                Syntax::List(vec![Syntax::Int(3.into()),]),
            ]),
            value().parse("(1 2 (3))").into_result().unwrap(),
        );

        assert_eq!(
            Syntax::Vector(vec![
                Syntax::Int(1.into()),
                Syntax::Int(2.into()),
                Syntax::List(vec![Syntax::Int(3.into()),]),
            ]),
            value().parse("[1 2 (3)]").into_result().unwrap(),
        );

        assert_eq!(
            Syntax::List(vec![
                Syntax::Int(1.into()),
                Syntax::Int(2.into()),
                Syntax::Vector(vec![Syntax::Int(3.into()),]),
            ]),
            value().parse("(1 2 [3])").into_result().unwrap(),
        );

        assert_eq!(
            Syntax::List(vec![
                Syntax::Symbol(Symbol::new("define")),
                Syntax::List(vec![
                    Syntax::Symbol(Symbol::new("something")),
                    Syntax::Symbol(Symbol::new("x")),
                ]),
                Syntax::List(vec![
                    Syntax::Symbol(Symbol::new("+")),
                    Syntax::Symbol(Symbol::new("x")),
                    Syntax::Symbol(Symbol::new("x")),
                ]),
            ]),
            value()
                .parse("(define (something x) (+ x x))")
                .into_result()
                .unwrap()
        );

        assert_eq!(
            Syntax::List(vec![Syntax::Int(123.into())]),
            value().parse("(123)").into_result().unwrap(),
        );

        assert_eq!(
            Syntax::List(vec![Syntax::String("123".into())]),
            value().parse(r#"("123")"#).into_result().unwrap()
        );

        assert_eq!(
            Syntax::Symbol(Symbol::new("foo")),
            value().parse(r#"foo;bar"#).into_result().unwrap()
        );

        assert_eq!(
            Syntax::Int(1.into()),
            value().parse("0001").into_result().unwrap()
        );

        assert_eq!(
            Syntax::List(vec![Syntax::Int(1.into())]),
            value().parse("(0001)").into_result().unwrap()
        );

        value().parse("(1 2 3").into_result().unwrap_err();
        value().parse("(1 2 [3)]").into_result().unwrap_err();
    }

    #[test]
    fn test_toplevel() {
        assert_eq!(
            vec![Syntax::List(vec![
                Syntax::Symbol(Symbol::new("foo")),
                Syntax::Symbol(Symbol::new("bar"))
            ])],
            toplevel()
                .parse(
                    r#"(
                    foo
                    ; comment between elements
                    ; comment between elements
                    ; comment between elements
                    ; comment between elements
                    ; comment between elements
                    bar ; hello
                    ; barrrr
                    ;;;;;;;
                    )
                    ;
                    "#
                )
                .into_result()
                .unwrap()
        );

        assert_eq!(
            vec![Syntax::List(vec![
                Syntax::Symbol(Symbol::new("+")),
                Syntax::Int(1.into()),
                Syntax::Int(2.into()),
                Syntax::Int(3.into()),
            ])],
            toplevel().parse("   (+ 1 2 3)").into_result().unwrap(),
        );

        assert_eq!(
            vec![Syntax::List(vec![
                Syntax::Symbol(Symbol::new("+")),
                Syntax::Int(1.into()),
                Syntax::Int(2.into()),
                Syntax::Int(3.into()),
            ])],
            toplevel().parse("(+ 1 2 3)   ").into_result().unwrap()
        );

        assert_eq!(
            vec![Syntax::Int(1.into())],
            toplevel().parse("0001").into_result().unwrap()
        );
        // TODO more assert_eq! tests

        toplevel()
            .parse(" \t( \r\n1   2    3 ) ")
            .into_result()
            .unwrap();

        toplevel().parse("(+ 1 2 3)   (())").into_result().unwrap();
        toplevel().parse("+ 1 2 3 (())").into_result().unwrap();
    }
}
