---
name: Bug report
about: Create a report to help us improve

---

- Did you upgrade to latest plugin version?
- Did you upgrade to/compile latest binary? Run shell command
  `bin/languageclient --version` to get its version number.
- (Neovim users only) Did you check output of `:checkhealth LanguageClient`?
- Did you check [troubleshooting]?

[troubleshooting]: https://github.com/autozimu/LanguageClient-neovim/blob/next/INSTALL.md#troubleshooting

## Describe the bug

A clear and concise description of what the bug is.

## Environment

- neovim/vim version (`nvim --version` or `vim --version`):
- This plugin version (`git rev-parse --short HEAD`):
- This plugin's binary version (`bin/languageclient --version`):
- Minimal vimrc content (A minimal vimrc is the smallest vimrc that could
  reproduce the issue. Refer to an example [here][min-vimrc.vim]):
- Language server link and version:

[min-vimrc.vim]: https://github.com/autozimu/LanguageClient-neovim/blob/next/min-vimrc.vim

## To Reproduce

Steps to reproduce the behavior:

- Create/Fetch example project ...
- Start vim, `nvim -u min-vimrc.vim` ...
- Edit ...
- Execute ....
- See error

## Current behavior

A clear and concise description of what's the current behavior.

## Expected behavior

A clear and concise description of what you expected to happen.

## Screenshots

If applicable, add screenshots to help explain your problem.

## Additional context

Add any other context about the problem here.
