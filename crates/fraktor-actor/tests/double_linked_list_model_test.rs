// Model-based property tests for `DoubleLinkedList` (not a port of an original
// case; the ledger does not count them).
//
// The properties P1 to P5 are defined in
// docs/design/issue-176-double-linked-list.md §3. Each run drives three
// lists with a random sequence of operations and, after every step, compares
// each list with a model: a `Vec` of (id, value) in list order that applies the
// Pekko semantics of each operation (the I8 table of the design). The sequences
// come from a fixed-seed SplitMix64, so a failure reproduces from the seed and
// the step that the message prints, together with the last operations.
//
// Ids passed to the operations are drawn from the live nodes of the list, from
// nodes the list removed, and from nodes of the other lists. An operation that
// must panic (a removed or foreign id, or a closure of the caller that panics)
// runs under `catch_unwind`, and the list must still equal the model afterwards.
//
// Values are (key, serial). The key takes four values so that distinct nodes
// share keys and the predicates see ties; the serial is unique, so a value
// swapped between nodes or a node made with the wrong value shows up.

use std::{
  cell::Cell,
  collections::HashSet,
  panic::{self, AssertUnwindSafe},
  sync::Once,
};

use fraktor_actor::kernel::double_linked_list::{DoubleLinkedList, NodeId};

type Value = (u8, u64);

const ID_PANIC: &str = "the node is in this list";
const CLOSURE_PANIC: &str = "a closure of the caller panicked";
const LISTS: usize = 3;

thread_local! {
  // Set only while an operation that must panic runs under `catch_unwind`.
  static EXPECTING_PANIC: Cell<bool> = const { Cell::new(false) };
}

// Keeps the expected panics out of the output. A panic outside an expected
// section (a failed assertion, or an unexpected panic of the list) prints as
// usual, whatever its message.
fn quiet_expected_panics() {
  static HOOK: Once = Once::new();
  HOOK.call_once(|| {
    let default = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
      if !EXPECTING_PANIC.with(Cell::get) {
        default(info);
      }
    }));
  });
}

// Runs `f`, which must panic, with the panic kept out of the output, and
// returns the panic message.
fn expect_panic(f: impl FnOnce()) -> Option<String> {
  EXPECTING_PANIC.with(|flag| flag.set(true));
  let outcome = panic::catch_unwind(AssertUnwindSafe(f));
  EXPECTING_PANIC.with(|flag| flag.set(false));
  let payload = outcome.err()?;
  Some(
    payload
      .downcast_ref::<String>()
      .cloned()
      .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
      .unwrap_or_default(),
  )
}

struct SplitMix64(u64);

impl SplitMix64 {
  fn next(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, n: usize) -> usize {
    (self.next() % n as u64) as usize
  }

  fn chance(&mut self, percent: u64) -> bool {
    self.next() % 100 < percent
  }
}

// The model of one list: (id, value) from the first to the last node.
#[derive(Default)]
struct Model {
  order: Vec<(NodeId, Value)>,
  removed: Vec<NodeId>,
}

impl Model {
  fn index_of(&self, id: NodeId) -> usize {
    self.order.iter().position(|(node, _)| *node == id).expect("the model holds the id")
  }

  fn value_at(&self, i: usize) -> Value {
    self.order[i].1
  }
}

struct Run {
  seed: u64,
  step: usize,
  rng: SplitMix64,
  lists: Vec<DoubleLinkedList<Value>>,
  models: Vec<Model>,
  issued: HashSet<NodeId>,
  serial: u64,
  log: Vec<String>,
}

// What an insert-or-return operation must return.
enum Expect {
  Existing(NodeId),
  New,
}

impl Run {
  fn new(seed: u64) -> Self {
    Self {
      seed,
      step: 0,
      rng: SplitMix64(seed),
      lists: (0..LISTS).map(|_| DoubleLinkedList::new()).collect(),
      models: (0..LISTS).map(|_| Model::default()).collect(),
      issued: HashSet::new(),
      serial: 0,
      log: Vec::new(),
    }
  }

  fn context(&self) -> String {
    let from = self.log.len().saturating_sub(12);
    format!("seed {:#x}, step {}, last operations: {:?}", self.seed, self.step, &self.log[from..])
  }

  fn fresh_value(&mut self) -> Value {
    self.serial += 1;
    ((self.rng.below(4)) as u8, self.serial)
  }

  // P4: every id returned for a new node is new across all lists.
  fn record_new(&mut self, l: usize, id: NodeId, at: usize, value: Value) {
    assert!(self.issued.insert(id), "P4: id {id:?} returned twice; {}", self.context());
    self.models[l].order.insert(at, (id, value));
  }

  fn live_id(&mut self, l: usize) -> Option<NodeId> {
    let order = &self.models[l].order;
    if order.is_empty() {
      None
    } else {
      Some(order[self.rng.below(order.len())].0)
    }
  }

  // An id that does not name a node of list `l`: one it removed, or one of
  // another list (live or removed).
  fn stranger_id(&mut self, l: usize) -> Option<NodeId> {
    let mut candidates: Vec<NodeId> = self.models[l].removed.clone();
    for (other, model) in self.models.iter().enumerate() {
      if other != l {
        candidates.extend(model.order.iter().map(|(id, _)| *id));
        candidates.extend(model.removed.iter().copied());
      }
    }
    if candidates.is_empty() {
      None
    } else {
      Some(candidates[self.rng.below(candidates.len())])
    }
  }

  // P1, P2: the list equals its model.
  fn check(&self, l: usize) {
    let list = &self.lists[l];
    let model = &self.models[l];
    let ids: Vec<NodeId> = model.order.iter().map(|(id, _)| *id).collect();
    let values: Vec<Value> = model.order.iter().map(|(_, value)| *value).collect();
    // One step more than the model's length, so a cycle shows up as a longer
    // walk instead of a walk that never ends.
    let forward: Vec<NodeId> = list.forward_iter().take(ids.len() + 1).collect();
    assert_eq!(forward, ids, "P1 forward order of list {l}; {}", self.context());
    let backward: Vec<NodeId> = list.backward_iter().take(ids.len() + 1).collect();
    let reversed: Vec<NodeId> = ids.iter().rev().copied().collect();
    assert_eq!(backward, reversed, "P1 backward order of list {l}; {}", self.context());
    let read: Vec<Value> = forward.iter().map(|id| list[*id]).collect();
    assert_eq!(read, values, "P1 values of list {l}; {}", self.context());
    assert_eq!(list.is_empty(), ids.is_empty(), "P2 is_empty of list {l}; {}", self.context());
    assert_eq!(list.get_first(), ids.first().copied(), "P2 first of list {l}; {}", self.context());
    assert_eq!(list.get_last(), ids.last().copied(), "P2 last of list {l}; {}", self.context());
    for (i, id) in ids.iter().enumerate() {
      let previous = if i == 0 { None } else { Some(ids[i - 1]) };
      assert_eq!(list.get_previous(*id), previous, "P2 previous of {id:?}; {}", self.context());
      assert_eq!(list.get_next(*id), ids.get(i + 1).copied(), "P2 next of {id:?}; {}", self.context());
      assert_eq!(list.get(*id), Some(&values[i]), "P2 get of {id:?}; {}", self.context());
    }
  }

  fn step(&mut self) {
    let l = self.rng.below(LISTS);
    let op = self.rng.below(16);
    match op {
      | 0 | 1 => self.prepend_or_append(l, op == 0),
      | 2 => self.remove(l),
      | 3 | 4 => self.move_to_end(l, op == 3),
      | 5 | 6 => self.end_or_else_insert(l, op == 5),
      | 7 | 8 => self.neighbour_or_else_insert(l, op == 7),
      | 9 | 10 => self.find_or_else_insert(l, op == 9),
      | 11 => self.write_value(l),
      | 12 | 13 => self.stranger(l),
      | _ => self.panicking_closure(l),
    }
    self.check(l);
  }

  fn prepend_or_append(&mut self, l: usize, front: bool) {
    let value = self.fresh_value();
    self.log.push(format!("list {l}: {} {value:?}", if front { "prepend" } else { "append" }));
    let id = if front { self.lists[l].prepend(value) } else { self.lists[l].append(value) };
    let at = if front { 0 } else { self.models[l].order.len() };
    self.record_new(l, id, at, value);
  }

  fn remove(&mut self, l: usize) {
    let Some(id) = self.live_id(l) else { return };
    self.log.push(format!("list {l}: remove {id:?}"));
    let got = self.lists[l].remove(id);
    let i = self.models[l].index_of(id);
    let (_, want) = self.models[l].order.remove(i);
    self.models[l].removed.push(id);
    assert_eq!(got, want, "P3 removed value; {}", self.context());
    assert_eq!(self.lists[l].get(id), None, "P5 get of the removed {id:?}; {}", self.context());
  }

  fn move_to_end(&mut self, l: usize, front: bool) {
    let Some(id) = self.live_id(l) else { return };
    self.log.push(format!("list {l}: move {id:?} to the {}", if front { "front" } else { "back" }));
    let got = if front { self.lists[l].move_to_front(id) } else { self.lists[l].move_to_back(id) };
    assert_eq!(got, id, "P3 moved id; {}", self.context());
    let i = self.models[l].index_of(id);
    let entry = self.models[l].order.remove(i);
    if front {
      self.models[l].order.insert(0, entry);
    } else {
      self.models[l].order.push(entry);
    }
  }

  // Applies an insert-or-return operation: `run` calls the list with a `new`
  // closure that counts its calls, and `expect` and `at` say what the model
  // wants.
  fn insert_or_return(
    &mut self,
    l: usize,
    expect: Expect,
    at: usize,
    run: impl FnOnce(&mut DoubleLinkedList<Value>, &mut dyn FnMut() -> Value) -> NodeId,
  ) {
    let value = self.fresh_value();
    let mut calls = 0;
    let got = run(&mut self.lists[l], &mut || {
      calls += 1;
      value
    });
    match expect {
      | Expect::Existing(want) => {
        assert_eq!(got, want, "P3 returned an existing node; {}", self.context());
        assert_eq!(calls, 0, "P3 made a new value for an existing node; {}", self.context());
      },
      | Expect::New => {
        assert_eq!(calls, 1, "P3 made the new value {calls} times; {}", self.context());
        self.record_new(l, got, at, value);
      },
    }
  }

  fn end_or_else_insert(&mut self, l: usize, front: bool) {
    let key = self.rng.below(4) as u8;
    self.log.push(format!("list {l}: get_{}_or_else_insert key == {key}", if front { "first" } else { "last" }));
    let model = &self.models[l];
    let end = if front { model.order.first() } else { model.order.last() };
    let (expect, at) = match end {
      | Some((id, value)) if value.0 == key => (Expect::Existing(*id), 0),
      | _ => (Expect::New, if front { 0 } else { model.order.len() }),
    };
    self.insert_or_return(l, expect, at, |list, new| {
      if front {
        list.get_first_or_else_prepend(|v| v.0 == key, new)
      } else {
        list.get_last_or_else_append(|v| v.0 == key, new)
      }
    });
  }

  fn neighbour_or_else_insert(&mut self, l: usize, forward: bool) {
    let Some(id) = self.live_id(l) else { return };
    let key = self.rng.below(4) as u8;
    self
      .log
      .push(format!("list {l}: get_{}_or_else_insert {id:?} key == {key}", if forward { "next" } else { "previous" }));
    let i = self.models[l].index_of(id);
    let (expect, at) = self.neighbour_expectation(l, i, forward, key);
    self.insert_or_return(l, expect, at, |list, new| {
      if forward {
        list.get_next_or_else_insert(id, |v| v.0 == key, new)
      } else {
        list.get_previous_or_else_insert(id, |v| v.0 == key, new)
      }
    });
  }

  // The model of `get_next_or_else_insert` / `get_previous_or_else_insert`
  // from the node at `i`.
  fn neighbour_expectation(&self, l: usize, i: usize, forward: bool, key: u8) -> (Expect, usize) {
    let model = &self.models[l];
    if forward {
      if i + 1 < model.order.len() && model.value_at(i + 1).0 == key {
        (Expect::Existing(model.order[i + 1].0), 0)
      } else {
        (Expect::New, i + 1)
      }
    } else if i > 0 && model.value_at(i - 1).0 == key {
      (Expect::Existing(model.order[i - 1].0), 0)
    } else {
      (Expect::New, i)
    }
  }

  fn find_or_else_insert(&mut self, l: usize, forward: bool) {
    let Some(id) = self.live_id(l) else { return };
    let bound = self.rng.below(4) as u8;
    let key = self.rng.below(4) as u8;
    self.log.push(format!(
      "list {l}: find_{}_or_else_insert {id:?} while key <= {bound}, key == {key}",
      if forward { "next" } else { "previous" }
    ));
    let model = &self.models[l];
    let mut j = model.index_of(id);
    if forward {
      while j + 1 < model.order.len() && model.value_at(j + 1).0 <= bound {
        j += 1;
      }
    } else {
      while j > 0 && model.value_at(j - 1).0 <= bound {
        j -= 1;
      }
    }
    let (expect, at) = self.neighbour_expectation(l, j, forward, key);
    self.insert_or_return(l, expect, at, |list, new| {
      if forward {
        list.find_next_or_else_insert(id, |v| v.0 <= bound, |v| v.0 == key, new)
      } else {
        list.find_previous_or_else_insert(id, |v| v.0 <= bound, |v| v.0 == key, new)
      }
    });
  }

  fn write_value(&mut self, l: usize) {
    let Some(id) = self.live_id(l) else { return };
    let value = self.fresh_value();
    self.log.push(format!("list {l}: write {value:?} to {id:?}"));
    if self.rng.chance(50) {
      self.lists[l][id] = value;
    } else {
      *self.lists[l].get_mut(id).expect("a live id") = value;
    }
    let i = self.models[l].index_of(id);
    self.models[l].order[i].1 = value;
  }

  // P5: a removed or foreign id names nothing here, every operation that takes
  // it panics, and the list stays as it was.
  fn stranger(&mut self, l: usize) {
    let Some(id) = self.stranger_id(l) else { return };
    let op = self.rng.below(12);
    self.log.push(format!("list {l}: stranger {id:?}, operation {op}"));
    assert_eq!(self.lists[l].get(id), None, "P5 get of a stranger; {}", self.context());
    let found = self.lists[l].get_mut(id).is_some();
    assert!(!found, "P5 get_mut of a stranger; {}", self.context());
    let list = &mut self.lists[l];
    let text = expect_panic(|| match op {
      | 0 => drop(list.remove(id)),
      | 1 => drop(list.move_to_front(id)),
      | 2 => drop(list.move_to_back(id)),
      | 3 => drop(list.get_previous(id)),
      | 4 => drop(list.get_next(id)),
      | 5 => drop(list.get_next_or_else_insert(id, |_| true, || (0, 0))),
      | 6 => drop(list.get_previous_or_else_insert(id, |_| false, || (0, 0))),
      | 7 => drop(list.find_next_or_else_insert(id, |_| true, |_| false, || (0, 0))),
      | 8 => drop(list.find_previous_or_else_insert(id, |_| true, |_| false, || (0, 0))),
      | 9 => {
        let _read: Value = list[id];
      },
      | 10 => list[id] = (0, 0),
      | _ => drop(list.find_next_or_else_insert(id, |_| false, |_| true, || (0, 0))),
    });
    assert!(text.as_deref().is_some_and(|t| t.contains(ID_PANIC)), "P5 panic message {text:?}; {}", self.context());
  }

  // P5: a closure of the caller that panics leaves the list as it was.
  fn panicking_closure(&mut self, l: usize) {
    let id = self.live_id(l);
    let op = self.rng.below(8);
    self.log.push(format!("list {l}: panicking closure, operation {op}, id {id:?}"));
    let list = &mut self.lists[l];
    let panicking_new = || -> Value { panic!("{CLOSURE_PANIC}") };
    let panicking_check = |_: &Value| -> bool { panic!("{CLOSURE_PANIC}") };
    let text = expect_panic(|| match (op, id) {
      // `new` panics: the end never passes `check`, so a node is always made.
      | (0, _) => drop(list.get_first_or_else_prepend(|_| false, panicking_new)),
      | (1, _) => drop(list.get_last_or_else_append(|_| false, panicking_new)),
      | (2, Some(id)) => drop(list.get_next_or_else_insert(id, |_| false, panicking_new)),
      | (3, Some(id)) => drop(list.find_previous_or_else_insert(id, |_| true, |_| false, panicking_new)),
      // `check`, `is_before` or `is_after` panics. Where the list does not call
      // it (no neighbour), `new` is called instead and panics too.
      | (4, Some(_)) => drop(list.get_first_or_else_prepend(panicking_check, panicking_new)),
      | (5, Some(id)) => drop(list.get_previous_or_else_insert(id, panicking_check, panicking_new)),
      | (6, Some(id)) => drop(list.find_next_or_else_insert(id, panicking_check, |_| false, panicking_new)),
      | (7, Some(id)) => drop(list.find_previous_or_else_insert(id, |_| true, panicking_check, panicking_new)),
      | _ => panic!("{CLOSURE_PANIC}"),
    });
    assert!(
      text.as_deref().is_some_and(|t| t.contains(CLOSURE_PANIC)),
      "P5 panic message {text:?}; {}",
      self.context()
    );
  }
}

fn run(seed: u64, steps: usize) -> usize {
  quiet_expected_panics();
  let mut run = Run::new(seed);
  let mut longest = 0;
  for step in 0..steps {
    run.step = step;
    run.step();
    longest = longest.max(run.models.iter().map(|m| m.order.len()).max().unwrap_or(0));
  }
  for l in 0..LISTS {
    run.check(l);
  }
  longest
}

// P1 to P5 over 64 seeds of 400 steps each.
#[test]
fn random_operations_match_the_model() {
  for seed in 0..64_u64 {
    run(0xD0B1_E000 + seed, 400);
  }
}

// P1 to P5 on long lists: inserts outnumber removals, so the lists grow to
// hundreds of nodes.
#[test]
fn long_random_operations_match_the_model() {
  for seed in 0..4_u64 {
    let longest = run(0x10A6_0000 + seed, 3000);
    assert!(longest >= 300, "seed {seed}: the longest list had only {longest} nodes");
  }
}

// F3 of the design review: the foreign id panics and leaves this list as it was.
#[test]
fn removing_an_id_from_another_list_panics_and_keeps_this_list() {
  quiet_expected_panics();
  let mut mine = DoubleLinkedList::new();
  let mut other = DoubleLinkedList::new();
  let foreign = other.append("other");
  let a = mine.append("a");
  let b = mine.append("b");
  let text = expect_panic(|| {
    let _removed: &str = mine.remove(foreign);
  });
  assert!(text.as_deref().is_some_and(|t| t.contains(ID_PANIC)), "panic message {text:?}");
  assert_eq!(mine.forward_iter().collect::<Vec<_>>(), [a, b]);
  assert_eq!(mine.backward_iter().collect::<Vec<_>>(), [b, a]);
  assert_eq!((mine[a], mine[b]), ("a", "b"));
  assert_eq!(other.forward_iter().map(|id| other[id]).collect::<Vec<_>>(), ["other"]);
}
