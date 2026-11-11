fn helper() {}

use std::fmt;

fn ported_case() {
    use std::collections::HashMap;

    let _map: HashMap<u8, fmt::Error> = HashMap::new();
    helper();
}
