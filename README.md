# miniscript

A small, simple, embeddable Lisp-like scripting language for Rust tools.
Compiles s-expressions to bytecode and runs them on a tiny VM.

> **Status: heavily in development.** Semantics shift often; see
> [BUGS.md](BUGS.md) and [TODO](TODO) for what's known-broken and what's next.

```lisp
; factorial, with a tail-recursive helper
(defn fact-iter [n acc]
  (if (<= n 1)
    acc
    (fact-iter (- n 1) (* acc n))))

(println (fact-iter 5 1))  ; => 120
```

## Language

Everything is an s-expression; calls are `(head arg ...)` in prefix position.

| Literals | Example |
|---|---|
| nil | `nil` |
| booleans | `true`, `false` |
| integers (arbitrary precision) | `42`, `-7` |
| floats | `2.5`, `1e10` |
| strings | `"hello"` |
| keywords | `:name` |
| vectors | `[1 2 3]` |
| tables | `{:name "bob" :age 30}` |
| quoted forms | `'(1 2 3)` |

```lisp
; bindings: defl (local), def (global), set (rebind), let (scoped)
(defl x 10)
(let [y (+ x 5)] (* y 2))  ; => 30

; functions and closures
(defl make-adder (fn [n] (fn [m] (+ n m))))
(defl add5 (make-adder 5))
(add5 3)  ; => 8

; named functions (defn) support self-recursion with TCO
(defn sum-to [n acc]
  (if (<= n 0) acc (sum-to (- n 1) (+ acc n))))

; control flow: if / when / cond / do
(cond (== x 0) "zero"
      (== x 1) "one"
      true     "many")

; tables and keywords
(defl m {:name "bob"})
(tget :name m)  ; => "bob"

; quote and eval
(eval '(+ 1 2))  ; => 3
```

Falsy values are `nil`, `false`, `0`, `""`, and empty vectors/tables/lists —
everything else is truthy.

The standard builtins are `print`, `println`, `str-append`, `str-repeat`,
`tget`, and `repr`.

## Running

```sh
cargo run -- program.lisp   # run a file (prints disassembly + result)
cargo test                  # test suite
cargo clippy --all-targets  # lints (kept clean)
```

With no file argument it runs `test.lisp`.

## Embedding

The pipeline is factored for embedding: parse to syntax, compile with a
`ProgramBuilder`, load the standard builtins, run.

```rust
let tree = parse(source)?;
let program = compile(ProgramBuilder::new(), tree)?;
load_standard_builtins(&mut program);
let value = program.run()?;
```

(`src/main.rs:program` has the full 5-line version.) Note the crate is
currently binary-only — exposing it as a library is on the way to making
"embeddable" literally true.
