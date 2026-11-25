//! Unit tests for [`StatusReply`](super::StatusReply): the rendering and the
//! paths that keep an error's type.

use core::error::Error;
use core::fmt;

use super::StatusReply;
use crate::adaptor::{ErrorMessage, StatusReplyAck};

// An error whose `Debug` and `Display` print only the message.
#[derive(PartialEq)]
struct Unnamed(&'static str);

impl fmt::Debug for Unnamed {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.0)
  }
}

impl fmt::Display for Unnamed {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(self.0)
  }
}

impl Error for Unnamed {}

#[test]
fn a_success_renders_its_value_and_keeps_it() {
  let reply = StatusReply::success("woho");
  assert_eq!(format!("{reply:?}"), format!("Success({:?})", reply.get_value().expect("a success")));
  assert_eq!(reply.get_value().expect("a success").downcast_ref::<&'static str>(), Some(&"woho"));
  assert!(StatusReply::ack().get_value().expect("a success").downcast_ref::<StatusReplyAck>().is_some());
}

#[test]
fn a_kept_error_renders_its_type_and_still_downcasts() {
  let name = core::any::type_name::<Unnamed>();
  for reply in [StatusReply::error(Unnamed("boo")), StatusReply::from_result_keep_error(Err::<(), _>(Unnamed("boo")))] {
    assert_eq!(format!("{reply:?}"), format!("Error({name}: boo)"));
    assert_eq!(reply.get_error().expect("a failure").downcast_ref::<Unnamed>(), Some(&Unnamed("boo")));
  }
}

#[test]
fn a_text_error_renders_its_text_and_downcasts_to_error_message() {
  for reply in [StatusReply::error_message("boo"), StatusReply::from_result(Err::<(), _>(Unnamed("boo")))] {
    assert_eq!(format!("{reply:?}"), "Error(boo)");
    assert!(reply.get_error().expect("a failure").downcast_ref::<ErrorMessage>().is_some());
  }
}
