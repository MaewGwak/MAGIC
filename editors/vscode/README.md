# MAGIC for VS Code

Syntax highlighting, snippets and compiler diagnostics for `.magic` files.

## Install without vsce

Copy this directory into your VS Code extensions directory, e.g.:

```sh
cp -r editors/vscode ~/.vscode/extensions/magic-language-0.1.0
```

Reload VS Code. The extension activates for `.magic` files.

## Commands

- **MAGIC: Check file** — runs `magicc check <file> --backend c` and shows
  `file:line` diagnostics.
- **MAGIC: Build file** — opens a terminal running `magicc build <file> --fast`.

Set `magic.compilerPath` if `magicc` is not on `PATH`; by default the extension
looks for `<workspace>/target/release/magicc` then `magicc`.

## Next tooling steps

- formatter and linter
- LSP (autocomplete, go-to-definition, hover)
- DWARF symbolizer for static panic traces
