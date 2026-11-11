use fraktor_cluster_core_rs::extension::ClusterApi;

// syn-check: allow(port-adaptor-boundary) -- 移行中の facade。port に置き換えるまで保持する
pub struct Suppressed {
  first: ClusterApi,
  second: ClusterApi,
}

pub struct NotSuppressed {
  api: ClusterApi,
}
