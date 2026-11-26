// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// license agreements; and to You under the Apache License, version 2.0:
//
//   https://www.apache.org/licenses/LICENSE-2.0
//
// This file is part of the Apache Pekko project, which was derived from Akka.
//
// Copyright (C) 2009-2022 Lightbend Inc. <https://www.lightbend.com>
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Model tests of the switch outside the original SwitchSpec (design of
// #186, docs/design/issue-186-util-specs.md §3): a random sequence of
// operations runs on one thread against a `bool` model, and the switch must
// agree with the model after every operation, including actions that fail
// and actions that panic. These are not ports of Pekko cases and are not
// counted in the ledger. Inputs come from a fixed-seed SplitMix64, so a
// failure repeats.

use core::convert::Infallible;
use std::panic::{self, AssertUnwindSafe};

use fraktor_actor::kernel::switch::Switch;

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: usize) -> usize {
    (self.next_u64() % bound as u64) as usize
  }
}

#[derive(Debug, PartialEq, Eq)]
struct Boom;

fn quiet_expected_panics() {
  use std::sync::Once;
  static ONCE: Once = Once::new();
  ONCE.call_once(|| {
    let default = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
      let expected = info.payload().downcast_ref::<&str>().is_some_and(|text| *text == "model action panics");
      if !expected {
        default(info);
      }
    }));
  });
}

#[test]
fn switch_agrees_with_bool_model_over_random_operations() {
  quiet_expected_panics();
  let mut rng = SplitMix64 { state: 0x186 };
  for round in 0..60 {
    let start_on = rng.below(2) == 0;
    let s = Switch::new(start_on);
    let mut model = start_on;
    for step in 0..120 {
      let context = format!("round {round} step {step} model {model}");
      match rng.below(10) {
        | 0 => {
          let expected = !model;
          assert_eq!(s.switch_on(), expected, "{context}: switch_on");
          model = true;
        },
        | 1 => {
          let expected = model;
          assert_eq!(s.switch_off(), expected, "{context}: switch_off");
          model = false;
        },
        | 2 => {
          let result = s.switch_on_with(|| Ok::<(), Boom>(()));
          assert_eq!(result, Ok(!model), "{context}: switch_on_with ok");
          model = true;
        },
        | 3 => {
          let result = s.switch_on_with(|| Err::<(), Boom>(Boom));
          if model {
            assert_eq!(result, Ok(false), "{context}: switch_on_with err while on does not run the action");
          } else {
            assert_eq!(result, Err(Boom), "{context}: switch_on_with err");
          }
          // The state does not move either way.
        },
        | 4 => {
          let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
            let _ = s.switch_off_with(|| -> Result<(), Infallible> { panic!("model action panics") });
          }));
          // The action runs only when the switch is on; while off, nothing
          // panics and nothing changes. While on, the panic propagates and
          // the drop guard reverts the change (the model stays on).
          assert_eq!(outcome.is_err(), model, "{context}: switch_off_with panic only runs while on");
        },
        | 5 => {
          assert_eq!(s.if_on(|| {}), model, "{context}: if_on");
          assert_eq!(s.if_off(|| {}), !model, "{context}: if_off");
          assert_eq!(s.if_on_yield(|| 1), if model { Some(1) } else { None }, "{context}: if_on_yield");
          assert_eq!(s.if_off_yield(|| 1), if model { None } else { Some(1) }, "{context}: if_off_yield");
        },
        | 6 => {
          assert_eq!(s.while_on(|| {}), model, "{context}: while_on");
          assert_eq!(s.while_off(|| {}), !model, "{context}: while_off");
        },
        | 7 => {
          assert_eq!(s.fold(|| "on", || "off"), if model { "on" } else { "off" }, "{context}: fold");
        },
        | 8 => {
          // A change through the locked window, as in the original's case 6.
          let changed = s.locked(|held| {
            assert_eq!(held.is_on(), model, "{context}: locked window reads the model state");
            held.switch_on_with(|| Ok::<(), Boom>(())).expect("infallible window action")
          });
          assert_eq!(changed, !model, "{context}: locked switch_on_with");
          model = true;
        },
        | _ => {
          assert_eq!(s.is_on(), model, "{context}: is_on");
          assert_eq!(s.is_off(), !model, "{context}: is_off");
        },
      }
      assert_eq!(s.is_on(), model, "{context}: state after the operation");
      assert_eq!(s.is_off(), !model, "{context}: state after the operation");
    }
  }
}
