# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

### Categories each change fall into

* **Added**: for new features.
* **Changed**: for changes in existing functionality.
* **Deprecated**: for soon-to-be removed features.
* **Removed**: for now removed features.
* **Fixed**: for any bug fixes.
* **Security**: in case of vulnerabilities.


## [Unreleased]
### Added
- Add support for building without the standard library.
- Add a `const fn`, `DynStack::new_unchecked`. Allows static initialization. This makes the
  minimum required compiler version 1.39.
- Implement `Iterator::size_hint` and `ExactSizeIterator` for `DynStackIter` and `DynStackIterMut`.

### Changed
- Don't allocate memory in `DynStack::new`. Postpone allocation until the first push.
- Upgrade the crate to Rust 2018 edition.
- Implement `Send` and/or `Sync` for `DynStack<T>` if `T` is `Send`/`Sync`.


## [0.3.0] - 2019-04-24
### Fixed
- Assert that `T` is a trait object in `DynStack::new`. Prevents using the stack on normal
  objects, that could have caused undefined behavior.
- Check fat pointer memory layout at build time. Makes this crate fail to build if the memory
  representation of a trait object ever changes. Hopefully preventing undefined behavior.


## [0.2.1] - 2019-04-24
Just removed large binary files that were accidentally included in the package uploaded to
crates.io. Otherwise identical to 0.2.0.


## [0.2.0] - 2018-12-16
### Fixed
- Correctly drop in remove_last, prevent overflow panics.
- Correctly handle alignments greater than 16 by moving data if necessary.
