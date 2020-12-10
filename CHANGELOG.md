# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.1.162]

## [0.1.161]

### Fixed
- Fix incorrect deserialization of completion items in NCM2 (#1151)
- Fix markdown syntax on hover (#1039)
- Fix diagnostics signs logic (#1126)

### Added
- Add support for initialization options in server command (#1116)
- Add function to execute code action by kind (#1160)
- Add support for rust-analyzer chaining hints (#1108)
- Add parameter to explainErrorAtPoint to enable running silently (#1143)
- Use separate namespace for document and diagnostic highlights (#1145)
- Add LanguageClient_codeLensDisplay config (#1144)
- Add completion item documentation (#1043)
- Do not send notifications/requests for buffers without a configured server (#1121)
- Add support for document highlight on vim8 (#1123)
- Add automatic server restart on crash (#1113)

## [0.1.160]

### Fixed
- Fix issue with Sorbet sending an extra field on the messages it sends (#1115)

## [0.1.159]

### Added
- Add support for disabling/enabling server-specific extensions (#1072)
- Add support for custom codelens highlight group (#1100)
- Add support for clangd's textDocument/switchSourceHeader (#1109)
- Add configuration for hover window margin size (#1111)
- Add support to skip setting buffer omnifunc (#1079).
- Add support of setting languageclient binary path (#1020).
- Support file watching scenario of writing via rename (#1054).

### Fixed
- Bump lsp-types to 0.83 to fix issue with workspaceSymbolProvider definition (#1114)
- Fix rust-analyzer test codelens action (#1104)

## [0.1.158]

### Added
- Add support of multiple configuration files (#1013).
- Add support for code actions using ranges.
- Add support of `$/progress` notification.
- Use a minimal style for neovim's floating window and add support for override (#1033).
- Add `hideVirtualTextsOnInsert` option (#982).
- Add support of populating `source` in diagnostics (#1062).
- Add support of overriding selection UI (#1059, #1060).
- Add plug mappings (#1065).

### Changed
- Hide error of `ContentModified` (#997).
- Do not show `window/logMessage` messages (#1056).
- Change aarch64 build from `-gnu` to `-musl` (#1044).

### Fixed
- Fix issue of no signs in sign column and virtual text on Windows (#970).
- Fix error handling of install script (#1011).
- Fix detection of Haskell cabal project root (#1048).
- Fix rust-analyzer runnable actions (#1058).

### Thanks
Huge thanks to Martin (@martskins) for numerous contributions to this release,
from improving overall code structure, addressing pain points to answering
issues.

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
