// SPDX-License-Identifier: Apache-2.0
//
// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements. See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License. You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// Licensed under the Apache License, Version 2.0. See LICENSE-APACHE and NOTICE
// at the repository root.
//
// This file has been modified from the Apache Pekko original.

// Checked against Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f): the upstream test
// file changes only by 9dfca01566 #3348; the cases and their inputs are unchanged.
// The record is docs/baseline/pekko-v2.0.0-migration/batch-3.md.
//
// Ported from Pekko v1.7.1:
//   actor-tests/src/test/scala/org/apache/pekko/actor/AddressSpec.scala
//
// Original test names are kept as comments next to each ported case.

use fraktor_actor::kernel::address::Address;

// "Address ordering" must { "work correctly with multiple mixed protocols" in { ... } }
#[test]
fn work_correctly_with_multiple_mixed_protocols() {
  let pekko_address1 = Address::with_host_port("pekko", "system", "host1", 1000);
  let pekko_address2 = Address::with_host_port("pekko", "system", "host2", 1000);
  let akka_address1 = Address::with_host_port("akka", "system", "host1", 1000);
  let akka_address2 = Address::with_host_port("akka", "system", "host2", 1000);

  let mut addresses =
    vec![pekko_address1.clone(), pekko_address2.clone(), akka_address1.clone(), akka_address2.clone()];
  addresses.sort();

  assert_eq!(addresses, vec![akka_address1, akka_address2, pekko_address1, pekko_address2]);
}
