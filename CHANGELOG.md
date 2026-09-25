# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/fischeti/astli/compare/v0.1.0...v0.1.1) - 2026-09-25

### Documentation

- *(astli)* inline each crate as a module page
- *(astli)* a tour and a map of the modules in the crate docs
- *(text)* a usage guide in the crate docs
- *(diag)* a usage guide in the crate docs
- *(syntax)* a usage guide in the crate docs
- *(preproc)* a usage guide in the crate docs
- *(parse)* a usage guide in the crate docs
- *(fmt)* a usage guide in the crate docs

### Performance

- *(fmt)* place comments only where a gap holds some
- *(cli)* allocate with mimalloc

## [0.1.0](https://github.com/fischeti/astli/releases/tag/v0.1.0) - 2026-09-24

The first release: a preprocessor, a lossless syntax tree, and a formatter in
the lowRISC style.
