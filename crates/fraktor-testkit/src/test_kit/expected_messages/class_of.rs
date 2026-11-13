//! A type tag that a test passes to the message-matching expect methods.

use core::any::Any;
use core::fmt;
use core::marker::PhantomData;

use fraktor_actor::kernel::message::Message;

/// Names a type whose messages a test expects.
///
/// Mirrors `Class[_]` in `org.apache.pekko.testkit`, where the class object
/// carries the runtime type. Rust has no class object, so the tag carries the
/// type as a parameter. The parameter lets the expect methods return the
/// matched value as `T` without a cast at the call site, so
/// `expect_msg_any_class_of(&[ClassOf::of::<i32>()])` yields an `i32`.
pub struct ClassOf<T> {
  marker: PhantomData<fn() -> T>,
}

impl ClassOf<()> {
  /// Returns the tag of `T`.
  pub fn of<T: Any>() -> ClassOf<T> {
    ClassOf { marker: PhantomData }
  }
}

impl<T: Any> ClassOf<T> {
  /// Returns whether `message` carries a value of the tagged type.
  pub fn matches(&self, message: &Message) -> bool {
    message.is::<T>()
  }
}

impl<T> Copy for ClassOf<T> {}

impl<T> Clone for ClassOf<T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T> fmt::Debug for ClassOf<T> {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("ClassOf")
  }
}
