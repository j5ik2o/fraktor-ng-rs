use crate::adaptor::support::HelperApi;
use crate::kernel::extension::ClusterIdentityResolver;
use fraktor_actor::adaptor::tokio::TokioApi;
use fraktor_cluster_core_rs::extension::ClusterApi;

pub struct PubSubDeliveryActor {
  identity_resolver: Box<dyn ClusterIdentityResolver>,
  resolver_port: Box<dyn crate::kernel::extension::ClusterIdentityResolver>,
  helper: HelperApi,
  tokio: TokioApi,
  legacy_core: ClusterApi,
  kernel_without_api: crate::kernel::extension::Membership,
}
