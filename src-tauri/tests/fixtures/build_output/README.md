# Build output fixtures

Complete console output of real failing (and a few successful) Gradle builds, used by the
fixture tests in `src/services/build_parser.rs`. Each file is stdout and stderr merged, as the
app receives them.

Captured with Gradle 9.4.1 (`--console=plain`), AGP 9.2.1 (built-in Kotlin), Kotlin 2.3.10,
KSP 2.3.9, Room 2.8.4, and JetBrains Runtime 21.0.8 on macOS.

| File | Build | Trigger |
|------|-------|---------|
| `successful_build.txt` | `:app:assembleDebug :app:assembleRelease :app:lintDebug` (succeeds) | None |
| `kotlin_k2_errors.txt` | `:app:assembleDebug` | Unresolved references and a type mismatch in two files |
| `kotlin_k2_warnings.txt` | `:app:assembleDebug` (succeeds) | Deprecated call and a redundant cast |
| `javac_errors.txt` | `:app:assembleDebug` | Incompatible types and a missing symbol |
| `ksp_room_errors.txt` | `:app:assembleDebug` | Room `@Entity` without a primary key and an invalid `@Query` |
| `lint_abort_on_error.txt` | `:app:lintDebug` | `NewApi` error with the default `abortOnError` |
| `lint_text_report.txt` | `:app:lintDebug` | The text report lint writes for the same run (errors and warnings) |
| `r8_missing_classes.txt` | `:app:assembleRelease` | Library class extends a class that is only `compileOnly` |
| `r8_keep_rule_syntax.txt` | `:app:assembleRelease` | Typo in `proguard-rules.pro` |
| `resource_merger_malformed_xml.txt` | `:app:assembleDebug` | Unclosed tag in `res/values` |
| `aapt2_compile_invalid_value.txt` | `:app:assembleDebug` | Invalid color value |
| `aapt2_link_errors.txt` | `:app:assembleDebug` | Missing resource references and an unknown attribute in a layout |
| `configuration_cache_problems.txt` | `--configuration-cache :app:printStamp` | Build listener and `Task.project` at execution time |
| `configuration_cache_problems_warn.txt` | `--configuration-cache --configuration-cache-problems=warn :app:help` (succeeds) | Build listener |
| `gradle_script_compile_error.txt` | `--configuration-cache :app:printStamp` | Unresolved reference in `build.gradle.kts` |

Sanitized: the project path is replaced with `/Users/dev/project`, the home directory with
`/Users/dev`, and the terminal progress escape sequence Gradle prints last is removed. Nothing
else was edited.
