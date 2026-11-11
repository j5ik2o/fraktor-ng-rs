mod outer {
    mod inner {
        include!("inner.inc");
    }
}

mod commented {
    // include!("commented.inc");
    pub struct Commented;
}

mod quoted {
    const NOTE: &str = "include!(x)";
}

include!("top.inc");
