mod builtin;
mod compile;
mod constant;
mod function;
mod global;
mod instruction;
mod keyword;
mod local;
mod parse;
mod program;
mod symbol;
mod util;
mod value;

use crate::builtin::load_standard_builtins;
use crate::compile::{CompileError, compile_syntax};
use crate::parse::{ParserError, parse};
use crate::program::ProgramBuilder;
use crate::program::{Program, ProgramError};
use std::fs;
use thiserror::Error;

// TODO rename this to ProgramError or something?
#[derive(Debug, Error)]
enum MainError {
    #[error("Parse error: {0}")]
    Parse(#[from] ParserError),
    #[error("Compile error: {0}")]
    Compile(#[from] CompileError),
    #[error("Program error: {0}")]
    Runtime(#[from] ProgramError),
}

fn program(file_content: &str) -> Result<Program, MainError> {
    let tree = parse(file_content)?;
    let builder = ProgramBuilder::new();
    let mut program = compile_syntax(builder, tree)?;
    load_standard_builtins(&mut program);
    Ok(program)
}

fn debug_run(file_content: &str) {
    println!("==== File content ====");
    println!("{file_content}\n");
    println!("======================");

    let program = match program(file_content) {
        Ok(program) => program,
        Err(err) => {
            println!("Error: {err}");
            return;
        }
    };

    if std::env::var_os("TRACE_DISASM").is_some() {
        let disassembly = program.disassemble();
        println!("==== Disassembly ====");
        println!("{disassembly}\n");
        println!("======================");
    }

    match program.run() {
        Ok(value) => println!("Ok: {value}"),
        Err(err) => {
            println!("Error: {}", err.err());

            for trace in err.backtrace() {
                println!("Trace: {}", trace);
            }
        }
    }
}

fn debug_run_file(file_name: &str) {
    let file_content = fs::read_to_string(file_name).unwrap();
    debug_run(&file_content);
}

fn main() {
    let mut read_something = false;

    for arg in std::env::args().skip(1) {
        debug_run_file(&arg);
        read_something = true;
    }

    if !read_something {
        debug_run_file("test.ms");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use num::BigInt;

    fn run(file_content: &str) -> Result<Value, MainError> {
        let program = program(file_content)?;
        Ok(program.run()?)
    }

    #[test]
    fn test_cmp() {
        assert_eq!(Value::new(true), run("(> 1 0)").unwrap());
        assert_eq!(Value::new(true), run("(< 1 2)").unwrap());
        assert_eq!(Value::new(true), run("(<= 1 1)").unwrap());
        assert_eq!(Value::new(true), run("(>= 1 1)").unwrap());
        assert_eq!(Value::new(true), run("(== true true)").unwrap());
        assert_eq!(Value::new(true), run("(!= true false)").unwrap());
        assert_eq!(Value::new(false), run("(not (>= 1 1))").unwrap());
    }

    #[test]
    fn test_cmp_int_float() {
        // ints and floats compare by numeric value, in both directions
        assert_eq!(Value::new(true), run("(== 1 1.0)").unwrap());
        assert_eq!(Value::new(true), run("(== 1.0 1)").unwrap());
        assert_eq!(Value::new(true), run("(== 0 0.0)").unwrap());
        assert_eq!(Value::new(true), run("(== -3 -3.0)").unwrap());
        assert_eq!(Value::new(false), run("(== 1 2.0)").unwrap());
        assert_eq!(Value::new(false), run("(!= 1 1.0)").unwrap());
        assert_eq!(Value::new(true), run("(!= 1 2.0)").unwrap());

        // equality agrees with the ordering operators
        assert_eq!(Value::new(true), run("(<= 2 2.0)").unwrap());
        assert_eq!(Value::new(true), run("(>= 2 2.0)").unwrap());
        assert_eq!(Value::new(false), run("(< 2 2.0)").unwrap());
        assert_eq!(Value::new(false), run("(> 2 2.0)").unwrap());
        assert_eq!(Value::new(true), run("(< 2 2.5)").unwrap());
        assert_eq!(Value::new(true), run("(> 3 2.5)").unwrap());

        // conversion recurses into containers
        assert_eq!(Value::new(true), run("(== [1 2] [1.0 2.0])").unwrap());
        assert_eq!(Value::new(false), run("(== [1 2] [1.0 2.5])").unwrap());

        // non-numeric types still only equal themselves
        assert_eq!(Value::new(true), run(r#"(== "1" "1")"#).unwrap());
        assert_eq!(Value::new(false), run(r#"(== "1" 1)"#).unwrap());
        assert_eq!(Value::new(false), run("(== nil 0)").unwrap());
        assert_eq!(Value::new(false), run("(== true 1)").unwrap());
    }

    #[test]
    fn test_if() {
        assert_eq!(Value::new(1), run("(if (> 1 0) 1 0)").unwrap());

        assert_eq!(
            Value::new(3),
            run("(if (> 2 1) (if (< 1 2) 3 4) 5)").unwrap()
        );

        assert_eq!(
            Value::new(5),
            run("(if (> 1 2) (if (< 1 2) 3 4) 5)").unwrap()
        );
    }

    #[test]
    fn test_defl() {
        assert_eq!(Value::new(1), run("(defl x 1) x").unwrap());
        assert_eq!(Value::new(3), run("(defl x 1) (defl y 2) (+ x y)").unwrap());

        assert_eq!(Value::new(7), run("(+ (defl x 5) 2)").unwrap());
    }

    #[test]
    fn test_set() {
        assert_eq!(Value::new(1), run("(set x 1) x").unwrap());
        assert_eq!(Value::new(3), run("(set x 1) (set y 2) (+ x y)").unwrap());
        assert_eq!(Value::new(9), run("(+ (set x 4) (set y 5))").unwrap());
        assert_eq!(Value::new(11), run("(set x 10) (set x (+ x 1)) x").unwrap());

        assert_eq!(
            Value::new(24),
            run(r#"
                (set fact 1)
                (set n 2)
                (set fact (* fact n))
                (set n 3)
                (set fact (* fact n))
                (set n 4)
                (set fact (* fact n))
                fact
            "#)
            .unwrap()
        );
    }

    #[test]
    fn test_mixed_defls_set() {
        assert_eq!(Value::new(3), run("(defl x 1) (set y 2) (+ x y)").unwrap());

        assert_eq!(Value::new(11), run("(let [x 1] (+ x (set x 10)))").unwrap());
    }

    #[test]
    fn test_variadic_arith() {
        assert_eq!(Value::new(6), run("(+ 1 2 3)").unwrap());
        assert_eq!(Value::new(0), run("(- 10 4 6)").unwrap());
        assert_eq!(Value::new(1), run("(/ 24 3 8)").unwrap());
        assert_eq!(Value::new(7), run("(- 10 3)").unwrap());
        assert_eq!(Value::new(3), run("(/ 6 2)").unwrap());
        assert_eq!(Value::new(24), run("(* 2 3 4)").unwrap());
        run("(+ )").unwrap_err();
    }

    #[test]
    fn test_unary_arith() {
        assert_eq!(Value::new(-5), run("(- 5)").unwrap());
        assert_eq!(Value::new(0.25), run("(/ 4.0)").unwrap());
        assert_eq!(Value::new(2.0), run("(- -2.0)").unwrap());
    }

    #[test]
    fn test_float_mixed() {
        assert_eq!(Value::new(3.5), run("(+ 1 2.5)").unwrap());
        assert_eq!(Value::new(1.5), run("(/ 3 2.0)").unwrap());
        assert_eq!(Value::new(2.5), run("(- 5 2.5)").unwrap());
        assert_eq!(Value::new(true), run("(< 1.5 2.5)").unwrap());
        assert_eq!(Value::new(false), run("(== 1 2.5)").unwrap());
    }

    #[test]
    fn test_bigint() {
        let big: BigInt = "123456789012345678901234567890".parse().unwrap();

        assert_eq!(
            Value::new(big.clone()),
            run("(+ 123456789012345678901234567890 0)").unwrap()
        );

        assert_eq!(
            Value::new(&big * &BigInt::from(2)),
            run("(* 123456789012345678901234567890 2)").unwrap()
        );

        assert_eq!(
            Value::new(-big),
            run("(- 123456789012345678901234567890)").unwrap()
        );
    }

    #[test]
    fn test_string_builtins() {
        assert_eq!(
            Value::new("foobar"),
            run("(str-append \"foo\" \"bar\")").unwrap()
        );

        assert_eq!(Value::new("ababab"), run(r#"(str-repeat "ab" 3)"#).unwrap());
        assert_eq!(Value::new(""), run(r#"(str-repeat "a" 0)"#).unwrap());

        assert_eq!(
            Value::new("ab"),
            run("(str-append (str-repeat \"a\" 1) (str-repeat \"b\" 1))").unwrap()
        );
    }

    #[test]
    fn test_truthiness() {
        assert_eq!(Value::new(2), run("(if 0 1 2)").unwrap());
        assert_eq!(Value::new(2), run(r#"(if "" 1 2)"#).unwrap());
        assert_eq!(Value::new(1), run(r#"(if "x" 1 2)"#).unwrap());
        assert_eq!(Value::new(2), run("(if nil 1 2)").unwrap());
        assert_eq!(Value::new(1), run("(if 1 1 2)").unwrap());
        assert_eq!(Value::new(2), run("(if false 1 2)").unwrap());
        assert_eq!(Value::new(1), run("(if true 1 2)").unwrap());
    }

    #[test]
    fn test_not() {
        assert_eq!(Value::new(true), run("(not false)").unwrap());
        assert_eq!(Value::new(false), run("(not true)").unwrap());
        assert_eq!(Value::new(true), run("(not 0)").unwrap());
        assert_eq!(Value::new(false), run("(not 1)").unwrap());
        assert_eq!(Value::new(true), run("(not (not 5))").unwrap());
    }

    #[test]
    fn test_let() {
        assert_eq!(Value::new(3), run("(let [x 1 y 2] (+ x y))").unwrap());
        assert_eq!(
            Value::new(30),
            run("(let [x 1 y 2] (* (+ x y) 10))").unwrap()
        );

        assert_eq!(Value::new(2), run("(let [x 1] (let [x 2] x))").unwrap());
        assert_eq!(Value::new(1), run("(let [x 2] (let [x 1] x))").unwrap());
        assert_eq!(Value::new(1), run("(let [x 1] (let [x x] x))").unwrap());

        assert_eq!(
            Value::new(11),
            run("(let [x 1] (+ x (let [x 10] x)))").unwrap()
        );

        assert_eq!(
            Value::new(3),
            run("(let [x 1] (* x (let [y 2] (+ y 1))))").unwrap()
        );

        assert_eq!(Value::new(2), run("(let [x 1 y 2] (+ x (/ y 2)))").unwrap());

        assert_eq!(Value::new(1), run("(defl x 1) (let [x 2] x) x").unwrap());
        assert_eq!(Value::new(1), run("(set x 1) (let [x 2] x) x").unwrap());
    }

    #[test]
    fn test_multiple_expressions() {
        assert_eq!(Value::new(7), run("(+ 1 2) (+ 3 4)").unwrap());
        assert_eq!(Value::new(5), run("1 2 3 4 5").unwrap());
    }

    #[test]
    fn test_fn_args() {
        assert_eq!(
            Value::new(7),
            run("(defl sub (fn [a b] (- a b))) (sub 10 3)").unwrap()
        );

        assert_eq!(
            Value::new(4),
            run("(defl mix (fn [a b c] (- (+ a b) c))) (mix 3 4 3)").unwrap()
        );
    }

    #[test]
    fn test_closure_calls_closure_upvalue() {
        let source = r#"
            (defl test "hello world")

            (defl fun
              (fn []
                (str-append test "!")))

            (defl main
              (fn []
                (fun)))

            (main)
        "#;

        assert_eq!(Value::new("hello world!"), run(source).unwrap());
    }

    #[test]
    fn test_upvalue_set_shared_state() {
        let source = r#"
            (defl x 1)
            (defl bump
              (fn []
                (set x (+ x 1))
                x))
            (bump)
            (bump)
            x
        "#;

        assert_eq!(Value::new(3), run(source).unwrap());
    }

    #[test]
    fn test_returned_closure_escapes_scope() {
        let source = r#"
            (defl make-adder
              (fn [n]
                (fn [m] (+ n m))))
            (defl add5 (make-adder 5))
            (add5 3)
        "#;

        assert_eq!(Value::new(8), run(source).unwrap());
    }

    #[test]
    fn test_nested_closure_calls() {
        let source = r#"
            (defl base 10)
            (defl f (fn [] base))
            (defl g (fn [] (f)))
            (defl h (fn [] (g)))
            (h)
        "#;

        assert_eq!(Value::new(10), run(source).unwrap());
    }

    #[test]
    fn test_tco_non_tail_recursion() {
        let source = r#"
            (defn f [n]
              (if (<= n 0)
                0
                (+ 1 (f (- n 1)))))
            (f 5)
        "#;

        assert_eq!(Value::new(5), run(source).unwrap());
    }

    #[test]
    fn test_compound_forms_preserve_non_tail_work() {
        for nested_call in [
            "(do (f (- n 1)))",
            "(let [] (f (- n 1)))",
            "(when true (f (- n 1)))",
            "(cond true (f (- n 1)))",
        ] {
            let source = format!("(defn f [n] (if (== n 0) 0 (+ 1 {nested_call}))) (f 3)");
            assert_eq!(Value::new(3), run(&source).unwrap(), "{nested_call}");
        }
    }

    #[test]
    fn test_tco_deep_tail_recursion() {
        let source = r#"
            (defn sum-to [n acc]
              (if (<= n 0)
                acc
                (sum-to (- n 1) (+ acc n))))
            (sum-to 100000 0)
        "#;

        assert_eq!(Value::new(5000050000i64), run(source).unwrap());
    }

    #[test]
    fn test_tco_tail_recursion_in_let() {
        let source = r#"
            (defn f [n]
              (let [m (- n 1)]
                (if (<= n 0) 7 (f m))))
            (f 100000)
        "#;

        assert_eq!(Value::new(7), run(source).unwrap());
    }

    #[test]
    fn test_tco_mutual_recursion() {
        let source = r#"
            (defn even? [n]
              (if (== n 0) true (odd? (- n 1))))
            (defn odd? [n]
              (if (== n 0) false (even? (- n 1))))
        "#;

        assert_eq!(
            Value::new(false),
            run(&format!("{source} (even? 21)")).unwrap()
        );

        assert_eq!(
            Value::new(true),
            run(&format!("{source} (odd? 21)")).unwrap()
        );
    }

    #[test]
    fn test_tco_sibling_closure_upvalues() {
        let source = r#"
            (defn make-counter [n]
              (fn [step]
                (if (<= step 0)
                  n
                  ((make-counter (+ n 1)) (- step 1)))))
            ((make-counter 0) 3)
        "#;

        assert_eq!(Value::new(3), run(source).unwrap());
    }

    #[test]
    fn test_table_literals() {
        assert_eq!(Value::new(true), run("(== {1 2} {1 2})").unwrap());
        assert_eq!(Value::new(2), run("(tget 1 {1 2})").unwrap());
        assert_eq!(Value::new(true), run("(== {} {})").unwrap());
        // keys may be computed expressions
        assert_eq!(Value::new(2), run("(tget (+ 1 0) {(+ 1 0) 2})").unwrap());
        // tables round-trip through eval
        assert_eq!(Value::new(true), run("(== (eval {1 2}) {1 2})").unwrap());
        // the explicit call form still works
        assert_eq!(Value::new(true), run("(== (table 1 2) {1 2})").unwrap());
        run("(table 1)").unwrap_err();
    }

    #[test]
    fn test_nested_upvalues() {
        // two-level escape captures the defining cell
        assert_eq!(
            Value::new(1),
            run("(defl m (fn [a] (fn [b] (fn [c] a)))) (defl f ((m 1) 2)) (f 3)").unwrap()
        );

        assert_eq!(
            Value::new(4),
            run("(defl m (fn [a] (fn [b] (fn [c] (+ a c))))) (defl f ((m 1) 2)) (f 3)").unwrap()
        );

        // mutation through two levels shares the cell
        assert_eq!(
            Value::new(3),
            run("(defl m (fn [a] (fn [b] (fn [] (set a (+ a 1)) a)))) (defl f ((m 1) 2)) (f) (f)")
                .unwrap()
        );

        // shadowing across levels resolves to the nearest binding
        assert_eq!(
            Value::new(2),
            run("(defl m (fn [x] (fn [x] x))) ((m 1) 2)").unwrap()
        );
    }

    #[test]
    fn test_closure_identity() {
        // closures compare by identity: distinct closures are never equal
        assert_eq!(
            Value::new(false),
            run("(defnl f [n] (let [g (fn [] n)] g)) (== (f 1) (f 1))").unwrap()
        );

        // a closure is always equal to itself
        assert_eq!(
            Value::new(true),
            run("(defnl f [n] (let [g (fn [] n)] g)) (defl a (f 1)) (== a a)").unwrap()
        );

        let cyclic = r#"
            (defn m []
              (let [g nil]
                (set g (fn [] g)) g))

            (defl a (m))
            (defl b (m))
        "#;

        // distinct self-referential cycles terminate
        assert_eq!(
            Value::new(false),
            run(&format!("{cyclic} (== a b)")).unwrap()
        );

        assert_eq!(
            Value::new(true),
            run(&format!("{cyclic} (== a a)")).unwrap()
        );

        // including nested in containers
        assert_eq!(
            Value::new(false),
            run(&format!("{cyclic} (== [a] [b])")).unwrap()
        );

        run(&format!("{cyclic} (< a b)")).unwrap();
    }

    #[test]
    fn test_keywords() {
        assert_eq!(Value::new(true), run("(== :a :a)").unwrap());
        assert_eq!(Value::new(false), run("(== :a :b)").unwrap());
        assert_eq!(Value::new(true), run("(== (keyword foo) :foo)").unwrap());
    }

    #[test]
    fn test_program() {
        let source = r#"
            ; A whole program mixing everything
            (set total 0)
            (defl a 2)
            (defl b 3)
            (set total (+ a b a))
            (if (== total 7) "seven" "wrong")
        "#;

        assert_eq!(Value::new("seven"), run(source).unwrap());

        let source = r#"
            (set x 10)
            (set y (/ x 2))
            (if (== (* y 2) x) "even" "odd")
        "#;

        assert_eq!(Value::new("even"), run(source).unwrap());

        let source = r#"
            (let [name "world"]
                (if (not (== (str-repeat name 0) "world"))
                    (str-append "hello " name)
                    "empty"))
        "#;

        assert_eq!(Value::new("hello world"), run(source).unwrap());
    }
}
