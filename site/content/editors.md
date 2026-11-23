# Editors

`astli lsp` is a language server. It reads the package around the file you open, the way [`astli check`](checking.md) does with no arguments, so it needs no setup beyond `astli.toml`, if the package has one. An editor needs `astli` on its `PATH`.

The server provides:

- the problems `astli lint` and `astli check` report, as you type, and the fixes lint rules offer as quick fixes;
- formatting, under the package's `[fmt]`;
- go to definition, find references, and highlighting of a name's other uses, across the package and its dependencies;
- hover: a name's type and width as elaborated, a parameter's value, what a macro call expands to;
- inlay hints: the port or parameter a positional connection lands on, the ports a `.*` connects, and what a parameter's expression evaluates to;
- colours for what each name is: a module, a port, a parameter, a type;
- faded `` `ifdef `` branches that the build leaves out, under the package's defines;
- symbols in a file, and across the workspace.

Where `astli.toml` names [tops](reference/manifest.md#topname), the problems include what their builds get wrong, and hover gives the values the tops set a parameter to.

## Zed

Zed needs an extension to start a language server. astli's is in the repository, in [`editors/zed`](https://github.com/fischeti/astli/tree/main/editors/zed), and not published yet. Install the [`verilog`](https://github.com/someone13574/zed-verilog-extension) extension first, which gives Zed the languages. Then clone the repository, run `zed: install dev extension` and choose `editors/zed`. Zed builds the extension itself, which needs Rust installed through `rustup`.

The `verilog` extension opens `.sv`, `.svh`, `.v` and `.vh` files all as SystemVerilog, and names other servers for it. Tell Zed to use astli's, and to format through it, in `settings.json`:

```json
{
  "languages": {
    "SystemVerilog": {
      "language_servers": ["astli"],
      "formatter": "language_server"
    }
  }
}
```

## VS Code

VS Code needs an extension to start a language server, and astli's is not published yet.

## Emacs

Eglot, built into Emacs 29 and later, starts the server for `verilog-mode`, which opens `.sv`, `.svh`, `.v` and `.vh` files:

```elisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs '(verilog-mode . ("astli" "lsp"))))
(add-hook 'verilog-mode-hook #'eglot-ensure)
```

`M-x eglot-format-buffer` formats the file, and `eglot-inlay-hints-mode` toggles the hints.

## Neovim

Neovim 0.11 and later starts a server from configuration alone. Put this in `init.lua`:

```lua
-- Neovim gives `.vh` headers no filetype of its own.
vim.filetype.add({ extension = { vh = 'verilog' } })

vim.lsp.config('astli', {
  cmd = { 'astli', 'lsp' },
  filetypes = { 'systemverilog', 'verilog' },
  root_markers = { 'astli.toml', 'Bender.yml', '.git' },
})
vim.lsp.enable('astli')
```

To format on save and show inlay hints:

```lua
vim.api.nvim_create_autocmd('LspAttach', {
  callback = function(args)
    local client = vim.lsp.get_client_by_id(args.data.client_id)
    if client.name ~= 'astli' then return end
    vim.lsp.inlay_hint.enable(true, { bufnr = args.buf })
    vim.api.nvim_create_autocmd('BufWritePre', {
      buffer = args.buf,
      callback = function() vim.lsp.buf.format({ bufnr = args.buf, id = client.id }) end,
    })
  end,
})
```

## Claude Code

The [astli plugin](https://github.com/fischeti/astli/tree/main/plugin) starts the server. Claude then sees the problems after each edit it makes, and can go to definitions, find references, hover and list symbols as a tool. Add it with:

```sh
/plugin marketplace add fischeti/astli
/plugin install astli@astli
```

## Files changed outside the editor

The server keeps what it found about a package between edits. It finds the package again when a manifest or the lock changes, or when a source file is created or deleted, if the editor tells it about changes on disk. An editor that does not tell the server sees such changes only once every file of the package is closed and one is opened again.
