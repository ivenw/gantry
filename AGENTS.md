- If you are unsure how to use a feature in a particular crate, check the docs.rs documentation for that crate.
- ALWAYS check that everything still compiles after you are done with all your changes.
- Code is read from top to bottom, from least to most granular, so follow the following implementation order.

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
