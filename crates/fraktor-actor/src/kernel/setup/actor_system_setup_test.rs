//! Unit tests for [`ActorSystemSetup`](super::ActorSystemSetup).

use alloc::sync::Arc;

use super::super::{ActorSystemSetup, Setup};

#[derive(Clone, Debug, PartialEq)]
struct First(u32);

#[derive(Clone, Debug, PartialEq)]
struct Second(u32);

impl Setup for First {}
impl Setup for Second {}

#[test]
fn an_empty_setup_holds_nothing() {
  let setups = ActorSystemSetup::empty();
  assert_eq!(setups.get::<First>(), None);
  assert_eq!(ActorSystemSetup::default().get::<Second>(), None);
}

#[test]
fn the_later_of_two_setups_of_one_type_wins_when_created_from_a_set() {
  let setups =
    ActorSystemSetup::from_setups([Arc::new(First(1)) as Arc<dyn Setup>, Arc::new(Second(2)), Arc::new(First(3))]);
  assert_eq!(setups.get::<First>(), Some(&First(3)));
  assert_eq!(setups.get::<Second>(), Some(&Second(2)));
}

#[test]
fn adding_a_setup_leaves_a_clone_taken_before_unchanged() {
  let before = ActorSystemSetup::empty().with_setup(First(1));
  let after = before.clone().with_setup(First(2)).and(Second(3));
  assert_eq!(before.get::<First>(), Some(&First(1)));
  assert_eq!(before.get::<Second>(), None);
  assert_eq!(after.get::<First>(), Some(&First(2)));
  assert_eq!(after.get::<Second>(), Some(&Second(3)));
}

#[test]
fn a_setup_of_one_type_is_not_returned_for_another() {
  let setups = First(1).and(First(2));
  assert_eq!(setups.get::<First>(), Some(&First(2)));
  assert_eq!(setups.get::<Second>(), None);
}

#[test]
fn get_returns_the_registered_instance() {
  let registered = Arc::new(First(1));
  let setups = ActorSystemSetup::from_setups([Arc::clone(&registered) as Arc<dyn Setup>]).and(Second(2));
  let cloned = setups.clone();

  assert!(setups.get::<First>().is_some_and(|setup| core::ptr::eq(setup, Arc::as_ptr(&registered))));
  assert!(cloned.get::<First>().is_some_and(|setup| core::ptr::eq(setup, Arc::as_ptr(&registered))));
}
