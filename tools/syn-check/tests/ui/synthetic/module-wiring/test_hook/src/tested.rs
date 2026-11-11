pub struct Tested;

#[cfg(all(test, unix))]
mod fixtures;

#[cfg(test)]
#[path = "tested_test.rs"]
mod tests;
