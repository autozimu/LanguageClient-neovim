# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.1.157]

### Added

- Update cursor location properly after additional text edits (like automatic import) applied in completion (#961)
- Add [CHANGELOG](https://github.com/autozimu/LanguageClient-neovim/blob/next/CHANGELOG.md).
- Add support for `rust-anaylzer.selectAndApplySourceChange` (#965)
- Add support to customize highlight for floating hover window (#966)
- Add support for the proposed semantic highlighting (#954)
- Add `LanguageClient_applyCompletionAdditionalTextEdits` flag (#978)
- Add default root path for Go project (#976)
- Add doc for installation on ArchLinux through aur (#988)
- Add CodeLens integration with FZF (#996)
- Add option to set preferred MarkupKind (#972)
- New function `LanguageClient#isServerRunning()` for statusline integration (#1003)
- New function `LanguageClient#statusLineDiagnosticsCounts` (#1006)

### Fixed

- Fix issue calling `nvim_create_namespace` under vim #967
- Fix signs not cleanup #1008
