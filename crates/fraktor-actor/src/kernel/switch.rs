//! An atomic switch that can be either on or off, as in
//! `org.apache.pekko.util.Switch`.

mod locked_switch;
mod revert_guard;
mod switch_value;
pub use locked_switch::LockedSwitch;
pub use switch_value::Switch;
