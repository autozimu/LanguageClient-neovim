# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- Update cursor location properly when additional text edits (like automatic
  import) happens in completion by [cheuk-fung](https://github.com/cheuk-fung). #961

### Fixed

- Fix issue calling `nvim_create_namespace` under vim. #967
