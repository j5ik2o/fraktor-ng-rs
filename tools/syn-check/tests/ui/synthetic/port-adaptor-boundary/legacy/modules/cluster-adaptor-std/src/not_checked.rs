use fraktor_cluster_core_rs::extension::ClusterApi;

struct Private {
  api: ClusterApi,
}

pub(crate) struct CrateVisible {
  api: ClusterApi,
}

pub mod nested {
  pub struct InNestedModule {
    api: super::ClusterApi,
  }
}

pub enum NotAStruct {
  Api(ClusterApi),
}
