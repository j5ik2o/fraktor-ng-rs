macro_rules! declare {
    ($($tokens:tt)*) => { $($tokens)* };
}

declare! { pub struct HiddenManager; }

pub struct Visible;
