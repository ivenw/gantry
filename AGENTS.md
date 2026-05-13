- If you are unsure how to use a feature in a particular crate, check the docs.rs documentation for that crate.
- ALWAYS check that everything still compiles after you are done with all your changes.
- Code is read from top to bottom, from least to most granular, from primary to supporting abstractions. Example:

  ```rust
  // Good
  fn foo() {
    bar()
  }

  fn bar() {}

  // Bad
  fn bar() {}

  fn foo() {
    bar()
  }
  ```

- ALWAYS confirm with the user before supressing any lint instead of fixing it.
- Prefer to import import a type at the top of a module instead of fully qualifying it at the call site/return type.
- ALWAYS write at least a minimally descriptive docstring of what a function does. A docstrings ALWAYS starts with one short sentence.
The docstring should be informative to the caller. Implementation details are to be documented inside of the function body.
