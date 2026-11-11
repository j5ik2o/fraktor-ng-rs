use std::fmt;

#[cfg(test)]
#[path = "widget_test.rs"]
mod tests;

pub struct Widget;

impl fmt::Display for Widget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("widget")
    }
}

#[cfg(test)]
mod inline_checks {
    fn check() {
        use std::collections::HashMap;

        let _map: HashMap<u8, u8> = HashMap::new();
    }
}

#[cfg(test)]
mod late_hook;
