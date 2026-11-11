use crate::kernel::{extension::ClusterApi as CoreClusterApi, grain::GrainRef as CoreGrainRef};
use fraktor_actor::kernel::scheduler::SchedulerApi;

pub struct GrainRef {
  inner: CoreGrainRef,
}

pub struct ClusterApi {
  inner: CoreClusterApi,
}

pub struct FullyQualifiedGrainRef {
  inner: crate::kernel::grain::FullyQualifiedGrainRef,
}

pub struct TimerDriver(SchedulerApi, fraktor_actor::kernel::clock::ClockApi);
