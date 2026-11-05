# MAGIC test matrix

129 tests, all green. Run with `cargo test`. Every row names the file that owns the test and
what it proves, so the docs can link straight to it.

## Added after v0.3

| Test | File | Proves |
|---|---|---|
| `small_programs_stay_single_file` | `tests/emit.rs` | <32 functions → 1 translation unit |
| `large_programs_split_into_chunks` | `tests/emit.rs` | 40 functions → 4 units, main once, callee prototype present |
| `arena_uses_one_allocation` | `src/codegen.rs` | one `malloc`, offsets 0/16, `free(magic_arena)` |
| `return_inside_arena_is_rejected` | `src/codegen.rs` | bump pointers never reach `free` |
| `arena_with_runtime_size_falls_back` | `src/codegen.rs` | param-sized array → per-array malloc + `free(a)` |
| `arena_const_folds_expression_sizes` | `src/codegen.rs` | `let n = 2 * 3` folds → `malloc(24)` |
| `arena_aligns_mixed_element_sizes` | `src/codegen.rs` | u8 then f64 → offsets 0 and 8, total 24 |
| `sqrt_is_builtin` | `src/codegen.rs` | `math.h` included, `sqrt(...)` emitted |
| `print_prec_formats_floats` | `src/codegen.rs` | `printf("%.*f\n", (int)(3), (double)(1.5))` |
| `casts_emit_c_casts` | `src/codegen.rs` | `3.9 as i32` emits `((int32_t)(3.9))` |
| `assert_emits_panic` | `src/codegen.rs` | `assert` emits `if (!(cond)) magic_panic("line N: assertion failed")` |
| `for_loops_emit_c_for` | `src/codegen.rs` | `for i in 0..3` emits `for (int32_t i = 0; i < 3; i++)` |
| `break_and_continue_emit` | `src/codegen.rs` | `break;` and `continue;` emitted |
| `len_is_builtin` | `src/codegen.rs` | `len(a)` emits `a_len` as `%lld` |
| `vec_push_emits_helper` | `src/codegen.rs` | `magic_push`, `v_len`, `v_cap` emitted |
| `struct_parameters_pass_by_value` | `src/codegen.rs` | `int32_t px(Point p)` emitted |
| `struct_return_values` | `src/codegen.rs` | `Point make(void)` and `Point p = make();` |
| `str_eq_uses_strcmp` | `src/codegen.rs` | `str_eq` emits `strcmp(...) == 0` |
| `str_len_uses_strlen` | `src/codegen.rs` | `str_len` emits `strlen(...)` |
| `abs_emits_ternary` | `src/codegen.rs` | `abs` emits `x < 0 ? -x : x` |
| `min_max_emit_ternaries` | `src/codegen.rs` | `min`/`max` emit `<`/`>` ternaries |
| `vec_parameter_signature_and_index` | `src/codegen.rs` | `T **v, int64_t *v_len, int64_t *v_cap` and `(*v)[magic_index(...)]` |
| `vec_parameter_push_uses_realloc_pointer` | `src/codegen.rs` | `magic_push((void**)v, v_len, v_cap)` in a block |
| `struct_arrays_emit_struct_pointer` | `src/codegen.rs` | `Point *a` and `].x = 5;` |
| `pop_emits_helper` | `src/codegen.rs` | `magic_pop` for locals and `magic_pop(v_len` for params |
| `field_paths_emit_chained_access` | `src/codegen.rs` | `o.i.a = 5;` emitted with chained access |
| `index_field_paths_emit_chained_access` | `src/codegen.rs` | `o[0].i.a = 5;` emitted with chained access |
| `reserve_emits_helper` | `src/codegen.rs` | `magic_reserve` for locals and params |
| `clear_emits_len_zero` | `src/codegen.rs` | `(v_len = 0)` and `(*v_len = 0)` |
| `struct_vecs_and_params` | `src/codegen.rs` | `P **v, int64_t *v_len, int64_t *v_cap` and `v[i].x` |
| `reports_arena_bound_separately` | `tests/emit.rs` | function total plus `main arena at line 1` row |
| `reports_one_row_per_arena` | `tests/emit.rs` | two arenas → total 20 and rows of 8 and 12 |
| `reports_bounds_for_expression_sizes` | `tests/emit.rs` | `let n = 4 * 4; [0; n]` → 64 bytes |
| `reports_struct_array_sizes` | `tests/emit.rs` | `[P; 10]` with two i32 fields → 80 bytes |
| `reports_nested_struct_sizes` | `tests/emit.rs` | `Outer { Inner, i64 }` → 160 bytes for 10 |
| `reports_struct_padding` | `tests/emit.rs` | `{ u8, i64 }` pads to 16 bytes |
| `all_examples_compile` | `tests/emit.rs` | every one of the 14 examples still compiles |
| `examples_produce_expected_output` | `tests/e2e.rs` | all 14 examples run and print the documented output |

`arena_frees_at_block_end` now asserts the bump path (constant-folded `let n`).

## Unit — lexer

| Test | File | Proves |
|---|---|---|
| `lexes_keywords_and_ops` | `src/lexer.rs` | keywords, `->`, EOF |
| `tracks_line_numbers` | `src/lexer.rs` | diagnostics get real line numbers |

## Unit — parser

| Test | File | Proves |
|---|---|---|
| `parses_fib` | `src/parser.rs` | functions, params, return types |
| `precedence` | `src/parser.rs` | `1 + 2 * 3` nests as `1 + (2*3)` |

## Unit — codegen / type checker

| Test | File | Proves |
|---|---|---|
| `hello_emits_a_c_program` | `src/codegen.rs` | full pipeline emits valid C |
| `fib_emits_recursive_c` | `src/codegen.rs` | recursion reaches the emitter |
| `rejects_unknown_function` | `src/codegen.rs` | call checking |
| `rejects_immutable_assignment` | `src/codegen.rs` | `let` is immutable |
| `arrays_emit_bounds_checked_c` | `src/codegen.rs` | `malloc`, `magic_index`, `free` emitted |
| `array_as_value_is_rejected` | `src/codegen.rs` | arrays are not values |
| `structs_emit_c` | `src/codegen.rs` | struct typedef + compound literal |
| `struct_type_errors` | `src/codegen.rs` | wrong/missing/unknown field, immutable field |
| `array_params_emit_pointer_and_length` | `src/codegen.rs` | `int32_t *xs, int64_t xs_len` |
| `arena_frees_at_block_end` | `src/codegen.rs` | free emitted inside the block, pointer nulled |
| `nested_arenas_are_rejected` | `src/codegen.rs` | nesting is a compile error |
| `arrays_after_arena_are_out_of_scope` | `src/codegen.rs` | use after `}` is a compile error |

## Integration — source to C (`tests/emit.rs`)

| Test | File | Proves |
|---|---|---|
| `hello_pipeline` | `tests/emit.rs` | includes + `int main(void)` + literal |
| `loops_and_mutation` | `tests/emit.rs` | `while`, `i64` locals |
| `type_errors_are_rejected` | `tests/emit.rs` | bool/int, wrong arg, missing main |
| `string_escapes_survive_codegen` | `tests/emit.rs` | `\n`, `\"` round-trip |
| `memory_report_sums_constant_arrays` | `tests/emit.rs` | `[i32] = [0; n]`, n=10 → 40 bytes |
| `memory_report_ignores_dynamic_sizes` | `tests/emit.rs` | runtime size → 0, never a guess |

## End-to-end — compile with gcc, run, compare stdout (`tests/e2e.rs`)

| Test | File | Proves | Needs gcc |
|---|---|---|---|
| `hello_prints` | `tests/e2e.rs` | pipeline produces `e2e` | yes |
| `array_parameter_sums` | `tests/e2e.rs` | `fn sum(xs: [i32], n)` → `100` | yes |
| `struct_field_roundtrip` | `tests/e2e.rs` | struct literal + field write → `8\n35` | yes |
| `arena_prints_inside_and_after` | `tests/e2e.rs` | arena frees at `}` → `5\nafter` | yes |
| `out_of_bounds_exits_one` | `tests/e2e.rs` | OOB read → exit 1, stderr names line | yes |
| `sqrt_prints_three` | `tests/e2e.rs` | `sqrt(9.0)` → `3` | yes |
| `cast_truncates_float_to_int` | `tests/e2e.rs` | `3.9 as i32` → `3` | yes |
| `assert_passes` | `tests/e2e.rs` | true assertion continues → `ok` | yes |
| `assert_fails_with_exit_one` | `tests/e2e.rs` | false assertion → exit 1, line named | yes |
| `for_loop_sums` | `tests/e2e.rs` | `for i in 0..5` sums to `10` | yes |
| `break_and_continue_work` | `tests/e2e.rs` | skip 2, stop at 6 → `13` | yes |
| `break_outside_loop_is_rejected` | `tests/e2e.rs` | top-level `break` is a compile error | no |
| `break_inside_arena_is_rejected` | `tests/e2e.rs` | bump pointers must not escape the block | no |
| `len_returns_array_length` | `tests/e2e.rs` | `len([7; 5])` → `5` | yes |
| `vec_push_and_index` | `tests/e2e.rs` | push 1,2,3 → `3` and `6` | yes |
| `vec_grows_past_initial_capacity` | `tests/e2e.rs` | 100 pushes → `100` and `99` | yes |
| `struct_parameter_reads_field` | `tests/e2e.rs` | struct arg by value → `7` | yes |
| `struct_return_roundtrip` | `tests/e2e.rs` | returned struct → `11` / `22` | yes |
| `str_eq_compares_contents` | `tests/e2e.rs` | `str_eq("abc","abc")` → `true`, `"a"` vs `"b"` → `false` | yes |
| `str_len_counts_bytes` | `tests/e2e.rs` | `str_len("hello")` → `5` | yes |
| `str_byte_reads_and_bounds_checks` | `tests/e2e.rs` | `str_byte("ABC",1)` → `66`; index 5 → exit 1 | yes |
| `str_byte_n_uses_the_given_length` | `tests/e2e.rs` | `str_byte_n("hello",1,5)` → `101`; index 3 of 3 → exit 1 | yes |
| `abs_works_for_int_and_float` | `tests/e2e.rs` | `abs(-5)` → `5`, `abs(-2.5)` → `2.5` | yes |
| `min_and_max_work` | `tests/e2e.rs` | `min(3,7)` → `3`, `max(3,7)` → `7`, float pair → `1.5`/`2.5` | yes |
| `min_max_work_for_u8_and_i64` | `tests/e2e.rs` | u8 pair → `5`/`9`; i64 pair → `3000000000`/`5000000000` | yes |
| `abs_works_for_narrow_and_wide_types` | `tests/e2e.rs` | `abs(u8 5)` → `5`; `abs(i64 -5e9)` → `5000000000` | yes |
| `vec_parameter_sums` | `tests/e2e.rs` | `[i32+]` parameter sums to `7` | yes |
| `vec_parameter_can_push` | `tests/e2e.rs` | callee pushes; caller sees `2` and `11` | yes |
| `struct_array_reads_and_writes_fields` | `tests/e2e.rs` | `a[0].x = 7` → `7`, `3`, `0` | yes |
| `nested_struct_field_roundtrip` | `tests/e2e.rs` | `Outer { i: Inner { a: 3, b: 4 } }` → `3`, `4`, `9` | yes |
| `chained_field_access_works` | `tests/e2e.rs` | `o.i.a` read/write → `3`, `7`, `9` | yes |
| `index_field_path_roundtrip` | `tests/e2e.rs` | `os[1].i.a = 42` → `42`, `1`, `3` | yes |
| `reserve_avoids_reallocation` | `tests/e2e.rs` | reserve 1000, push 100 → `100`, `99` | yes |
| `clear_resets_length` | `tests/e2e.rs` | push 2 → len 2, clear → 0, push 9 → `9` | yes |
| `struct_vec_roundtrip` | `tests/e2e.rs` | `[P+]` push in callee and caller → `2`, `4`, `8` | yes |
| `struct_vec_pop_returns_struct` | `tests/e2e.rs` | `pop` returns the last `P` → `3`, `4`, len `1` | yes |
| `pop_removes_last_element` | `tests/e2e.rs` | pop → `3`, len `2`, pop → `2`, len `1` | yes |
| `pop_on_vec_parameter_updates_caller` | `tests/e2e.rs` | callee pops → `9`, caller len `0` | yes |
| `negative_index_exits_one` | `tests/e2e.rs` | signed index rejected by unsigned compare | yes |

## CLI — the real binary (`tests/cli.rs`)

These run `magicc` itself, so argument parsing, the build cache and `--report` are covered.

| Test | What it proves | Needs gcc |
|---|---|---|
| `check_accepts_a_valid_program` | exit 0 and prints `ok` | no |
| `version_flag_prints_the_package_version` | `--version` prints the crate version | no |
| `help_flag_prints_usage` | `--help` prints usage and flags | no |
| `check_rejects_a_missing_file` | non-zero exit | no |
| `check_reports_the_memory_bound` | `--report` prints `40` for ten `i32`s | no |
| `report_counts_struct_arrays` | `--report` prints `80` for ten two-field structs | no |
| `report_shows_arena_rows` | `--report` prints a per-arena row with `16` bytes | no |
| `builds_several_source_files_as_one_program` | `magicc build main.magic lib.magic` → `42` | yes |
| `cc_env_may_include_arguments` | `CC="gcc -O1"` still builds and runs → `42` | yes |
| `imports_are_inlined` | `import "lib.magic";` then `helper()` → `18` | yes |
| `missing_import_fails` | missing import → build fails, `cannot read` | yes |
| `errors_point_at_the_imported_file` | type error in `lib.magic` → `lib.magic:2:` | yes |
| `cc_flag_overrides_cc_env` | bogus `CC`, `--cc "gcc -O1"` still builds | yes |
| `output_flag_names_the_binary` | `-o custom.exe` writes that path and it runs | yes |
| `duplicate_function_names_the_file` | second `helper` reported as `b.magic:1` | yes |
| `unroll_flag_emits_straight_line_and_runs` | `--unroll` removes the loop, `break` keeps it | yes |
| `unroll_warns_for_gcc` | `--unroll` with gcc prints a note | yes |
| `version_flags_are_interchangeable` | `-v`, `-version`, `-V`, `--version`, `version` all print it | yes |
| `else_if_chain_picks_the_matching_branch` | `else if` chain → `2` | yes |
| `par_for_runs` | `par for` fills an array, sum is `140` | yes |
| `emit_prints_c` | stdout contains `int main(void)` | no |
| `build_creates_an_executable_that_runs` | builds, runs, prints `42` | yes |
| `second_build_is_up_to_date` | cache hit prints `up to date` | yes |
| `dev_flag_builds` | `--dev` path works | yes |

## Not covered by `cargo test`

| Check | Why not | Where |
|---|---|---|
| binary-trees checksum `568328208` | 30s+ workload | `bench-bt/` manual script |
| arena bound vs measured RSS | environment-dependent | planned |
| compile-speed numbers | benchmark, not a unit test | `bench-compile/RESULTS.md` |
