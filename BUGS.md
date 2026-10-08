# BUGS

## Critical

### 1. Self-referential closures form a reference cycle that is never freed
`src/value.rs:57-61`, `src/value.rs:113-115`

Closures capture upvalue cells by shared `Rc` (`src/program.rs:594-602`), and
`set` through an upvalue writes the closure back into its own cell:

```
(defn m [] (let [g nil] (set g (fn [] g)) g))
```

Each `(m)` leaks a closure ↔ cell cycle. (Comparing such closures no longer
overflows: `Closure`/`Cell` use identity equality — distinct cycles compare
`false`, a closure always equals itself.)

Note: the old trigger `(defnl f [] (let [g (fn [] g)] g)) (== (f) (f))` never
formed a cycle — `let` inserts the name *after* compiling the initializer, so
the inner `g` resolves to a global and the closure captures nothing.

## Medium — `eval`

`src/program.rs:625-644` builds a fresh `Program`, copies the outer
`GlobalStore` in, runs it, and drops it.

- **Closures cannot be called.** `Value::Closure` carries a `proto_index` into
  the outer program's `protos`; the eval'd program's table is empty.
  `(defn f [] 42) (eval '(f))` => `Invalid function referenced: 0`.

- **Keywords from `eval` can alias unrelated outer keywords.** `src/program.rs:655-665`,
  `src/keyword.rs:25-48`. `eval` compiles in a fresh program, while `Keyword`
  equality and hashing use only the per-program intern-table index.  Thus an
  outer `:foo` and eval'd `:bar` can both have index 0:

  ```
  (tget (eval ':bar) {:foo 1})  => 1; expected nil
  ```

- **`eval` rejects builtins (and containers holding them) with `UncompileableValue`.**
  `src/compile.rs:540-550` errors on `BuiltinFunction`/`Closure` in
  `compile_value_quoted`; `src/program.rs:655-666` surfaces it as
  `RuntimeError::Compile`. Distinct from the proto-index staleness above.

  ```
  (eval print)  => `Compilation error: Uncompileable value`, while `(eval 1)` works
  ```

## Medium

- **Mixed integer/float comparisons lose integer precision.** `src/value.rs:189-231`.
  The implementation converts `BigInt` to `f64`, making distinct integers past
  f64's exact integer range compare equal to the same float:

  ```
  (== 9007199254740993 9007199254740992.0)  => true; expected false
  ```

- **Table key lookup conflicts with numeric equality.** `src/value.rs:126-156`.
  `(== 1 1.0)` is true, but `ValueKey` keeps integers and floats in distinct
  variants (and therefore distinct hash/equality classes):

  ```
  (tget 1.0 {1 "x"})  => nil; expected "x" if table keys follow `==`
  ```

- **Mixed integer/float *arithmetic* also loses precision** (above entry covers only
  comparisons). `src/value.rs:339-340,361-362,383-384,419-433` all funnel through
  `bigint_to_f64`:

  ```
  (+ 9007199254740993 0.0)  => 9007199254740992.0
  ```

- **Special forms cannot be shadowed by locals.** `src/compile.rs:141-151`
  `resolve_call_target` checks the head symbol against the special-form table
  before scope resolution, so a local named `+`, `if`, `let`, `fn`, etc. is
  ignored in call position:

  ```
  (let [+ 100] (+ 1 2))  => 3; local `+` is dead
  ```

- **Only self-recursion gets TCO — deep mutual/differing-chunk tail calls hit
  `RecursionTooDeep`.** `src/program.rs:445` (`ptr::eq(target_chunk, chunk)`)
  fast-paths only same-chunk calls; all other `PossibleTailCall`s recurse with
  `call_depth + 1` (`:476-482`):

  ```
  (even? 10000) with standard even?/odd? pair  => `Maximum recursion depth exceeded`,
  despite both calls being in tail position
  ```

- **Keyword ordering is interning order, not lexicographic.**
  `src/keyword.rs:39-49` orders/hashes by `index`; `src/value.rs:226,232`
  inherit it for `<`/`>` comparisons:

  ```
  (> :b :a)  => false when `:b` is interned first (program text order decides)
  ```

- **`NaN == NaN` is true; NaN sorts above everything.** `src/value.rs:195`
  (`Float` eq via `OrderedFloat`) and `:219` (total-order `cmp`). `OrderedFloat`
  gives NaN total equality, unlike IEEE `==`:

  ```
  (defl i (+ 1e308 1e308)) (== (- i i) (- i i))  => true
  ```

## Low

- **`num_locals` never reclaimed** — `src/local.rs:51-68`. `insert` bumps the
  counter, `pop_scope` never decrements, so every `let` site permanently consumes
  a frame slot (three sibling `let`s => `num_locals == 3`). `pop_scope` can also
  remove the base scope, after which `top_mut`'s `unwrap()` panics.
- **`str_repeat` with a negative count** — `src/builtin.rs:90-93` reports
  `Integer too large` for `-1`.
- **`arith_op(0)` silently pushes nothing** — `src/program.rs:226-228`.
  Unreachable from the compiler today; would unbalance the stack if reached.
- **`load_local` takes `&mut Cell` and holds a `Ref` across a `Vec::push`** —
  `src/program.rs:976-981`. Should be `&Cell`.
- **`Value::String` prints unquoted** — `src/value.rs:265`.
  `(print "a" 'a)` => `a a`.
- **`ariadne` declared but never imported** (`Cargo.toml:6`); `into_message`
  (`src/parse.rs:103`) keeps only `err.reason()`, discarding the span.
- **`fs::read_to_string(...).unwrap()`** — `src/main.rs:56` panics with a raw
  `Os` error on a missing file.
- **Typos in user-visible output**: `Runtme` (`src/main.rs:30`), `Mimsatched`
  (`src/program.rs:108`), `LOCAlS` (`src/program.rs:270`, disassembler header);
  also `errrors` (`src/parse.rs:305`), `substract` (`src/instruction.rs:21`),
  `reciprocial` (`src/instruction.rs:25`).
- **Corrupt/truncated bytecode halts silently instead of erroring** —
  `src/instruction.rs:160-167` returns `None` for bad opcodes/short operands, and
  `src/program.rs:319` (`while let Some(insn)`) then just exits the loop and
  returns `pop().unwrap_or(Nil)`. `jump_forward` (`src/instruction.rs:128-130`)
  uses `saturating_add`, clamping wild jumps to `usize::MAX` → same silent halt.
  Unreachable from today's compiler, but any malformed stream yields a bogus
  `Nil`/partial result instead of an error. (Related:
  `// XXX TODO invalid opcodes should be an error`.)
- **`patch_jump` underflows on any backward target** —
  `src/instruction.rs:396-400`: `target - operand_off - size_of::<usize>()`
  panics in debug (wraps in release, then `copy_from_slice` may panic) for
  `target < operand_off + 8`. Safe today (all jumps forward), but the
  `TODO backward jumps` path would hit it immediately.
- **`Span` truncates offsets to `u32`** — `src/debug.rs:8-13`. Inputs over 4 GiB
  silently corrupt debug spans.
