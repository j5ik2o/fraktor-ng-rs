// syn-check: allow(use-placement) -- 生成コードとの整合のため関数内に置く
fn build() {
    let value = 1;
    {
        use std::collections::HashMap;
    }
}

fn other() {
    use std::collections::HashSet;
}
