use fraktor_cluster_core_rs::extension::{ClusterApi, GrainApi as Grains, Membership};
use fraktor_cluster_core_rs::extension::ClusterApi as CoreCluster;
use fraktor_cluster_core_rs::glob::*;
use other_crate::extension::RemoteApi;

pub struct FullPathApi {
  handle: fraktor_cluster_core_rs::extension::ClusterApi,
}

pub struct RenamedApi {
  grains: Grains,
}

pub struct Cluster {
  inner: CoreCluster,
}

pub struct Generic {
  shared: std::sync::Arc<ClusterApi>,
}

pub struct Clean {
  membership: Membership,
  remote: RemoteApi,
  other: other_crate::extension::RemoteApi,
  globbed: GlobbedApi,
  port: Box<dyn fraktor_cluster_core_rs::extension::ClusterPort>,
  full_not_api: fraktor_cluster_core_rs::extension::Membership,
}
