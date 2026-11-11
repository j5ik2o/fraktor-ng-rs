use crate::rule::Rule;

mod ambiguous_suffix;
mod cfg_std_forbid;
mod mod_file;
mod module_examples;
mod module_wiring;
mod port_adaptor_boundary;
mod rustdoc;
mod tests_location;
mod type_per_file;
mod use_placement;

/// Every rule, in the order the reports list them.
pub(crate) fn all() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(mod_file::ModFile),
        Box::new(module_examples::ModuleExamples),
        Box::new(type_per_file::TypePerFile),
        Box::new(tests_location::TestsLocation),
        Box::new(use_placement::UsePlacement),
        Box::new(ambiguous_suffix::AmbiguousSuffix),
        Box::new(rustdoc::Rustdoc),
        Box::new(module_wiring::ModuleWiring),
        Box::new(port_adaptor_boundary::PortAdaptorBoundary),
        Box::new(cfg_std_forbid::CfgStdForbid),
    ]
}
